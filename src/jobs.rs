use crate::{audit::Audit, model::*, runtime};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum State {
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}
impl State {
    fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}
struct Job {
    state: State,
    kind: Option<String>,
    mode: Mode,
    submitted: u64,
    started: Option<u64>,
    finished: Option<u64>,
    deadline_ms: Option<u64>,
    progress: Progress,
    result: Option<RunResult>,
    cancel: CancellationToken,
    done: Arc<Notify>,
}
pub struct Worker {
    pub config: Config,
    pub audit: Arc<Audit>,
    jobs: Mutex<HashMap<String, Job>>,
    lane: Arc<Semaphore>,
    pub paused: AtomicBool,
    stopped: CancellationToken,
    closed: AtomicBool,
    gate: Option<DispatchGate>,
}
pub type DispatchGate = Arc<dyn Fn(&str) -> Outcome<bool> + Send + Sync>;
impl Worker {
    pub fn new(config: Config) -> Outcome<Arc<Self>> {
        Self::with_gate(config, None)
    }
    pub fn with_gate(config: Config, gate: Option<DispatchGate>) -> Outcome<Arc<Self>> {
        let audit = Audit::open(&config)?;
        Self::with_audit(config, gate, audit)
    }
    pub fn with_audit(
        config: Config,
        gate: Option<DispatchGate>,
        audit: Arc<Audit>,
    ) -> Outcome<Arc<Self>> {
        let worker = Arc::new(Self {
            config,
            audit,
            jobs: Mutex::new(HashMap::new()),
            lane: Arc::new(Semaphore::new(1)),
            paused: AtomicBool::new(false),
            stopped: CancellationToken::new(),
            closed: AtomicBool::new(false),
            gate,
        });
        let weak = Arc::downgrade(&worker);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let Some(w) = weak.upgrade() else {
                    break;
                };
                if w.closed.load(Ordering::Relaxed) {
                    break;
                }
                w.prune();
            }
        });
        let weak = Arc::downgrade(&worker);
        let failed = worker.audit.failure_token();
        let stopped = worker.stopped.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = failed.cancelled() => { if let Some(worker) = weak.upgrade() { worker.shutdown().await; } },
                _ = stopped.cancelled() => {},
            }
        });
        Ok(worker)
    }
    fn prune(&self) {
        if self.config.retention_seconds == 0 {
            return;
        }
        let keep = self.config.retention_seconds * 1000;
        let current = now();
        self.jobs.lock().unwrap().retain(|_, j| {
            !j.finished
                .is_some_and(|t| current.saturating_sub(t) >= keep)
        });
    }
    pub fn submit(self: &Arc<Self>, input: Submission) -> Outcome<Value> {
        self.submit_at(input, uuid::Uuid::new_v4().to_string(), now())
    }
    pub fn submit_at(
        self: &Arc<Self>,
        mut input: Submission,
        id: String,
        submitted: u64,
    ) -> Outcome<Value> {
        input.resolve_mode();
        input.resolve_root()?;
        input.validate()?;
        if self.closed.load(Ordering::Relaxed) {
            return Err(Failure::new("SERVER_CLOSED", "Server is shutting down."));
        }
        if self.paused.load(Ordering::Relaxed) {
            return Err(Failure::new(
                "QUOTA_PAUSED",
                "Dispatch is paused after a quota failure; restart after checking provider limits.",
            ));
        }
        if input.execution_mode == Mode::Host && !self.config.allow_host_execution {
            return Err(Failure::new(
                "HOST_MODE_DISABLED",
                "Host mode is disabled; request isolation or enable host execution in configuration.",
            ));
        }
        input.model(&self.config)?;
        if input.execution_mode == Mode::Host {
            crate::snapshot::allowed_root(&self.config, input.root.as_deref())?;
        }
        self.prune();
        let mut store = self.jobs.lock().unwrap();
        if store.values().filter(|j| !j.state.terminal()).count() >= self.config.max_queue {
            return Err(Failure::new(
                "QUEUE_FULL",
                "Pending-job limit reached; retry after a job finishes.",
            ));
        }
        if store.len() >= self.config.max_jobs {
            return Err(Failure::new(
                "RESULT_STORE_FULL",
                "Retained-job limit reached; wait for expiry.",
            ));
        }
        let seconds = input.timeout_seconds.unwrap_or(self.config.timeout_seconds);
        let deadline_ms = deadline(submitted, seconds)?;
        let deadline = deadline_ms
            .map(|ms| {
                Instant::now()
                    .checked_add(Duration::from_millis(ms.saturating_sub(now())))
                    .ok_or_else(|| Failure::new("INPUT_INVALID", "Deadline cannot be represented."))
            })
            .transpose()?;
        let cancel = CancellationToken::new();
        let done = Arc::new(Notify::new());
        self.audit.record(
            "job.queued",
            json!({"job_id":id,"submission":input,"deadline":deadline_ms}),
        )?;
        self.audit.flush()?;
        store.insert(
            id.clone(),
            Job {
                state: State::Queued,
                kind: input.kind.clone(),
                mode: input.execution_mode,
                submitted,
                started: None,
                finished: None,
                deadline_ms,
                progress: Progress::default(),
                result: None,
                cancel: cancel.clone(),
                done,
            },
        );
        drop(store);
        let this = self.clone();
        let task_id = id.clone();
        tokio::spawn(async move {
            this.run(task_id, input, deadline, cancel).await;
        });
        Ok(
            json!({"job_id":id,"state":"queued","execution_mode":self.jobs.lock().unwrap()[&id].mode,"deadline":deadline_ms}),
        )
    }
    async fn run(
        self: Arc<Self>,
        id: String,
        input: Submission,
        deadline: Option<Instant>,
        cancel: CancellationToken,
    ) {
        let model = input.model(&self.config).unwrap_or_default();
        let result = self.run_inner(&id, &input, deadline, &cancel).await;
        let mut result = match result {
            Ok(r) => r,
            Err(e) => {
                let mut r = RunResult::empty(model);
                r.error = Some(e);
                r
            }
        };
        if self.audit.has_failed() {
            result.error = Some(Failure::new(
                "AUDIT_FAILED",
                "Full audit logging failed; execution was stopped.",
            ));
        } else if cancel.is_cancelled() {
            result.error = Some(Failure::new("CANCELLED", "Task was cancelled."));
        }
        let quota = result
            .error
            .as_ref()
            .is_some_and(|e| e.code == "QUOTA_EXHAUSTED");
        if quota {
            self.paused.store(true, Ordering::Relaxed);
        }
        if let Err(error) = self
            .audit
            .record("job.finished", json!({"job_id":id,"result":result}))
            .and_then(|_| self.audit.close_scope(&format!("job-{id}")))
            .and_then(|_| self.audit.flush())
        {
            result.error = Some(error);
        }
        let mut store = self.jobs.lock().unwrap();
        if let Some(j) = store.get_mut(&id) {
            j.state = match result.error.as_ref().map(|e| e.code.as_str()) {
                Some("CANCELLED") => State::Cancelled,
                Some(_) => State::Failed,
                None => State::Completed,
            };
            j.result = Some(result);
            j.finished = Some(now());
            j.done.notify_waiters();
        }
    }
    async fn run_inner(
        self: &Arc<Self>,
        id: &str,
        input: &Submission,
        deadline: Option<Instant>,
        cancel: &CancellationToken,
    ) -> Outcome<RunResult> {
        let _local = tokio::select! {
            permit = self.lane.clone().acquire_owned() => permit.map_err(io_failure)?,
            _ = cancel.cancelled() => return Err(Failure::new("CANCELLED", "Queued task was cancelled.")),
            _ = runtime::wait_timeout(deadline.map(|d| d.saturating_duration_since(Instant::now()))) => return Err(Failure::new("TIMEOUT", "Task deadline elapsed in the queue.")),
        };
        let _lock = loop {
            if cancel.is_cancelled() {
                return Err(Failure::new("CANCELLED", "Queued task was cancelled."));
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Err(Failure::new(
                    "TIMEOUT",
                    "Task deadline elapsed while waiting for the execution lock.",
                ));
            }
            if self.paused.load(Ordering::Relaxed) {
                return Err(Failure::new(
                    "QUOTA_PAUSED",
                    "Task was not dispatched after a quota failure.",
                ));
            }
            if self
                .gate
                .as_ref()
                .map(|gate| gate(id))
                .transpose()?
                .unwrap_or(true)
                && let Some(file) = runtime::try_lock(&self.config)?
                && self
                    .gate
                    .as_ref()
                    .map(|gate| gate(id))
                    .transpose()?
                    .unwrap_or(true)
            {
                break file;
            }
            tokio::select! { _ = cancel.cancelled() => {}, _ = tokio::time::sleep(Duration::from_millis(250)) => {} }
        };
        if input.execution_mode != Mode::Host {
            runtime::check().await?;
        }
        if let Some(j) = self.jobs.lock().unwrap().get_mut(id) {
            j.state = State::Running;
            j.started = Some(now());
        }
        self.audit.record(
            "job.running",
            json!({"job_id":id,"execution_mode":input.execution_mode}),
        )?;
        let weak = Arc::downgrade(self);
        let task_id = id.to_string();
        let progress: runtime::ProgressCallback = Arc::new(move |v| {
            if let Some(w) = weak.upgrade()
                && let Some(j) = w.jobs.lock().unwrap().get_mut(&task_id)
            {
                j.progress = v;
            }
        });
        let remaining = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        if remaining.is_some_and(|d| d.is_zero()) {
            return Err(Failure::new(
                "TIMEOUT",
                "Task deadline elapsed before dispatch.",
            ));
        }
        let capture = self.audit.capture(format!("job-{id}"));
        let result =
            runtime::execute(&self.config, input, remaining, cancel, progress, capture).await;
        if result
            .error
            .as_ref()
            .is_some_and(|e| e.code == "QUOTA_EXHAUSTED")
        {
            self.paused.store(true, Ordering::Relaxed);
        }
        Ok(result)
    }
    pub fn status(&self, id: &str) -> Outcome<Value> {
        self.prune();
        let store = self.jobs.lock().unwrap();
        let j = store.get(id).ok_or_else(not_found)?;
        Ok(
            json!({"job_id":id,"state":j.state,"kind":j.kind,"execution_mode":j.mode,"submitted_at":j.submitted,"started_at":j.started,"finished_at":j.finished,"deadline":j.deadline_ms,
            "progress":j.progress,"completion":if j.state==State::Completed{"complete"}else if j.state.terminal(){"incomplete"}else{"pending"},
            "verification_status":"unverified","error":j.result.as_ref().and_then(|r|r.error.as_ref()),"dispatch_paused":self.paused.load(Ordering::Relaxed)}),
        )
    }
    pub fn result(&self, id: &str, offset: usize, limit: usize, patch: bool) -> Outcome<Value> {
        if limit == 0 || limit > 16000 {
            return Err(Failure::new(
                "INPUT_INVALID",
                "Result page limit must be between 1 and 16000.",
            ));
        }
        self.prune();
        let store = self.jobs.lock().unwrap();
        let j = store.get(id).ok_or_else(not_found)?;
        if !j.state.terminal() {
            return Ok(json!({"job_id":id,"state":j.state,"ready":false}));
        }
        let r = j.result.as_ref().unwrap();
        result_page(
            id,
            &self.status_unlocked(id, j),
            r,
            self.config.audit_logging,
            self.audit.directory.as_deref(),
            (self.config.retention_seconds != 0)
                .then(|| j.finished.unwrap_or_default() + self.config.retention_seconds * 1000),
            offset,
            limit,
            patch,
        )
    }
    fn status_unlocked(&self, id: &str, j: &Job) -> Value {
        json!({"job_id":id,"state":j.state,"finished_at":j.finished})
    }
    pub fn export_result(&self, id: &str) -> Outcome<Option<RunResult>> {
        Ok(self
            .jobs
            .lock()
            .unwrap()
            .get(id)
            .ok_or_else(not_found)?
            .result
            .clone())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn result_page(
    id: &str,
    status: &Value,
    r: &RunResult,
    audit_logging: bool,
    audit_directory: Option<&std::path::Path>,
    expires_at: Option<u64>,
    offset: usize,
    limit: usize,
    patch: bool,
) -> Outcome<Value> {
    if limit == 0 || limit > 16000 {
        return Err(Failure::new(
            "INPUT_INVALID",
            "Result page limit must be between 1 and 16000.",
        ));
    }
    let text = if patch {
        r.patch.as_deref().unwrap_or_default()
    } else {
        &r.response
    };
    let total = text.chars().count();
    let page: String = text.chars().skip(offset).take(limit).collect();
    Ok(
        json!({"job_id":id,"state":status["state"],"ready":true,"completion":if status["state"]=="completed"{"complete"}else{"incomplete"},"verification_status":"unverified",
            "model":r.model,"actual_model":r.actual_model,"cli_status":r.cli_status,"usage":r.usage,"error":r.error,
            "exit_code":r.exit_code,"duration_ms":r.duration_ms,"audit_logging":audit_logging,"audit_directory":audit_directory,"audit_stream_prefix":audit_logging.then(||format!("job-{id}")),
            "manifest":if offset==0{r.manifest.clone()}else{vec![]},
            "section":if patch{"patch"}else{"response"},"text":page,"next_offset":if offset.saturating_add(limit)<total{Some(offset+limit)}else{None},"total_characters":total,
            "patch_available":r.patch.as_ref().is_some_and(|p|!p.is_empty()),"patch_truncated":r.patch_truncated,"expires_at":expires_at}),
    )
}
impl Worker {
    pub async fn cancel(&self, id: &str) -> Outcome<Value> {
        let _ = self
            .audit
            .record("job.cancel_requested", json!({"job_id":id}));
        let done = {
            let mut store = self.jobs.lock().unwrap();
            let j = store.get_mut(id).ok_or_else(not_found)?;
            if !j.state.terminal() {
                if j.state == State::Running {
                    j.state = State::Cancelling;
                }
                j.cancel.cancel();
            }
            j.done.clone()
        };
        loop {
            let notified = done.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self
                .jobs
                .lock()
                .unwrap()
                .get(id)
                .is_none_or(|j| j.state.terminal())
            {
                break;
            }
            notified.await;
        }
        let mut status = self.status(id)?;
        status["process_stopped"] = true.into();
        Ok(status)
    }
    pub async fn capabilities(&self) -> Outcome<Value> {
        let prefix = format!("probe-{}", uuid::Uuid::new_v4());
        let capture = self.audit.capture(prefix.clone());
        let result = runtime::capabilities(&self.config, &self.stopped, capture).await;
        self.audit.close_scope(&prefix)?;
        let mut v = result?;
        v["audit_logging"] = self.config.audit_logging.into();
        v["audit_connection_directory"] =
            serde_json::to_value(&self.audit.directory).map_err(io_failure)?;
        v["audit_rotation_bytes"] = self.config.audit_rotate_bytes.into();
        v["audit_auto_delete"] = false.into();
        v["dispatch_paused"] = self.paused.load(Ordering::Relaxed).into();
        v["retained_jobs"] = self.jobs.lock().unwrap().len().into();
        v["isolation_installed"] =
            (cfg!(target_os = "linux") && std::path::Path::new("/usr/bin/bwrap").exists()).into();
        Ok(v)
    }
    pub async fn shutdown(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.stopped.cancel();
        let _ = self.audit.record(
            "connection.shutdown",
            json!({"audit_failed":self.audit.has_failed()}),
        );
        let ids: Vec<_> = self
            .jobs
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, j)| !j.state.terminal())
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.cancel(&id).await;
        }
    }
}
pub fn deadline(submitted: u64, seconds: u64) -> Outcome<Option<u64>> {
    if seconds == 0 {
        return Ok(None);
    }
    submitted
        .checked_add(
            seconds
                .checked_mul(1000)
                .ok_or_else(|| Failure::new("INPUT_INVALID", "Deadline cannot be represented."))?,
        )
        .map(Some)
        .ok_or_else(|| Failure::new("INPUT_INVALID", "Deadline cannot be represented."))
}
fn not_found() -> Failure {
    Failure::new(
        "JOB_NOT_FOUND",
        "Unknown or expired job; jobs belong to the connection that submitted them.",
    )
}
