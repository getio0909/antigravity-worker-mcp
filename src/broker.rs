use crate::{
    audit::Audit,
    jobs::{self, Worker},
    model::*,
    platform,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, process::Command};

const RECORD_LIMIT: u64 = 16_777_216;
const PACKET_LIMIT: u64 = 1_048_576;

#[derive(Clone, Deserialize, Serialize)]
struct Record {
    schema: u32,
    status: Value,
    fingerprint: String,
    supervisor_ready: bool,
    audit_logging: bool,
    audit_directory: Option<PathBuf>,
    expires_at: Option<u64>,
    expired: bool,
}
impl Record {
    fn terminal(&self) -> bool {
        matches!(
            self.status["state"].as_str(),
            Some("completed" | "failed" | "cancelled")
        )
    }
    fn id(&self) -> &str {
        self.status["job_id"].as_str().unwrap_or_default()
    }
}

#[derive(Deserialize, Serialize)]
struct Packet {
    config: Config,
    submission: Submission,
    job_id: String,
    submitted_at: u64,
}

#[derive(Clone)]
pub struct Broker {
    config: Config,
    executable: PathBuf,
    pub audit: Arc<Audit>,
}
impl Broker {
    pub fn new(config: Config, executable: PathBuf, audit: Arc<Audit>) -> Outcome<Arc<Self>> {
        private_directory(&config.state_directory.join("jobs"))?;
        private_directory(&config.state_directory.join("idempotency"))?;
        Ok(Arc::new(Self {
            config,
            executable,
            audit,
        }))
    }
    fn job_dir(&self, id: &str) -> Outcome<PathBuf> {
        if uuid::Uuid::parse_str(id).is_err()
            || uuid::Uuid::parse_str(id).unwrap().to_string() != id
        {
            return Err(Failure::new(
                "INPUT_INVALID",
                "Job ID must be a canonical UUID.",
            ));
        }
        let path = self.config.state_directory.join("jobs").join(id);
        if !path.exists() {
            return Err(Failure::new(
                "JOB_NOT_FOUND",
                "Unknown job in this state directory.",
            ));
        }
        private_directory(&path)?;
        Ok(path)
    }
    fn read_record(&self, id: &str) -> Outcome<Record> {
        read_json(&self.job_dir(id)?.join("status.json"))
    }
    async fn admission(&self) -> Outcome<File> {
        acquire(&self.config.state_directory.join("admission.lock")).await
    }
    fn records(&self) -> Outcome<Vec<Record>> {
        let mut records = vec![];
        for item in fs::read_dir(self.config.state_directory.join("jobs")).map_err(io_failure)? {
            let item = item.map_err(io_failure)?;
            let name = item.file_name().to_string_lossy().into_owned();
            if uuid::Uuid::parse_str(&name).is_ok() {
                records.push(self.read_record(&name)?);
            }
        }
        Ok(records)
    }
    fn reconcile(&self, id: &str) -> Outcome<Record> {
        let path = self.job_dir(id)?;
        let Some(_transition) = platform::lock_path(&path.join("transition.lock"))? else {
            return self.read_record(id);
        };
        let mut record = self.read_record(id)?;
        let starting = !record.supervisor_ready
            && jobs::now()
                .saturating_sub(record.status["submitted_at"].as_u64().unwrap_or_default())
                < 10_000;
        if !record.terminal()
            && !starting
            && let Some(_runner) = platform::lock_path(&path.join("supervisor.lock"))?
        {
            let mut result =
                RunResult::empty(record.status["model"].as_str().unwrap_or_default().into());
            result.error = Some(Failure::new(
                "RUNNER_INTERRUPTED",
                "Supervisor exited without a terminal checkpoint; work was not replayed. Host effects may remain.",
            ));
            atomic_json(&path.join("result.json"), &result)?;
            record.status["state"] = "failed".into();
            record.status["completion"] = "incomplete".into();
            record.status["finished_at"] = jobs::now().into();
            record.status["error"] = serde_json::to_value(&result.error).map_err(io_failure)?;
            record.status["process_stopped"] = false.into();
            record.expires_at = expiry(jobs::now(), self.config.retention_seconds);
            atomic_json(&path.join("status.json"), &record)?;
        }
        if record.terminal()
            && !record.expired
            && record.expires_at.is_some_and(|t| jobs::now() >= t)
        {
            record.expired = true;
            atomic_json(&path.join("status.json"), &record)?;
            if path.join("result.json").exists() {
                fs::remove_file(path.join("result.json")).map_err(io_failure)?;
            }
        }
        Ok(record)
    }
    pub async fn submit(&self, mut input: Submission) -> Outcome<Value> {
        input.resolve_mode();
        input.resolve_root()?;
        input.validate()?;
        if input.execution_mode == Mode::Host && !self.config.allow_host_execution {
            return Err(Failure::new(
                "HOST_MODE_DISABLED",
                "Host execution is disabled in configuration.",
            ));
        }
        if input.execution_mode == Mode::Host {
            crate::snapshot::allowed_root(&self.config, input.root.as_deref())?;
        }
        let model = input.model(&self.config)?;
        let submitted = jobs::now();
        let seconds = input.timeout_seconds.unwrap_or(self.config.timeout_seconds);
        let deadline = jobs::deadline(submitted, seconds)?;
        if deadline.is_some_and(|ms| {
            std::time::Instant::now()
                .checked_add(Duration::from_millis(ms.saturating_sub(submitted)))
                .is_none()
        }) {
            return Err(Failure::new(
                "INPUT_INVALID",
                "Deadline cannot be represented.",
            ));
        }
        let key = input.idempotency_key.take().map(|v| digest(v.as_bytes()));
        let fingerprint = digest(&serde_json::to_vec(&json!({"submission":input,"model":model,"timeout_seconds":seconds,"agy_path":self.config.agy_path})).map_err(io_failure)?);
        let _admission = self.admission().await?;
        if let Some(key) = &key {
            let index = self
                .config
                .state_directory
                .join("idempotency")
                .join(format!("{key}.json"));
            if index.exists() {
                let id: String = read_json(&index)?;
                let record = self.reconcile(&id)?;
                if record.fingerprint != fingerprint {
                    return Err(Failure::new(
                        "IDEMPOTENCY_CONFLICT",
                        "This key already identifies a different submission.",
                    ));
                }
                return Ok(submission_reply(&record, true));
            }
        }
        if self.config.state_directory.join("dispatch.pause").exists() {
            return Err(Failure::new(
                "QUOTA_PAUSED",
                "Shared dispatch is paused after a quota failure; use ag_resume after checking the provider.",
            ));
        }
        let mut pending = 0;
        let mut retained = 0;
        for record in self.records()? {
            let record = self.reconcile(record.id())?;
            pending += usize::from(!record.terminal());
            retained += usize::from(!record.expired);
        }
        if pending >= self.config.max_queue {
            return Err(Failure::new(
                "QUEUE_FULL",
                "Shared unfinished-job limit reached.",
            ));
        }
        if retained >= self.config.max_jobs {
            return Err(Failure::new(
                "RESULT_STORE_FULL",
                "Shared retained-job limit reached; discard terminal results with ag_forget or wait for configured expiry.",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let directory = self.config.state_directory.join("jobs").join(&id);
        private_directory(&directory)?;
        let diagnostics = self.audit.supervisor_diagnostics(&id)?;
        let mut record = Record {
            schema: 1,
            status: json!({"job_id":id,"state":"queued","kind":input.kind,"execution_mode":input.execution_mode,"model":model,"submitted_at":submitted,"started_at":null,"finished_at":null,"deadline":deadline,"progress":Progress::default(),"completion":"pending","verification_status":"unverified","error":null,"process_stopped":false}),
            fingerprint,
            supervisor_ready: false,
            audit_logging: self.config.audit_logging,
            audit_directory: None,
            expires_at: None,
            expired: false,
        };
        self.audit.record(
            "job.reserved",
            json!({"job_id":id,"submission":input,"deadline":deadline}),
        )?;
        self.audit.flush()?;
        atomic_json(&directory.join("status.json"), &record)?;
        if let Some(key) = &key {
            atomic_json(
                &self
                    .config
                    .state_directory
                    .join("idempotency")
                    .join(format!("{key}.json")),
                &id,
            )?;
        }
        let packet = Packet {
            config: self.config.clone(),
            submission: input,
            job_id: id.clone(),
            submitted_at: submitted,
        };
        let bytes = serde_json::to_vec(&packet).map_err(io_failure)?;
        let mut command = Command::new(&self.executable);
        command
            .arg("--run-job")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(diagnostics)
            .kill_on_drop(false);
        #[cfg(unix)]
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        #[cfg(windows)]
        command.creation_flags(0x01000208);
        let launched = match command.spawn() {
            Ok(mut child) => {
                let mut stdin = child.stdin.take().unwrap();
                let delivery =
                    tokio::time::timeout(Duration::from_secs(5), stdin.write_all(&bytes)).await;
                drop(stdin);
                Ok((
                    platform::Supervisor::Direct(Box::new(child)),
                    matches!(delivery, Ok(Ok(()))),
                ))
            }
            Err(cause) => {
                let error = Failure {
                    code: "BACKGROUND_UNAVAILABLE".into(),
                    message: format!(
                        "Cannot launch an independent supervisor in this host session (OS error {}).",
                        cause
                            .raw_os_error()
                            .map_or_else(|| "unavailable".into(), |code| code.to_string())
                    ),
                };
                #[cfg(windows)]
                if cause.raw_os_error() == Some(5) && self.config.windows_desktop_fallback {
                    crate::windows_background::launch(
                        &self.executable,
                        &bytes,
                        self.audit.capture(format!("job-{id}.desktop-launch")),
                    )
                    .await
                    .map(|child| (platform::Supervisor::Desktop(child), true))
                } else {
                    Err(error)
                }
                #[cfg(not(windows))]
                Err(error)
            }
        };
        let (mut child, delivered) = match launched {
            Ok(child) => child,
            Err(error) => {
                record = self.read_record(&id)?;
                if record.terminal() {
                    return Ok(submission_reply(&record, false));
                }
                let mut result = RunResult::empty(model);
                result.error = Some(error.clone());
                atomic_json(&directory.join("result.json"), &result)?;
                record.status["state"] = "failed".into();
                record.status["finished_at"] = jobs::now().into();
                record.status["completion"] = "incomplete".into();
                record.status["error"] = serde_json::to_value(error).map_err(io_failure)?;
                record.status["process_stopped"] = true.into();
                record.expires_at = expiry(jobs::now(), self.config.retention_seconds);
                atomic_json(&directory.join("status.json"), &record)?;
                return Ok(submission_reply(&record, false));
            }
        };
        let end = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            record = self.read_record(&id)?;
            if record.supervisor_ready || record.terminal() || tokio::time::Instant::now() >= end {
                break;
            }
            if child.exited()? {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if !record.supervisor_ready && !record.terminal() {
            let _transition = acquire(&directory.join("transition.lock")).await?;
            record = self.read_record(&id)?;
            if !record.supervisor_ready && !record.terminal() {
                let stopped = child.stop().await.is_ok();
                let mut result = RunResult::empty(model);
                result.error = Some(Failure::new(
                    "SUPERVISOR_STARTUP_FAILED",
                    if delivered {
                        "Supervisor did not complete the startup handshake; no task was dispatched."
                    } else {
                        "Supervisor input delivery failed; no task was dispatched."
                    },
                ));
                atomic_json(&directory.join("result.json"), &result)?;
                record.status["state"] = "failed".into();
                record.status["completion"] = "incomplete".into();
                record.status["error"] = serde_json::to_value(&result.error).map_err(io_failure)?;
                record.status["finished_at"] = jobs::now().into();
                record.status["process_stopped"] = stopped.into();
                record.expires_at = expiry(jobs::now(), self.config.retention_seconds);
                atomic_json(&directory.join("status.json"), &record)?;
            }
        }
        self.audit.record("job.detached",json!({"job_id":id,"supervisor_pid":child.id(),"supervisor_ready":record.supervisor_ready,"launch_route":child.route()}))?;
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        Ok(submission_reply(&record, false))
    }
    pub fn status(&self, id: &str) -> Outcome<Value> {
        let record = self.reconcile(id)?;
        let mut status = record.status;
        status["supervisor_ready"] = record.supervisor_ready.into();
        status["result_expired"] = record.expired.into();
        status["expires_at"] = record.expires_at.into();
        status["dispatch_paused"] = self
            .config
            .state_directory
            .join("dispatch.pause")
            .exists()
            .into();
        status["background"] = true.into();
        Ok(status)
    }
    pub fn result(&self, id: &str, offset: usize, limit: usize, patch: bool) -> Outcome<Value> {
        if limit == 0 || limit > 16000 {
            return Err(Failure::new(
                "INPUT_INVALID",
                "Result page limit must be between 1 and 16000.",
            ));
        }
        let record = self.reconcile(id)?;
        if record.expired {
            return Err(Failure::new(
                "RESULT_EXPIRED",
                "The result expired; job metadata and enabled audit logs remain.",
            ));
        }
        if !record.terminal() {
            return Ok(
                json!({"job_id":id,"state":record.status["state"],"ready":false,"background":true}),
            );
        }
        let result: RunResult =
            read_json(&self.job_dir(id)?.join("result.json")).map_err(|error| {
                if self.read_record(id).is_ok_and(|r| r.expired) {
                    Failure::new(
                        "RESULT_EXPIRED",
                        "The result expired; metadata and audit logs remain.",
                    )
                } else {
                    error
                }
            })?;
        let mut result = jobs::result_page(
            id,
            &record.status,
            &result,
            record.audit_logging,
            record.audit_directory.as_deref(),
            record.expires_at,
            offset,
            limit,
            patch,
        )?;
        result["background"] = true.into();
        result["process_stopped"] = record.status["process_stopped"].clone();
        Ok(result)
    }
    pub async fn cancel(&self, id: &str) -> Outcome<Value> {
        let directory = self.job_dir(id)?;
        {
            let _transition = acquire(&directory.join("transition.lock")).await?;
            let record = self.read_record(id)?;
            if !record.terminal() {
                atomic_json(
                    &directory.join("cancel.requested"),
                    &json!({"at":jobs::now()}),
                )?;
                let mut record = record;
                record.status["state"] = "cancelling".into();
                atomic_json(&directory.join("status.json"), &record)?;
                self.audit
                    .record("job.cancel_requested", json!({"job_id":id}))?;
            }
        }
        let end = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let status = self.status(id)?;
            if matches!(
                status["state"].as_str(),
                Some("completed" | "failed" | "cancelled")
            ) || tokio::time::Instant::now() >= end
            {
                return Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    pub fn list(&self, after: Option<&str>, limit: usize) -> Outcome<Value> {
        if limit == 0 || limit > 50 {
            return Err(Failure::new(
                "INPUT_INVALID",
                "List limit must be between 1 and 50.",
            ));
        }
        let mut records = self.records()?;
        records.sort_by_key(|r| {
            (
                r.status["submitted_at"].as_u64().unwrap_or_default(),
                r.id().to_owned(),
            )
        });
        records.reverse();
        let start = if let Some(id) = after {
            records
                .iter()
                .position(|r| r.id() == id)
                .ok_or_else(|| Failure::new("JOB_NOT_FOUND", "Unknown list cursor."))?
                + 1
        } else {
            0
        };
        let ids: Vec<_> = records
            .iter()
            .skip(start)
            .take(limit)
            .map(|r| r.id().to_owned())
            .collect();
        let next = (start + ids.len() < records.len())
            .then(|| ids.last().cloned())
            .flatten();
        let jobs: Vec<_> = ids
            .iter()
            .map(|id| self.status(id))
            .collect::<Outcome<_>>()?;
        Ok(
            json!({"jobs":jobs,"next_after":next,"dispatch_paused":self.config.state_directory.join("dispatch.pause").exists()}),
        )
    }
    pub async fn resume(&self) -> Outcome<Value> {
        let _admission = self.admission().await?;
        let path = self.config.state_directory.join("dispatch.pause");
        if path.exists() {
            fs::remove_file(path).map_err(io_failure)?;
        }
        self.audit
            .record("dispatch.resumed", json!({"at":jobs::now()}))?;
        Ok(json!({"dispatch_paused":false,"replayed_jobs":0}))
    }
    pub async fn forget(&self, id: &str) -> Outcome<Value> {
        let directory = self.job_dir(id)?;
        let _transition = acquire(&directory.join("transition.lock")).await?;
        let mut record = self.read_record(id)?;
        if !record.terminal() {
            return Err(Failure::new(
                "JOB_ACTIVE",
                "Only terminal results can be discarded; cancel active work first.",
            ));
        }
        record.expired = true;
        atomic_json(&directory.join("status.json"), &record)?;
        if directory.join("result.json").exists() {
            fs::remove_file(directory.join("result.json")).map_err(io_failure)?;
        }
        self.audit
            .record("job.result_discarded", json!({"job_id":id}))?;
        Ok(
            json!({"job_id":id,"result_expired":true,"audit_preserved":true,"idempotency_preserved":true}),
        )
    }
    pub async fn probe(&self) -> Outcome<Value> {
        let prefix = format!("probe-{}", uuid::Uuid::new_v4());
        let capture = self.audit.capture(prefix.clone());
        let value =
            crate::runtime::capabilities(&self.config, &self.audit.failure_token(), capture).await;
        self.audit.close_scope(&prefix)?;
        let mut value = value?;
        value["audit_logging"] = self.config.audit_logging.into();
        value["audit_connection_directory"] =
            serde_json::to_value(&self.audit.directory).map_err(io_failure)?;
        value["audit_rotation_bytes"] = self.config.audit_rotate_bytes.into();
        value["audit_auto_delete"] = false.into();
        value["default_timeout_seconds"] = self.config.timeout_seconds.into();
        value["result_retention_seconds"] = self.config.retention_seconds.into();
        value["isolation_installed"] =
            (cfg!(target_os = "linux") && Path::new("/usr/bin/bwrap").exists()).into();
        value["background_jobs"] = true.into();
        value["windows_desktop_fallback"] =
            (cfg!(windows) && self.config.windows_desktop_fallback).into();
        value["cross_connection_lookup"] = true.into();
        value["queue_scope"] = "same stateDirectory on one host".into();
        value["dispatch_paused"] = self
            .config
            .state_directory
            .join("dispatch.pause")
            .exists()
            .into();
        value["retained_jobs"] = self.records()?.iter().filter(|r| !r.expired).count().into();
        value["job_metadata_auto_delete"] = false.into();
        Ok(value)
    }
    fn turn(&self, id: &str) -> Outcome<bool> {
        if self.config.state_directory.join("dispatch.pause").exists() {
            return Ok(false);
        }
        let records = self.records()?;
        let mut records: Vec<_> = records
            .into_iter()
            .map(|r| self.reconcile(r.id()))
            .collect::<Outcome<Vec<_>>>()?
            .into_iter()
            .filter(|r| !r.terminal())
            .collect();
        records.sort_by_key(|r| {
            (
                r.status["submitted_at"].as_u64().unwrap_or_default(),
                r.id().to_owned(),
            )
        });
        Ok(records.first().is_some_and(|r| r.id() == id))
    }
}

pub async fn run_job() -> Outcome<()> {
    let bytes = tokio::task::spawn_blocking(|| {
        let mut bytes = vec![];
        std::io::stdin()
            .take(PACKET_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(io_failure)?;
        if bytes.len() as u64 > PACKET_LIMIT {
            return Err(Failure::new(
                "INPUT_INVALID",
                "Supervisor input exceeds its transport limit.",
            ));
        }
        Ok(bytes)
    })
    .await
    .map_err(io_failure)??;
    run_job_bytes(bytes).await
}
pub async fn run_job_bytes(bytes: Vec<u8>) -> Outcome<()> {
    if bytes.len() as u64 > PACKET_LIMIT {
        return Err(Failure::new(
            "INPUT_INVALID",
            "Supervisor input exceeds its transport limit.",
        ));
    }
    let mut packet: Packet = serde_json::from_slice(&bytes).map_err(io_failure)?;
    packet.config.home = PathBuf::from(
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .ok_or_else(|| io_failure("home"))?,
    );
    let audit = Audit::open(&packet.config)?;
    let broker = Broker::new(
        packet.config.clone(),
        std::env::current_exe().map_err(io_failure)?,
        audit.clone(),
    )?;
    let directory = broker.job_dir(&packet.job_id)?;
    let Some(_supervisor) = platform::lock_path(&directory.join("supervisor.lock"))? else {
        return Err(Failure::new(
            "JOB_ALREADY_RUNNING",
            "A supervisor already owns this job.",
        ));
    };
    {
        let _transition = acquire(&directory.join("transition.lock")).await?;
        let mut record = broker.read_record(&packet.job_id)?;
        if record.terminal() {
            return Ok(());
        }
        record.supervisor_ready = true;
        record.audit_directory = audit.directory.clone();
        atomic_json(&directory.join("status.json"), &record)?;
    }
    let gate_broker = broker.clone();
    let worker = Worker::with_audit(
        packet.config.clone(),
        Some(Arc::new(move |id| gate_broker.turn(id))),
        audit.clone(),
    )?;
    let id = packet.job_id;
    worker.submit_at(packet.submission, id.clone(), packet.submitted_at)?;
    let termination = async {
        #[cfg(unix)]
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_=signal.recv()=>{},_=platform::interrupt()=>{}}
            return;
        }
        platform::interrupt().await;
    };
    tokio::pin!(termination);
    let mut termination_enabled = true;
    let outcome = async {
        loop {
            if directory.join("cancel.requested").exists() {
                let _ = worker.cancel(&id).await;
            }
            let status = worker.status(&id)?;
            let result = worker.export_result(&id)?;
            let _transition = acquire(&directory.join("transition.lock")).await?;
            let mut record = broker.read_record(&id)?;
            if record.terminal() {
                break;
            }
            let model = record.status["model"].clone();
            record.status = status;
            record.status["model"] = model;
            if let Some(result) = result {
                record.status["cancel_requested"] =
                    directory.join("cancel.requested").exists().into();
                if result
                    .error
                    .as_ref()
                    .is_some_and(|e| e.code == "QUOTA_EXHAUSTED")
                {
                    let _admission = broker.admission().await?;
                    atomic_json(
                        &packet.config.state_directory.join("dispatch.pause"),
                        &json!({"job_id":id,"at":jobs::now()}),
                    )?;
                }
                atomic_json(&directory.join("result.json"), &result)?;
                record.status["process_stopped"] = true.into();
                record.expires_at = expiry(jobs::now(), packet.config.retention_seconds);
                atomic_json(&directory.join("status.json"), &record)?;
                break;
            }
            atomic_json(&directory.join("status.json"), &record)?;
            drop(_transition);
            tokio::select! {
                _=tokio::time::sleep(Duration::from_millis(250))=>{},
                _=&mut termination, if termination_enabled=>{termination_enabled=false;let _=worker.cancel(&id).await;},
            }
        }
        Ok::<(), Failure>(())
    }
    .await;
    worker.shutdown().await;
    if let Err(error) = &outcome {
        let _ = audit.record("supervisor.failure", json!({"job_id":id,"error":error}));
        let _ = audit.flush();
    }
    outcome?;
    audit.record("supervisor.close", json!({"job_id":id}))?;
    audit.flush()?;
    Ok(())
}

fn submission_reply(record: &Record, deduplicated: bool) -> Value {
    json!({"job_id":record.id(),"state":record.status["state"],"execution_mode":record.status["execution_mode"],"deadline":record.status["deadline"],"background":true,"supervisor_ready":record.supervisor_ready,"deduplicated":deduplicated,"result_expired":record.expired})
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn expiry(at: u64, seconds: u64) -> Option<u64> {
    (seconds != 0).then(|| at.saturating_add(seconds.saturating_mul(1000)))
}
async fn acquire(path: &Path) -> Outcome<File> {
    let end = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(file) = platform::lock_path(path)? {
            return Ok(file);
        }
        if tokio::time::Instant::now() >= end {
            return Err(Failure::new(
                "STATE_BUSY",
                "A state transition is busy; retry this control operation.",
            ));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn private_directory(path: &Path) -> Outcome<()> {
    if !path.exists() {
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(windows)]
        let builder = fs::DirBuilder::new();
        builder
            .create(path)
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(e)
                }
            })
            .map_err(io_failure)?;
    }
    let meta = fs::symlink_metadata(path).map_err(io_failure)?;
    if !meta.is_dir() || meta.file_type().is_symlink() || !platform::is_private(&meta) {
        return Err(Failure::new(
            "STATE_UNSAFE",
            "Job storage must be private, without symlinks or reparse points.",
        ));
    }
    Ok(())
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Outcome<T> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|e| checkpoint_error("open", e))?;
    let meta = file
        .metadata()
        .map_err(|e| checkpoint_error("metadata", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(Failure::new(
                "STATE_UNSAFE",
                "Job state cannot be hard-linked.",
            ));
        }
    }
    if !meta.is_file() || !platform::is_private(&meta) || meta.len() > RECORD_LIMIT {
        return Err(Failure::new(
            "STATE_UNSAFE",
            "Job state must be a bounded private regular file.",
        ));
    }
    serde_json::from_reader(file.take(RECORD_LIMIT + 1)).map_err(|error| Failure {
        code: "IO_FAILED".into(),
        message: format!(
            "Cannot decode a checkpoint (category {:?}, line {}, column {}).",
            error.classify(),
            error.line(),
            error.column()
        ),
    })
}
fn checkpoint_error(operation: &str, error: std::io::Error) -> Failure {
    Failure {
        code: "IO_FAILED".into(),
        message: format!(
            "Checkpoint {operation} failed (OS error {}).",
            error
                .raw_os_error()
                .map_or_else(|| "unavailable".into(), |code| code.to_string())
        ),
    }
}
fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Outcome<()> {
    private_directory(path.parent().ok_or_else(|| io_failure("parent"))?)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".checkpoint-")
        .tempfile_in(path.parent().unwrap())
        .map_err(|e| checkpoint_error("temporary creation", e))?;
    serde_json::to_writer(&mut temporary, value).map_err(io_failure)?;
    temporary
        .flush()
        .map_err(|e| checkpoint_error("flush", e))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| checkpoint_error("sync", e))?;
    let retry_until = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        match temporary.persist(path) {
            Ok(_) => break,
            Err(error) => {
                if cfg!(windows)
                    && matches!(error.error.raw_os_error(), Some(5 | 32 | 33))
                    && std::time::Instant::now() < retry_until
                {
                    // Windows readers or antivirus can briefly prevent atomic replacement.
                    temporary = error.file;
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                return Err(checkpoint_error("replace", error.error));
            }
        }
    }
    #[cfg(unix)]
    File::open(path.parent().unwrap())
        .and_then(|f| f.sync_all())
        .map_err(io_failure)?;
    Ok(())
}
