use crate::{model::*, runtime};
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

fn now() -> u64 {
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
    kind: Kind,
    mode: Mode,
    submitted: u64,
    started: Option<u64>,
    finished: Option<u64>,
    deadline_ms: u64,
    progress: Progress,
    result: Option<RunResult>,
    cancel: CancellationToken,
    done: Arc<Notify>,
}
pub struct Worker {
    pub config: Config,
    jobs: Mutex<HashMap<String, Job>>,
    lane: Arc<Semaphore>,
    pub paused: AtomicBool,
    stopped: CancellationToken,
    closed: AtomicBool,
}
impl Worker {
    pub fn new(config: Config) -> Arc<Self> {
        let worker = Arc::new(Self {
            config,
            jobs: Mutex::new(HashMap::new()),
            lane: Arc::new(Semaphore::new(1)),
            paused: AtomicBool::new(false),
            stopped: CancellationToken::new(),
            closed: AtomicBool::new(false),
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
        worker
    }
    fn prune(&self) {
        let keep = self.config.retention_seconds * 1000;
        let current = now();
        self.jobs.lock().unwrap().retain(|_, j| {
            !j.finished
                .is_some_and(|t| current.saturating_sub(t) >= keep)
        });
    }
    pub fn submit(self: &Arc<Self>, mut input: Submission) -> Outcome<Value> {
        input.resolve_mode();
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
        self.config.model(input.model_profile)?;
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
        let id = uuid::Uuid::new_v4().to_string();
        let submitted = now();
        let seconds = input.timeout_seconds.unwrap_or(self.config.timeout_seconds);
        let cancel = CancellationToken::new();
        let done = Arc::new(Notify::new());
        store.insert(
            id.clone(),
            Job {
                state: State::Queued,
                kind: input.kind.clone(),
                mode: input.execution_mode,
                submitted,
                started: None,
                finished: None,
                deadline_ms: submitted + seconds * 1000,
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
            this.run(
                task_id,
                input,
                Instant::now() + Duration::from_secs(seconds),
                cancel,
            )
            .await;
        });
        Ok(
            json!({"job_id":id,"state":"queued","execution_mode":self.jobs.lock().unwrap()[&id].mode,"deadline":submitted+seconds*1000}),
        )
    }
    async fn run(
        self: Arc<Self>,
        id: String,
        input: Submission,
        deadline: Instant,
        cancel: CancellationToken,
    ) {
        let model = self.config.model(input.model_profile).unwrap_or_default();
        let result = self.run_inner(&id, &input, deadline, &cancel).await;
        let mut result = match result {
            Ok(r) => r,
            Err(e) => {
                let mut r = RunResult::empty(model);
                r.error = Some(e);
                r
            }
        };
        if cancel.is_cancelled() {
            result.error = Some(Failure::new("CANCELLED", "Task was cancelled."));
        }
        let quota = result
            .error
            .as_ref()
            .is_some_and(|e| e.code == "QUOTA_EXHAUSTED");
        if quota {
            self.paused.store(true, Ordering::Relaxed);
        }
        let mut store = self.jobs.lock().unwrap();
        if let Some(j) = store.get_mut(&id) {
            j.state = match result.error.as_ref().map(|e| e.code) {
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
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Outcome<RunResult> {
        let _local = tokio::select! {
            permit = self.lane.clone().acquire_owned() => permit.map_err(io_failure)?,
            _ = cancel.cancelled() => return Err(Failure::new("CANCELLED", "Queued task was cancelled.")),
            _ = tokio::time::sleep_until(deadline.into()) => return Err(Failure::new("TIMEOUT", "Task deadline elapsed in the queue.")),
        };
        let _lock = loop {
            if cancel.is_cancelled() {
                return Err(Failure::new("CANCELLED", "Queued task was cancelled."));
            }
            if Instant::now() >= deadline {
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
            if let Some(file) = runtime::try_lock(&self.config)? {
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
        let weak = Arc::downgrade(self);
        let task_id = id.to_string();
        let progress: runtime::ProgressCallback = Arc::new(move |v| {
            if let Some(w) = weak.upgrade()
                && let Some(j) = w.jobs.lock().unwrap().get_mut(&task_id)
            {
                j.progress = v;
            }
        });
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Failure::new(
                "TIMEOUT",
                "Task deadline elapsed before dispatch.",
            ));
        }
        let result = runtime::execute(&self.config, input, remaining, cancel, progress).await;
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
        let text = if patch {
            r.patch.as_deref().unwrap_or_default()
        } else {
            &r.response
        };
        let total = text.chars().count();
        let page: String = text.chars().skip(offset).take(limit).collect();
        let preview: Vec<_> = if offset == 0 && !patch {
            r.report.as_ref().map(|v| v.findings.iter().take(3).map(|f| {
            let evidence: Vec<_> = f.evidence.iter().take(3).map(|e| json!({"file":e.file,"line":e.line,"url":e.url,"excerpt":e.excerpt.chars().take(400).collect::<String>()})).collect();
            json!({"title":f.title,"severity":f.severity,"detail":f.detail.chars().take(1000).collect::<String>(),"evidence":evidence})
        }).collect()).unwrap_or_default()
        } else {
            vec![]
        };
        Ok(
            json!({"job_id":id,"state":j.state,"ready":true,"completion":if j.state==State::Completed{"complete"}else{"incomplete"},"verification_status":"unverified",
            "model":r.model,"actual_model":r.actual_model,"cli_status":r.cli_status,"usage":r.usage,"error":r.error,
            "summary":r.report.as_ref().map(|v|&v.summary),"findings_preview":preview,"findings_count":r.report.as_ref().map_or(0,|v|v.findings.len()),
            "limitations":r.report.as_ref().map(|v|&v.limitations),"manifest":if offset==0{r.manifest.clone()}else{vec![]},
            "section":if patch{"patch"}else{"response"},"text":page,"next_offset":if offset.saturating_add(limit)<total{Some(offset+limit)}else{None},"total_characters":total,
            "patch_available":r.patch.as_ref().is_some_and(|p|!p.is_empty()),"patch_truncated":r.patch_truncated,"expires_at":j.finished.unwrap_or_default()+self.config.retention_seconds*1000}),
        )
    }
    pub async fn cancel(&self, id: &str) -> Outcome<Value> {
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
        let mut v = runtime::capabilities(&self.config, &self.stopped).await?;
        v["dispatch_paused"] = self.paused.load(Ordering::Relaxed).into();
        v["retained_jobs"] = self.jobs.lock().unwrap().len().into();
        v["isolation_installed"] =
            (cfg!(target_os = "linux") && std::path::Path::new("/usr/bin/bwrap").exists()).into();
        Ok(v)
    }
    pub async fn shutdown(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.stopped.cancel();
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
        self.jobs.lock().unwrap().clear();
    }
}
fn not_found() -> Failure {
    Failure::new(
        "JOB_NOT_FOUND",
        "Unknown or expired job; jobs belong to the connection that submitted them.",
    )
}
