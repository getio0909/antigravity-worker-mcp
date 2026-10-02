use crate::{model::*, platform};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

struct Sink {
    file: File,
    index: u64,
    bytes: usize,
}
pub struct Audit {
    pub directory: Option<PathBuf>,
    rotate_bytes: usize,
    sinks: Mutex<HashMap<String, Sink>>,
    sequence: AtomicU64,
    failed: CancellationToken,
}
#[derive(Clone)]
pub struct Capture {
    audit: Arc<Audit>,
    prefix: String,
}
fn failure() -> Failure {
    Failure::new(
        "AUDIT_FAILED",
        "Full audit logging failed; execution was stopped. Check private storage and free space.",
    )
}
fn private_directory(path: &Path) -> Outcome<()> {
    if !path.exists() {
        fs::create_dir_all(path).map_err(|_| failure())?;
        platform::private_mode(path, 0o700)?;
    }
    let meta = fs::symlink_metadata(path).map_err(|_| failure())?;
    if !meta.is_dir() || meta.file_type().is_symlink() || !platform::is_private(&meta) {
        return Err(Failure::new(
            "AUDIT_UNSAFE",
            "Audit directories must be private and must not be symlinks or reparse points.",
        ));
    }
    Ok(())
}
impl Audit {
    pub fn open(config: &Config) -> Outcome<Arc<Self>> {
        let directory = if config.audit_logging {
            private_directory(&config.audit_directory)?;
            let path = config
                .audit_directory
                .join(format!("connection-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).map_err(|_| failure())?;
            platform::private_mode(&path, 0o700)?;
            Some(path)
        } else {
            None
        };
        let audit = Arc::new(Self {
            directory,
            rotate_bytes: config.audit_rotate_bytes,
            sinks: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(0),
            failed: CancellationToken::new(),
        });
        audit.record("connection.open", json!({"version":env!("CARGO_PKG_VERSION"),"pid":std::process::id(),"platform":std::env::consts::OS,"audit_logging":config.audit_logging,"rotation_bytes":config.audit_rotate_bytes}))?;
        Ok(audit)
    }
    pub fn failure_token(&self) -> CancellationToken {
        self.failed.clone()
    }
    pub fn supervisor_diagnostics(&self, id: &str) -> Outcome<std::process::Stdio> {
        let Some(directory) = &self.directory else {
            return Ok(std::process::Stdio::null());
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file =
            self.checked(options.open(directory.join(format!("job-{id}.supervisor.stderr.log"))))?;
        Ok(file.into())
    }
    pub fn has_failed(&self) -> bool {
        self.failed.is_cancelled()
    }
    fn checked<T>(&self, result: io::Result<T>) -> Outcome<T> {
        result.map_err(|_| {
            self.failed.cancel();
            failure()
        })
    }
    fn new_sink(&self, label: &str, index: u64) -> io::Result<Sink> {
        let extension = if label == "events" { "jsonl" } else { "log" };
        let path = self
            .directory
            .as_ref()
            .unwrap()
            .join(format!("{label}.{index:06}.{extension}"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        Ok(Sink {
            file,
            index,
            bytes: 0,
        })
    }
    fn write(&self, label: &str, mut bytes: &[u8], whole_record: bool) -> Outcome<()> {
        if self.directory.is_none() {
            return Ok(());
        }
        if self.has_failed() {
            return Err(failure());
        }
        if label.is_empty()
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
        {
            return Err(failure());
        }
        let mut sinks = self.sinks.lock().unwrap();
        if !sinks.contains_key(label) {
            let sink = self.checked(self.new_sink(label, 1))?;
            sinks.insert(label.into(), sink);
        }
        let sink = sinks.get_mut(label).unwrap();
        while !bytes.is_empty() {
            if sink.bytes >= self.rotate_bytes
                || (whole_record
                    && sink.bytes > 0
                    && sink.bytes.saturating_add(bytes.len()) > self.rotate_bytes)
            {
                self.checked(sink.file.sync_data())?;
                *sink = self.checked(self.new_sink(label, sink.index + 1))?;
            }
            let count = if whole_record {
                bytes.len()
            } else {
                bytes.len().min(self.rotate_bytes - sink.bytes)
            };
            self.checked(sink.file.write_all(&bytes[..count]))?;
            sink.bytes += count;
            bytes = &bytes[count..];
        }
        Ok(())
    }
    pub fn record(&self, event: &str, data: Value) -> Outcome<()> {
        if self.directory.is_none() {
            return Ok(());
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut line = serde_json::to_vec(&json!({"schema":1,"sequence":self.sequence.fetch_add(1,Ordering::SeqCst),"timestamp_ms":timestamp,"event":event,"data":data})).map_err(|_| failure())?;
        line.push(b'\n');
        self.write("events", &line, true)
    }
    pub fn bytes(&self, label: &str, bytes: &[u8]) -> Outcome<()> {
        if self.directory.is_none() || bytes.is_empty() {
            return Ok(());
        }
        self.write(label, bytes, false)?;
        self.record("stream.bytes", json!({"stream":label,"bytes":bytes.len()}))
    }
    pub fn flush(&self) -> Outcome<()> {
        for sink in self.sinks.lock().unwrap().values() {
            self.checked(sink.file.sync_data())?;
        }
        Ok(())
    }
    pub fn close_scope(&self, prefix: &str) -> Outcome<()> {
        let mut sinks = self.sinks.lock().unwrap();
        let names: Vec<_> = sinks
            .keys()
            .filter(|k| k.starts_with(&format!("{prefix}.")))
            .cloned()
            .collect();
        for name in names {
            if let Some(sink) = sinks.remove(&name) {
                self.checked(sink.file.sync_data())?;
            }
        }
        Ok(())
    }
    pub fn capture(self: &Arc<Self>, prefix: String) -> Capture {
        Capture {
            audit: self.clone(),
            prefix,
        }
    }
    pub fn reader<R>(self: &Arc<Self>, inner: R) -> AuditReader<R> {
        AuditReader {
            inner,
            audit: self.clone(),
        }
    }
    pub fn writer<W>(self: &Arc<Self>, inner: W) -> AuditWriter<W> {
        AuditWriter {
            inner,
            audit: self.clone(),
        }
    }
}
impl Capture {
    pub fn nested(&self, name: &str) -> Self {
        Self {
            audit: self.audit.clone(),
            prefix: format!("{}.{}", self.prefix, name),
        }
    }
    pub fn record(&self, event: &str, data: Value) -> Outcome<()> {
        self.audit
            .record(event, json!({"scope":self.prefix,"details":data}))
    }
    pub fn bytes(&self, stream: &str, bytes: &[u8]) -> Outcome<()> {
        self.audit
            .bytes(&format!("{}.{}", self.prefix, stream), bytes)
    }
    pub fn failure_token(&self) -> CancellationToken {
        self.audit.failure_token()
    }
}
pub struct AuditReader<R> {
    inner: R,
    audit: Arc<Audit>,
}
impl<R: AsyncRead + Unpin> AsyncRead for AuditReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buffer.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buffer) {
            Poll::Ready(Ok(())) => {
                match this.audit.bytes("mcp.stdin", &buffer.filled()[before..]) {
                    Ok(()) => Poll::Ready(Ok(())),
                    Err(e) => Poll::Ready(Err(io::Error::other(e.message))),
                }
            }
            other => other,
        }
    }
}
pub struct AuditWriter<W> {
    inner: W,
    audit: Arc<Audit>,
}
impl<W: AsyncWrite + Unpin> AsyncWrite for AuditWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_write(cx, buffer) {
            Poll::Ready(Ok(count)) => match this.audit.bytes("mcp.stdout", &buffer[..count]) {
                Ok(()) => Poll::Ready(Ok(count)),
                Err(e) => Poll::Ready(Err(io::Error::other(e.message))),
            },
            other => other,
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
