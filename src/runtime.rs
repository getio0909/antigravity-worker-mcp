use crate::{audit::Capture, model::*, platform, snapshot};
use serde_json::{Value, json};
#[cfg(target_os = "linux")]
use std::os::unix::fs::MetadataExt;
use std::{
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Notify,
};
use tokio_util::sync::CancellationToken;

const MAX_OUTPUT: usize = 2097152;
#[derive(Default)]
pub struct EventParser {
    pending: Vec<u8>,
    pub progress: Progress,
    pub result: Option<Value>,
    pub model: Option<String>,
    pub denied: bool,
}
impl EventParser {
    pub fn push(&mut self, bytes: &[u8]) -> Outcome<()> {
        self.progress.output_bytes += bytes.len();
        if self.progress.output_bytes > MAX_OUTPUT {
            return Err(Failure::new("OUTPUT_LIMIT", "CLI output exceeded 2 MiB."));
        }
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<_> = self.pending.drain(..=end).collect();
            self.line(&line)?;
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Outcome<()> {
        let line = std::mem::take(&mut self.pending);
        self.line(&line)
    }
    fn line(&mut self, bytes: &[u8]) -> Outcome<()> {
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(());
        }
        let v: Value = serde_json::from_slice(bytes)
            .map_err(|_| Failure::new("STREAM_INVALID", "CLI emitted malformed JSON events."))?;
        if !v.is_object() {
            return Err(Failure::new(
                "STREAM_INVALID",
                "CLI event must be an object.",
            ));
        }
        self.progress.events += 1;
        match v["event"].as_str() {
            Some("init") => self.model = v["init"]["model"].as_str().map(String::from),
            Some("step_update") => {
                self.progress.steps += 1;
                if let Some(error) = v["step_update"]["tool_info"].get("error") {
                    let s = error.to_string().to_lowercase();
                    if ["denied", "permission", "not allowed"]
                        .iter()
                        .any(|p| s.contains(p))
                    {
                        self.denied = true;
                    }
                }
            }
            Some("result") => {
                if self.result.is_some() || !v["result"].is_object() {
                    return Err(Failure::new(
                        "STREAM_INVALID",
                        "Expected one result envelope for a single-turn job.",
                    ));
                }
                self.denied |= v["result"]["denied_actions"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty());
                self.result = Some(v["result"].clone());
            }
            _ => {}
        }
        Ok(())
    }
}
pub struct ProcessOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub parser: EventParser,
}
pub type ProgressCallback = Arc<dyn Fn(Progress) + Send + Sync>;

#[allow(clippy::too_many_arguments)]
pub async fn process(
    command: &Path,
    args: &[OsString],
    cwd: Option<&Path>,
    stdin: Option<String>,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: Option<ProgressCallback>,
    inherit_environment: bool,
) -> Outcome<ProcessOutput> {
    process_with_capture(
        command,
        args,
        cwd,
        stdin,
        timeout,
        cancel,
        progress,
        inherit_environment,
        None,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn process_with_capture(
    command: &Path,
    args: &[OsString],
    cwd: Option<&Path>,
    stdin: Option<String>,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: Option<ProgressCallback>,
    inherit_environment: bool,
    capture: Option<Capture>,
) -> Outcome<ProcessOutput> {
    if cancel.is_cancelled() {
        return Err(Failure::new("CANCELLED", "Task was cancelled."));
    }
    if let Some(audit) = &capture {
        audit.record("process.dispatch",json!({"executable":command,"args":args.iter().map(|v|v.to_string_lossy()).collect::<Vec<_>>(),"cwd":cwd,"inherit_environment":inherit_environment}))?;
    }
    let mut cmd = Command::new(command);
    cmd.args(args);
    if !inherit_environment {
        cmd.env_clear();
        cmd.env("HOME", std::env::var_os("HOME").unwrap_or_default())
            .env("PATH", "/usr/bin:/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
    }
    cmd.env("LANG", "C.UTF-8")
        .env("TERM", "dumb")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    for key in [
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    cmd.creation_flags(0x00000200);
    let mut child = cmd.spawn().map_err(|_| {
        Failure::new(
            "EXECUTABLE_UNAVAILABLE",
            "Cannot launch the configured runtime.",
        )
    })?;
    let group = match platform::ProcessGuard::attach(&mut child) {
        Ok(group) => group,
        Err(error) => {
            let _ = child.kill().await;
            return Err(error);
        }
    };
    if let Some(audit) = &capture
        && let Err(error) = audit.record("process.started", json!({"pid":child.id()}))
    {
        group.kill();
        let _ = child.wait().await;
        return Err(error);
    }
    let mut input = child.stdin.take().unwrap();
    let stdin_capture = capture.clone();
    let writer = tokio::spawn(async move {
        if let Some(text) = stdin {
            let mut remaining = text.as_bytes();
            while !remaining.is_empty() {
                let Ok(count) = input.write(remaining).await else {
                    break;
                };
                if count == 0 {
                    break;
                }
                if let Some(audit) = &stdin_capture
                    && audit.bytes("stdin", &remaining[..count]).is_err()
                {
                    break;
                }
                remaining = &remaining[count..];
            }
        }
        drop(input);
    });
    let total = Arc::new(AtomicUsize::new(0));
    let failure = Arc::new(Mutex::new(None::<Failure>));
    let notify = Arc::new(Notify::new());
    let out_task = reader(
        child.stdout.take().unwrap(),
        total.clone(),
        failure.clone(),
        notify.clone(),
        progress,
        true,
        capture.clone(),
    );
    let err_task = reader(
        child.stderr.take().unwrap(),
        total,
        failure.clone(),
        notify.clone(),
        None,
        false,
        capture.clone(),
    );
    let audit_failed = capture
        .as_ref()
        .map_or_else(CancellationToken::new, Capture::failure_token);
    let mut interrupted = None;
    let status = tokio::select! {
        s = child.wait() => s.map_err(io_failure),
        _ = cancel.cancelled() => { interrupted = Some(Failure::new("CANCELLED", "Task was cancelled.")); Err(interrupted.clone().unwrap()) },
        _ = audit_failed.cancelled() => { interrupted = Some(Failure::new("AUDIT_FAILED", "Full audit logging failed; execution was stopped.")); Err(interrupted.clone().unwrap()) },
        _ = tokio::time::sleep(timeout) => { interrupted = Some(Failure::new("TIMEOUT", "Task deadline elapsed; execution was terminated.")); Err(interrupted.clone().unwrap()) },
        _ = notify.notified() => { interrupted = failure.lock().unwrap().clone(); Err(interrupted.clone().unwrap_or_else(|| Failure::new("STREAM_INVALID", "Output failed validation."))) },
    };
    if interrupted.is_some() || status.is_err() {
        group.terminate();
        if tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .is_err()
        {
            group.kill();
            let _ = child.wait().await;
        }
    }
    group.kill();
    let _ = writer.await;
    let out = tokio::time::timeout(Duration::from_secs(3), out_task)
        .await
        .map_err(|_| {
            Failure::new(
                "PROCESS_CLEANUP_FAILED",
                "Output reader did not stop after termination.",
            )
        })?
        .map_err(io_failure)?;
    let err = tokio::time::timeout(Duration::from_secs(3), err_task)
        .await
        .map_err(|_| {
            Failure::new(
                "PROCESS_CLEANUP_FAILED",
                "Diagnostic reader did not stop after termination.",
            )
        })?
        .map_err(io_failure)?;
    if let Some(audit) = &capture {
        audit.record("process.finished",json!({"exit_code":status.as_ref().ok().and_then(|s|s.code()),"interrupted":interrupted,"supervised_process_stopped":true}))?;
    }
    if let Some(f) = interrupted.or_else(|| failure.lock().unwrap().clone()) {
        return Err(f);
    }
    let status = status?;
    Ok(ProcessOutput {
        code: status.code(),
        stdout: String::from_utf8_lossy(&out.0).into_owned(),
        stderr: String::from_utf8_lossy(&err.0).into_owned(),
        parser: out.1,
    })
}
fn reader<R: AsyncRead + Unpin + Send + 'static>(
    mut stream: R,
    total: Arc<AtomicUsize>,
    failure: Arc<Mutex<Option<Failure>>>,
    notify: Arc<Notify>,
    callback: Option<ProgressCallback>,
    stdout: bool,
    capture: Option<Capture>,
) -> tokio::task::JoinHandle<(Vec<u8>, EventParser)> {
    tokio::spawn(async move {
        let mut output = vec![];
        let mut parser = EventParser::default();
        let mut buffer = [0u8; 8192];
        loop {
            let count = match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let bytes = &buffer[..count];
            let audit_error = capture.as_ref().and_then(|a| {
                a.bytes(if stdout { "stdout" } else { "stderr" }, bytes)
                    .err()
            });
            let too_large = total.fetch_add(count, Ordering::Relaxed) + count > MAX_OUTPUT;
            let error = if audit_error.is_some() {
                audit_error
            } else if too_large {
                Some(Failure::new("OUTPUT_LIMIT", "CLI output exceeded 2 MiB."))
            } else if stdout && callback.is_some() {
                parser.push(bytes).err()
            } else {
                None
            };
            if let Some(error) = error {
                let mut f = failure.lock().unwrap();
                if f.is_none() {
                    *f = Some(error);
                    notify.notify_one();
                }
            }
            if let Some(cb) = &callback {
                cb(parser.progress.clone());
            } else if output.len() < if stdout { MAX_OUTPUT } else { 32768 } {
                let remaining = (if stdout { MAX_OUTPUT } else { 32768 }) - output.len();
                output.extend_from_slice(&bytes[..count.min(remaining)]);
            }
        }
        if stdout
            && callback.is_some()
            && let Err(e) = parser.finish()
        {
            let mut f = failure.lock().unwrap();
            if f.is_none() {
                *f = Some(e);
                notify.notify_one();
            }
        }
        (output, parser)
    })
}

pub fn try_lock(config: &Config) -> Outcome<Option<File>> {
    platform::lock(config)
}
#[cfg(target_os = "linux")]
fn arg(args: &mut Vec<OsString>, values: &[&str]) {
    args.extend(values.iter().map(OsString::from));
}
#[cfg(target_os = "linux")]
fn bind(args: &mut Vec<OsString>, flag: &str, source: &Path, target: &Path) {
    args.extend([
        flag.into(),
        source.as_os_str().into(),
        target.as_os_str().into(),
    ]);
}
#[cfg(target_os = "linux")]
pub fn sandbox(
    config: &Config,
    workspace: &Path,
    settings: &Path,
    mode: Mode,
) -> Outcome<Vec<OsString>> {
    let mut args = vec![];
    arg(
        &mut args,
        &[
            "--unshare-user",
            "--unshare-pid",
            "--unshare-ipc",
            "--unshare-uts",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
        ],
    );
    for path in ["/usr", "/bin", "/sbin", "/lib", "/lib64"] {
        if Path::new(path).exists() {
            bind(&mut args, "--ro-bind", Path::new(path), Path::new(path));
        }
    }
    arg(
        &mut args,
        &[
            "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--tmpfs", "/run", "--dir",
            "/etc",
        ],
    );
    for path in [
        "/etc/ssl/certs",
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/nsswitch.conf",
        "/etc/passwd",
        "/etc/group",
        "/etc/gai.conf",
        "/etc/ld.so.cache",
    ] {
        if Path::new(path).exists() {
            bind(&mut args, "--ro-bind", Path::new(path), Path::new(path));
        }
    }
    let profile = config.home.join(".gemini/antigravity-cli");
    args.extend([
        "--dir".into(),
        config.home.as_os_str().into(),
        "--tmpfs".into(),
        config.home.join(".gemini").into_os_string(),
        "--dir".into(),
        profile.as_os_str().into(),
    ]);
    bind(
        &mut args,
        "--ro-bind",
        settings,
        &profile.join("settings.json"),
    );
    bind(
        &mut args,
        if mode == Mode::Analysis {
            "--ro-bind"
        } else {
            "--bind"
        },
        workspace,
        Path::new("/work"),
    );
    bind(
        &mut args,
        "--ro-bind",
        &config.agy_path,
        Path::new("/opt/agy"),
    );
    let token = profile.join("antigravity-oauth-token");
    if token.exists() {
        let s = fs::symlink_metadata(&token).map_err(io_failure)?;
        if !s.is_file() || s.uid() != unsafe { libc::geteuid() } {
            return Err(Failure::new(
                "AUTH_PROFILE_UNSAFE",
                "Authentication must be a regular file owned by this user.",
            ));
        }
        bind(&mut args, "--ro-bind", &token, &token);
    }
    let bin = profile.join("bin");
    if bin.exists() {
        if !fs::symlink_metadata(&bin).map_err(io_failure)?.is_dir() {
            return Err(Failure::new(
                "AUTH_PROFILE_UNSAFE",
                "CLI cache must be a regular directory.",
            ));
        }
        bind(&mut args, "--ro-bind", &bin, &bin);
    }
    for path in &config.runtime_paths {
        bind(&mut args, "--ro-bind", path, path);
    }
    let path = config
        .runtime_paths
        .iter()
        .map(|p| p.join("bin").to_string_lossy().into_owned())
        .chain(["/usr/bin".into(), "/bin".into()])
        .collect::<Vec<_>>()
        .join(":");
    args.extend(["--setenv".into(), "PATH".into(), path.into()]);
    arg(&mut args, &["--chdir", "/work", "--"]);
    Ok(args)
}
fn settings(config: &Config, mode: Mode) -> Value {
    if mode == Mode::Analysis {
        let allow = std::iter::once("read_file(/work)".into())
            .chain(
                config
                    .research_domains
                    .iter()
                    .map(|d| format!("read_url({d})")),
            )
            .collect::<Vec<String>>();
        let mut deny = vec![
            "write_file(*)".into(),
            "command(*)".into(),
            "unsandboxed(*)".into(),
            "execute_url(*)".into(),
            "mcp(*)".into(),
            format!("read_file({})", config.home.display()),
        ];
        if config.research_domains.is_empty() {
            deny.push("read_url(*)".into());
        }
        json!({"toolPermission":"strict","allowNonWorkspaceAccess":false,"enableTelemetry":false,"mcpServers":{},"permissions":{"allow":allow,"deny":deny,"ask":[]}})
    } else {
        json!({"toolPermission":"always-proceed","allowNonWorkspaceAccess":true,"enableTelemetry":false,"mcpServers":{},"permissions":{"allow":["read_file(*)","write_file(*)","command(*)","read_url(*)"],"deny":[],"ask":[]}})
    }
}
#[cfg(target_os = "linux")]
pub async fn check() -> Outcome<()> {
    for path in ["/usr/bin/bwrap", "/usr/bin/git"] {
        if !Path::new(path).exists() {
            return Err(Failure::new(
                "SANDBOX_UNAVAILABLE",
                "Install Bubblewrap and Git before requesting isolated execution.",
            ));
        }
    }
    let args = [
        "--ro-bind",
        "/",
        "/",
        "--unshare-user",
        "--unshare-pid",
        "--die-with-parent",
        "/usr/bin/true",
    ]
    .map(OsString::from);
    let r = process(
        Path::new("/usr/bin/bwrap"),
        &args,
        None,
        None,
        Duration::from_secs(5),
        &CancellationToken::new(),
        None,
        false,
    )
    .await?;
    if r.code != Some(0) {
        return Err(Failure::new(
            "SANDBOX_UNAVAILABLE",
            "Bubblewrap or unprivileged user namespaces are unavailable.",
        ));
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
pub async fn check() -> Outcome<()> {
    Err(Failure::new(
        "ISOLATION_UNSUPPORTED",
        "Isolation requires Linux; use host execution on this platform.",
    ))
}
#[cfg(not(target_os = "linux"))]
pub fn sandbox(_: &Config, _: &Path, _: &Path, _: Mode) -> Outcome<Vec<OsString>> {
    Err(Failure::new(
        "ISOLATION_UNSUPPORTED",
        "Isolation requires Linux; no host fallback is performed.",
    ))
}
pub async fn capabilities(
    config: &Config,
    cancel: &CancellationToken,
    capture: Capture,
) -> Outcome<Value> {
    let args = [OsString::from("--version")];
    let version = process_with_capture(
        &config.agy_path,
        &args,
        None,
        None,
        Duration::from_secs(10),
        cancel,
        None,
        true,
        Some(capture.nested("version")),
    )
    .await?;
    let args = [OsString::from("models")];
    let models = process_with_capture(
        &config.agy_path,
        &args,
        None,
        None,
        Duration::from_secs(20),
        cancel,
        None,
        true,
        Some(capture.nested("models")),
    )
    .await?;
    let catalog: Vec<_> = if models.code == Some(0) {
        models
            .stdout
            .lines()
            .filter_map(|s| {
                let (id, label) = s.split_once(char::is_whitespace)?;
                if !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                {
                    return None;
                }
                Some(json!({"id":id,"label":label.trim()}))
            })
            .take(100)
            .collect()
    } else {
        vec![]
    };
    Ok(
        json!({"version":env!("CARGO_PKG_VERSION"),"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"isolation_supported":cfg!(target_os="linux"),"cli_version":if version.code==Some(0){Some(version.stdout.trim())}else{None},"models":catalog,
        "models_error":if models.code==Some(0){None}else{Some(classify(&models.stderr).code)},"configured_profiles":config.models,
        "execution_modes":supported_modes(config.allow_host_execution),
        "default_execution_mode":"host","default_isolation":false,"skip_permissions":{"workspace":true,"analysis":false,"host":true},"concurrency":1,"lock_scope":"same stateDirectory on one host",
        "quota":{"available":false,"reason":"No stable quota API; token usage is not remaining plan quota."},
        "limits":{"max_queue":config.max_queue,"max_jobs":config.max_jobs,"files":100,"file_bytes":262144,"input_bytes":4194304,"output_bytes":MAX_OUTPUT}}),
    )
}
fn supported_modes(host: bool) -> Vec<&'static str> {
    let mut modes = vec![];
    if cfg!(target_os = "linux") {
        modes.extend(["workspace", "analysis"]);
    }
    if host {
        modes.push("host");
    }
    modes
}
pub async fn execute(
    config: &Config,
    input: &Submission,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: ProgressCallback,
    capture: Capture,
) -> RunResult {
    let started = Instant::now();
    let mut result = RunResult::empty(config.model(input.model_profile).unwrap_or_default());
    if let Err(e) = execute_inner(
        config,
        input,
        timeout,
        cancel,
        progress,
        &mut result,
        capture,
    )
    .await
    {
        result.error = Some(e);
    }
    result.duration_ms = Some(started.elapsed().as_millis() as u64);
    result
}
#[allow(clippy::too_many_arguments)]
async fn execute_inner(
    config: &Config,
    input: &Submission,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: ProgressCallback,
    result: &mut RunResult,
    capture: Capture,
) -> Outcome<()> {
    let dir = tempfile::Builder::new()
        .prefix("job-")
        .tempdir_in(&config.state_directory)
        .map_err(io_failure)?;
    let work = dir.path().join("work");
    result.manifest = snapshot::capture(config, input, &work)?;
    capture.record("input.manifest", json!({"files":result.manifest}))?;
    for (index, entry) in result.manifest.iter().enumerate() {
        let bytes = fs::read(work.join(&entry.path)).map_err(io_failure)?;
        let stream = format!("input-{index:04}");
        capture.bytes(&stream, &bytes)?;
        capture.record(
            "input.snapshot",
            json!({"path":entry.path,"sha256":entry.sha256,"bytes":bytes.len(),"stream":stream}),
        )?;
    }
    if input.execution_mode == Mode::Workspace {
        snapshot::copy_tree(&work, &dir.path().join("base"))?;
    }
    let settings_path = dir.path().join("settings.json");
    if input.execution_mode != Mode::Host {
        fs::write(
            &settings_path,
            settings(config, input.execution_mode).to_string(),
        )
        .map_err(io_failure)?;
    }
    let host_root = if input.execution_mode == Mode::Host {
        Some(snapshot::allowed_root(config, input.root.as_deref())?)
    } else {
        None
    };
    let command;
    let mut args;
    if host_root.is_some() {
        command = config.agy_path.clone();
        args = vec![];
    } else {
        command = PathBuf::from("/usr/bin/bwrap");
        args = sandbox(config, &work, &settings_path, input.execution_mode)?;
        args.push("/opt/agy".into());
    }
    let schema = schemars::schema_for!(Report);
    args.extend([
        "--input-format".into(),
        "stream-json".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--model".into(),
        result.model.clone().into(),
        "--mode".into(),
        if input.execution_mode == Mode::Analysis {
            "plan".into()
        } else {
            "accept-edits".into()
        },
        "--print-timeout".into(),
        format!("{}s", timeout.as_secs().max(1)).into(),
        "--json-schema".into(),
        serde_json::to_string(&schema).map_err(io_failure)?.into(),
    ]);
    if input.execution_mode != Mode::Host {
        args.push("--disable-slash-commands".into());
    }
    if input.execution_mode != Mode::Analysis {
        args.push("--dangerously-skip-permissions".into());
    }
    let request = format!(
        "Task type: {:?}. Execution mode: {:?}.\nWorkspace: {}.\nSelected files: {}\n\nTask:\n{}\n\nFinal report format: JSON matching the supplied schema, with summary (string), findings (array of objects), and limitations (array of strings). Findings contain title, detail, severity (info/low/medium/high), and evidence (array). Evidence entries contain file, line, url, and excerpt; use null for absent file, line, or url. An empty findings array is valid.",
        input.kind,
        input.execution_mode,
        if host_root.is_some() {
            "the configured workspace"
        } else {
            "/work"
        },
        serde_json::to_string(&input.files).unwrap_or_default(),
        input.instructions
    );
    let body = json!({"event":"user","message":{"content":request}}).to_string() + "\n";
    let run = process_with_capture(
        &command,
        &args,
        host_root.as_deref(),
        Some(body),
        timeout,
        cancel,
        Some(progress),
        host_root.is_some(),
        Some(capture.nested("cli")),
    )
    .await?;
    result.exit_code = run.code;
    let envelope = run.parser.result.unwrap_or(Value::Null);
    result.actual_model = run.parser.model;
    result.cli_status = envelope["status"].as_str().map(String::from);
    result.response = envelope["response"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let usage = envelope["usage"].as_object().map(|m| {
        m.iter()
            .filter(|(key, value)| {
                [
                    "input_tokens",
                    "output_tokens",
                    "thinking_tokens",
                    "cache_read_tokens",
                    "total_tokens",
                ]
                .contains(&key.as_str())
                    && value.as_f64().is_some_and(|n| n.is_finite() && n >= 0.)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<serde_json::Map<String, Value>>()
    });
    result.usage = usage.map(Value::Object);
    if run.code != Some(0)
        || envelope["status"] != "SUCCESS"
        || envelope.get("error").is_some_and(|e| !e.is_null())
    {
        return Err(classify(&(envelope["error"].to_string() + &run.stderr)));
    }
    if run.parser.denied
        || ["soft-denied", "permission denied", "tool denied"]
            .iter()
            .any(|p| run.stderr.to_lowercase().contains(p))
    {
        return Err(Failure::new(
            "PERMISSION_DENIED",
            "A required operation was denied; the task is incomplete.",
        ));
    }
    let report = envelope
        .get("structured_output")
        .cloned()
        .unwrap_or_else(|| serde_json::from_str(&result.response).unwrap_or(Value::Null));
    result.report = Some(snapshot::validate_report(
        report,
        &result.manifest,
        input.execution_mode,
    )?);
    result.response = serde_json::to_string(&result.report).map_err(io_failure)?;
    if input.execution_mode == Mode::Workspace {
        let args = [
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "diff",
            "--no-index",
            "--no-ext-diff",
            "--no-textconv",
            "--",
            "base",
            "work",
        ]
        .map(OsString::from);
        let diff = process_with_capture(
            Path::new("/usr/bin/git"),
            &args,
            Some(dir.path()),
            None,
            Duration::from_secs(5),
            cancel,
            None,
            false,
            Some(capture.nested("diff")),
        )
        .await?;
        if !matches!(diff.code, Some(0 | 1)) {
            return Err(Failure::new(
                "PATCH_FAILED",
                "Cannot generate the workspace patch.",
            ));
        }
        let patch = diff
            .stdout
            .replace("a/base/", "a/")
            .replace("b/base/", "b/")
            .replace("a/work/", "a/")
            .replace("b/work/", "b/");
        result.patch_truncated = patch.len() > 1048576;
        result.patch = Some(patch.chars().take(1048576).collect());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_utf8_and_duplicate_result() {
        let bytes =
            json!({"event":"result","result":{"status":"SUCCESS","response":"\u{e9}\u{1f680}"}})
                .to_string()
                + "\n";
        let mut p = EventParser::default();
        for b in bytes.bytes() {
            p.push(&[b]).unwrap();
        }
        p.finish().unwrap();
        assert_eq!(p.result.as_ref().unwrap()["response"], "\u{e9}\u{1f680}");
        assert_eq!(p.push(bytes.as_bytes()).unwrap_err().code, "STREAM_INVALID");
    }
}
