use antigravity_worker_mcp::{
    jobs::Worker,
    model::{Config, Submission},
    runtime, snapshot,
};
use serde_json::{Value, json};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
#[cfg(target_os = "linux")]
use tokio_util::sync::CancellationToken;

struct Context {
    dir: tempfile::TempDir,
    worker: Arc<Worker>,
}
fn supervisor_diagnostics(state: &std::path::Path, id: &str) -> String {
    let mut text = String::new();
    if let Ok(connections) = fs::read_dir(state.join("audit")) {
        for connection in connections.flatten() {
            if let Ok(bytes) = fs::read_to_string(
                connection
                    .path()
                    .join(format!("job-{id}.supervisor.stderr.log")),
            ) {
                text.push_str(&bytes.chars().take(4000).collect::<String>());
            }
        }
    }
    text
}
impl Context {
    fn new(max_queue: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("source");
        let state = dir.path().join("state");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("example.txt"), "original\n").unwrap();
        fs::create_dir(&state).unwrap();
        antigravity_worker_mcp::platform::private_mode(&state, 0o700).unwrap();
        let config = dir.path().join("config.json");
        fs::write(&config,json!({"agyPath":env!("CARGO_BIN_EXE_agy-fixture"),"allowedRoots":[root],"models":{"fast":"fixture-fast","deep":"fixture-deep"},"stateDirectory":state,"maxQueue":max_queue,"windowsDesktopFallback":false}).to_string()).unwrap();
        Self {
            dir,
            worker: Worker::new(Config::load(&config).unwrap()).unwrap(),
        }
    }
    fn submit(
        &self,
        instruction: &str,
        extra: Value,
    ) -> Result<String, antigravity_worker_mcp::model::Failure> {
        let mut v = json!({"kind":"review","root":self.dir.path().join("source"),"files":["example.txt"],"instructions":instruction,"timeout_seconds":10});
        for (k, val) in extra.as_object().unwrap() {
            v[k] = val.clone();
        }
        let s: Submission = serde_json::from_value(v).unwrap();
        Ok(self.worker.submit(s)?["job_id"].as_str().unwrap().into())
    }
    async fn wait(&self, id: &str) -> Value {
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            let s = self.worker.status(id).unwrap();
            if ["completed", "failed", "cancelled"].contains(&s["state"].as_str().unwrap()) {
                return self.worker.result(id, 0, 16000, false).unwrap();
            }
            assert!(Instant::now() < end, "task did not terminate");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
#[tokio::test]
async fn default_host_has_full_permissions_and_automatic_edits() {
    let c = Context::new(8);
    let id = c.submit("CASE:full-permissions", json!({})).unwrap();
    let r = c.wait(&id).await;
    assert_eq!(r["state"], "completed");
    assert_eq!(c.worker.status(&id).unwrap()["execution_mode"], "host");
    assert_eq!(
        fs::read_to_string(c.dir.path().join("source/direct.txt")).unwrap(),
        "host-execution-enabled\n"
    );
    c.worker.shutdown().await;
}
#[tokio::test]
#[cfg(target_os = "linux")]
async fn optional_isolation_returns_applicable_patch_without_changing_source() {
    let c = Context::new(8);
    runtime::check().await.unwrap();
    let id = c.submit("CASE:modify", json!({"isolation":true})).unwrap();
    let r = c.wait(&id).await;
    assert_eq!(r["state"], "completed", "{r}");
    assert_eq!(
        fs::read_to_string(c.dir.path().join("source/example.txt")).unwrap(),
        "original\n"
    );
    let patch = c.worker.result(&id, 0, 16000, true).unwrap();
    let path = c.dir.path().join("change.patch");
    fs::write(&path, patch["text"].as_str().unwrap()).unwrap();
    let out = runtime::process(
        Path::new("/usr/bin/git"),
        &["apply".into(), path.into_os_string()],
        Some(&c.dir.path().join("source")),
        None,
        Duration::from_secs(3),
        &CancellationToken::new(),
        None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(out.code, Some(0));
    assert_eq!(
        fs::read_to_string(c.dir.path().join("source/example.txt")).unwrap(),
        "updated\n"
    );
    assert!(c.dir.path().join("source/new.txt").exists());
    assert!(
        !fs::read_dir(c.dir.path().join("state")).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("job-"))
    );
    c.worker.shutdown().await;
}
#[tokio::test]
#[cfg(target_os = "linux")]
async fn analysis_denial_is_failure_even_with_zero_exit() {
    let c = Context::new(8);
    let id = c
        .submit("CASE:readonly", json!({"execution_mode":"analysis"}))
        .unwrap();
    let r = c.wait(&id).await;
    assert_eq!(r["error"]["code"], "PERMISSION_DENIED");
    assert_eq!(
        fs::read_to_string(c.dir.path().join("source/example.txt")).unwrap(),
        "original\n"
    );
    c.worker.shutdown().await;
}
#[tokio::test]
async fn timeout_malformed_output_and_running_cancel() {
    let c = Context::new(8);
    let id = c.submit("CASE:wait", json!({"timeout_seconds":1})).unwrap();
    assert_eq!(c.wait(&id).await["error"]["code"], "TIMEOUT");
    let id = c.submit("CASE:malformed", json!({})).unwrap();
    assert_eq!(c.wait(&id).await["error"]["code"], "STREAM_INVALID");
    let id = c.submit("CASE:output-limit", json!({})).unwrap();
    assert_eq!(c.wait(&id).await["error"]["code"], "OUTPUT_LIMIT");
    let id = c.submit("CASE:wait", json!({})).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let v = c.worker.cancel(&id).await.unwrap();
    assert_eq!(v["state"], "cancelled");
    assert_eq!(v["process_stopped"], true);
    c.worker.shutdown().await;
}
#[tokio::test]
async fn cancellation_stops_a_supervised_descendant() {
    let c = Context::new(8);
    let id = c.submit("CASE:child", json!({})).unwrap();
    let sentinel = c.dir.path().join("source/child-alive.txt");
    let end = Instant::now() + Duration::from_secs(5);
    while !sentinel.exists() {
        assert!(Instant::now() < end, "descendant did not start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(c.worker.cancel(&id).await.unwrap()["process_stopped"], true);
    let stopped = fs::read(&sentinel).unwrap();
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(fs::read(&sentinel).unwrap(), stopped);
    c.worker.shutdown().await;
}
#[tokio::test]
async fn queue_cancel_bounds_quota_and_model_validation() {
    let c = Context::new(2);
    let long = c.submit("CASE:wait", json!({})).unwrap();
    let queued = c.submit("fixture", json!({})).unwrap();
    assert_eq!(
        c.submit("fixture", json!({})).unwrap_err().code,
        "QUEUE_FULL"
    );
    assert_eq!(
        c.worker.cancel(&queued).await.unwrap()["state"],
        "cancelled"
    );
    c.worker.cancel(&long).await.unwrap();
    let quota = c.submit("CASE:quota", json!({})).unwrap();
    let after_quota = c.submit("fixture", json!({})).unwrap();
    assert_eq!(c.wait(&quota).await["error"]["code"], "QUOTA_EXHAUSTED");
    assert_eq!(c.wait(&after_quota).await["error"]["code"], "QUOTA_PAUSED");
    assert_eq!(
        c.submit("fixture", json!({})).unwrap_err().code,
        "QUOTA_PAUSED"
    );
    c.worker.shutdown().await;
}
#[tokio::test]
async fn separate_workers_share_execution_lock() {
    let c = Context::new(8);
    let other = Worker::new(c.worker.config.clone()).unwrap();
    let id = c.submit("CASE:wait", json!({})).unwrap();
    for _ in 0..100 {
        if c.worker.status(&id).unwrap()["progress"]["events"]
            .as_u64()
            .unwrap_or(0)
            > 0
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(runtime::try_lock(&other.config).unwrap().is_none());
    c.worker.cancel(&id).await.unwrap();
    assert!(runtime::try_lock(&other.config).unwrap().is_some());
    other.shutdown().await;
    c.worker.shutdown().await;
}
#[tokio::test]
async fn source_symlinks_hardlinks_and_nontext_are_rejected() {
    let c = Context::new(8);
    let root = c.dir.path().join("source");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("example.txt"), root.join("alias.txt")).unwrap();
        let id = c.submit("fixture", json!({"files":["alias.txt"]})).unwrap();
        assert_eq!(c.wait(&id).await["error"]["code"], "INPUT_SCOPE_INVALID");
    }
    #[cfg(windows)]
    {
        let junction = root.join("junction");
        let target = c.dir.path().join("outside");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("example.txt"), "outside\n").unwrap();
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap();
        assert!(output.status.success(), "junction creation failed");
        let id = c
            .submit("fixture", json!({"files":["junction/example.txt"]}))
            .unwrap();
        assert_eq!(c.wait(&id).await["error"]["code"], "INPUT_SCOPE_INVALID");
        fs::remove_dir(junction).unwrap();
    }
    fs::hard_link(root.join("example.txt"), root.join("linked.txt")).unwrap();
    let id = c.submit("fixture", json!({})).unwrap();
    assert_eq!(c.wait(&id).await["error"]["code"], "INPUT_SCOPE_INVALID");
    let input: Submission = serde_json::from_value(
        json!({"kind":"review","instructions":"fixture","root":root,"files":["../outside"]}),
    )
    .unwrap();
    assert_eq!(
        snapshot::capture(&c.worker.config, &input, &c.dir.path().join("bad-snapshot"))
            .unwrap_err()
            .code,
        "INPUT_SCOPE_INVALID"
    );
    fs::write(root.join("binary.txt"), [0u8, 1u8]).unwrap();
    let id = c
        .submit("fixture", json!({"files":["binary.txt"]}))
        .unwrap();
    assert_eq!(c.wait(&id).await["error"]["code"], "INPUT_FORMAT_INVALID");
    c.worker.shutdown().await;
}
#[tokio::test]
async fn queue_deadline_expires_before_dispatch() {
    let c = Context::new(8);
    let long = c.submit("CASE:wait", json!({})).unwrap();
    let queued = c.submit("fixture", json!({"timeout_seconds":1})).unwrap();
    let r = c.wait(&queued).await;
    assert_eq!(r["error"]["code"], "TIMEOUT");
    assert_eq!(c.worker.status(&queued).unwrap()["started_at"], Value::Null);
    c.worker.cancel(&long).await.unwrap();
    c.worker.shutdown().await;
}
#[tokio::test]
async fn host_toggle_model_and_result_store_limits() {
    let c = Context::new(8);
    let mut config = c.worker.config.clone();
    config.allow_host_execution = false;
    config.models.deep = None;
    config.max_queue = 2;
    config.max_jobs = 2;
    let worker = Worker::new(config).unwrap();
    let host: Submission = serde_json::from_value(
        json!({"kind":"research","instructions":"fixture","root":c.dir.path().join("source")}),
    )
    .unwrap();
    assert_eq!(worker.submit(host).unwrap_err().code, "HOST_MODE_DISABLED");
    let deep: Submission = serde_json::from_value(
        json!({"kind":"research","instructions":"fixture","isolation":true,"model_profile":"deep"}),
    )
    .unwrap();
    assert_eq!(worker.submit(deep).unwrap_err().code, "MODEL_UNAVAILABLE");
    worker.shutdown().await;
    let mut config = worker.config.clone();
    config.allow_host_execution = true;
    let worker = Worker::new(config).unwrap();
    for _ in 0..2 {
        let input: Submission = serde_json::from_value(
            json!({"kind":"research","instructions":"fixture","root":c.dir.path().join("source")}),
        )
        .unwrap();
        let id = worker.submit(input).unwrap()["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        for _ in 0..100 {
            if worker.result(&id, 0, 8000, false).unwrap()["ready"] == true {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    let input: Submission = serde_json::from_value(
        json!({"kind":"research","instructions":"fixture","root":c.dir.path().join("source")}),
    )
    .unwrap();
    assert_eq!(worker.submit(input).unwrap_err().code, "RESULT_STORE_FULL");
    worker.shutdown().await;
    c.worker.shutdown().await;
}
#[cfg(not(target_os = "linux"))]
#[tokio::test]
async fn isolation_is_rejected_without_host_fallback() {
    let c = Context::new(8);
    let id = c.submit("CASE:modify", json!({"isolation":true})).unwrap();
    assert_eq!(c.wait(&id).await["error"]["code"], "ISOLATION_UNSUPPORTED");
    assert_eq!(
        fs::read_to_string(c.dir.path().join("source/example.txt")).unwrap(),
        "original\n"
    );
    c.worker.shutdown().await;
}
#[tokio::test]
async fn unicode_pagination_delivers_complete_report() {
    let c = Context::new(8);
    let id = c.submit("fixture", json!({})).unwrap();
    let r = c.wait(&id).await;
    let full = r["text"].as_str().unwrap();
    let mut text = String::new();
    let mut offset = 0;
    loop {
        let v = c.worker.result(&id, offset, 3, false).unwrap();
        text.push_str(v["text"].as_str().unwrap());
        if let Some(next) = v["next_offset"].as_u64() {
            offset = next as usize;
        } else {
            break;
        }
    }
    assert_eq!(text, full);
    assert!(text.contains("\u{e9}\u{1f680}"));
    c.worker.shutdown().await;
}
#[tokio::test]
async fn real_stdio_discovery_paging_and_shutdown() {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    let c = Context::new(8);
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_antigravity-worker-mcp"));
    command.args([
        "--config",
        c.dir.path().join("config.json").to_str().unwrap(),
    ]);
    let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 8);
    assert!(tools.iter().any(|t| t.name == "ag_submit"));
    let caps = client
        .call_tool(CallToolRequestParams::new("ag_capabilities"))
        .await
        .unwrap();
    let audit_path = std::path::PathBuf::from(
        caps.structured_content.as_ref().unwrap()["audit_connection_directory"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(
        caps.structured_content.as_ref().unwrap()["default_isolation"],
        false
    );
    let args = json!({"kind":"review","root":c.dir.path().join("source"),"instructions":"fixture"});
    let req =
        CallToolRequestParams::new("ag_submit").with_arguments(args.as_object().unwrap().clone());
    let submitted = client.call_tool(req).await.unwrap();
    let id = submitted.structured_content.unwrap()["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut finished = false;
    for _ in 0..100 {
        let s = client
            .call_tool(
                CallToolRequestParams::new("ag_status")
                    .with_arguments(json!({"job_id":id}).as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        if s.structured_content.unwrap()["state"] == "completed" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        finished,
        "{}",
        supervisor_diagnostics(&c.worker.config.state_directory, &id)
    );
    let result = client
        .call_tool(
            CallToolRequestParams::new("ag_result")
                .with_arguments(json!({"job_id":id,"limit":5}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let v = result.structured_content.unwrap();
    assert_eq!(v["next_offset"], 5);
    assert_eq!(v["text"].as_str().unwrap().chars().count(), 5);
    assert_eq!(v["verification_status"], "unverified");
    client.cancel().await.unwrap();
    let incoming = String::from_utf8(audit_bytes(&audit_path, "mcp.stdin")).unwrap();
    let outgoing = String::from_utf8(audit_bytes(&audit_path, "mcp.stdout")).unwrap();
    assert!(incoming.contains("ag_submit") && incoming.contains("fixture"));
    assert!(outgoing.contains("unverified") && outgoing.contains("default_isolation"));
    c.worker.shutdown().await;
}

fn audit_bytes(directory: &std::path::Path, prefix: &str) -> Vec<u8> {
    let mut files: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(&format!("{prefix}."))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .flat_map(|p| fs::read(p).unwrap())
        .collect()
}

#[tokio::test]
async fn full_audit_survives_shutdown_and_keeps_complete_task_materials() {
    let c = Context::new(8);
    let id = c.submit("CASE:audit", json!({})).unwrap();
    let result = c.wait(&id).await;
    let directory = c.worker.audit.directory.clone().unwrap();
    c.worker.shutdown().await;
    assert!(directory.exists());
    let prefix = format!("job-{id}");
    assert_eq!(
        audit_bytes(&directory, &format!("{prefix}.input-0000")),
        b"original\n"
    );
    let stdin = String::from_utf8(audit_bytes(&directory, &format!("{prefix}.cli.stdin"))).unwrap();
    let stdout =
        String::from_utf8(audit_bytes(&directory, &format!("{prefix}.cli.stdout"))).unwrap();
    let stderr =
        String::from_utf8(audit_bytes(&directory, &format!("{prefix}.cli.stderr"))).unwrap();
    assert!(stdin.contains("CASE:audit"));
    assert!(stdout.contains("structured_output") && stdout.contains("SUCCESS"));
    assert!(stderr.contains("Synthetic audit diagnostic."));
    let events = String::from_utf8(audit_bytes(&directory, "events")).unwrap();
    assert!(
        events.contains("job.queued")
            && events.contains("job.finished")
            && events.contains("connection.shutdown")
    );
    for line in events.lines() {
        assert!(serde_json::from_str::<Value>(line).is_ok());
    }
    assert_eq!(result["exit_code"], 0);
    assert!(result["duration_ms"].is_number());
    for entry in fs::read_dir(directory).unwrap() {
        assert!(antigravity_worker_mcp::platform::is_private(
            &entry.unwrap().metadata().unwrap()
        ));
    }
}

#[tokio::test]
async fn disabled_audit_creates_no_files_and_cli_override_is_complete() {
    let c = Context::new(8);
    let mut config = c.worker.config.clone();
    config.audit_logging = false;
    config.audit_directory = c.dir.path().join("disabled-audit");
    let worker = Worker::new(config.clone()).unwrap();
    let input: Submission = serde_json::from_value(
        json!({"kind":"review","root":c.dir.path().join("source"),"instructions":"fixture"}),
    )
    .unwrap();
    let id = worker.submit(input).unwrap()["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 0..100 {
        if worker.status(&id).unwrap()["state"] == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let result = worker.result(&id, 0, 8000, false).unwrap();
    assert_eq!(result["state"], "completed");
    assert_eq!(result["audit_logging"], false);
    assert!(result["audit_directory"].is_null());
    assert!(result["audit_stream_prefix"].is_null());
    assert!(!config.audit_directory.exists());
    worker.shutdown().await;
    let override_path = c.dir.path().join("override-audit");
    let config_path = c.dir.path().join("override.json");
    fs::write(&config_path,json!({"agyPath":env!("CARGO_BIN_EXE_agy-fixture"),"allowedRoots":[c.dir.path().join("source")],"models":{"fast":"fixture-fast"},"stateDirectory":c.worker.config.state_directory,"auditDirectory":override_path,"auditLogging":true}).to_string()).unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_antigravity-worker-mcp"));
    command.args(["--config", config_path.to_str().unwrap(), "--no-audit"]);
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
    let caps = client
        .call_tool(CallToolRequestParams::new("ag_capabilities"))
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(caps["audit_logging"], false);
    assert!(caps["audit_connection_directory"].is_null());
    client.cancel().await.unwrap();
    assert!(!override_path.exists());
    c.worker.shutdown().await;
}

#[tokio::test]
async fn audit_rotation_preserves_every_byte_and_log_failure_stops_execution() {
    let c = Context::new(8);
    let mut config = c.worker.config.clone();
    config.audit_rotate_bytes = 4096;
    let worker = Worker::new(config).unwrap();
    let payload = "\u{e9}\u{1f680}".repeat(2500).into_bytes();
    worker.audit.bytes("payload", &payload).unwrap();
    let directory = worker.audit.directory.clone().unwrap();
    assert_eq!(audit_bytes(&directory, "payload"), payload);
    let input: Submission = serde_json::from_value(json!({"kind":"review","root":c.dir.path().join("source"),"instructions":"CASE:child","timeout_seconds":15})).unwrap();
    let _ = worker.submit(input).unwrap();
    let sentinel = c.dir.path().join("source/child-alive.txt");
    for _ in 0..100 {
        if sentinel.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(sentinel.exists());
    worker
        .audit
        .bytes("failure-trigger", &vec![b'x'; 4000])
        .unwrap();
    fs::remove_dir_all(&directory).unwrap();
    assert_eq!(
        worker
            .audit
            .bytes("failure-trigger", &[b'x'; 100])
            .unwrap_err()
            .code,
        "AUDIT_FAILED"
    );
    for _ in 0..100 {
        if worker.capabilities().await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let before = fs::read(&sentinel).unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(before, fs::read(&sentinel).unwrap());
    assert!(worker.audit.has_failed());
    worker.shutdown().await;
    c.worker.shutdown().await;
}

#[tokio::test]
async fn detached_jobs_survive_reconnection_and_accept_unlimited_duration() {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    let c = Context::new(8);
    let connect = || {
        let mut command =
            tokio::process::Command::new(env!("CARGO_BIN_EXE_antigravity-worker-mcp"));
        command.args([
            "--config",
            c.dir.path().join("config.json").to_str().unwrap(),
        ]);
        async move { ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap() }
    };
    let first = connect().await;
    let arguments = json!({"kind":"review","instructions":"CASE:delay CASE:unlimited","root":c.dir.path().join("source"),"idempotency_key":"reconnection-test"});
    let submitted = first
        .call_tool(
            CallToolRequestParams::new("ag_submit")
                .with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    let id = submitted["job_id"].as_str().unwrap().to_owned();
    assert_eq!(submitted["deadline"], Value::Null);
    assert_eq!(submitted["background"], true);
    first.cancel().await.unwrap();
    let second = connect().await;
    let duplicate = second
        .call_tool(
            CallToolRequestParams::new("ag_submit")
                .with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(duplicate["job_id"], id);
    assert_eq!(duplicate["deduplicated"], true);
    let mut finished = None;
    for _ in 0..150 {
        let result = second
            .call_tool(
                CallToolRequestParams::new("ag_result")
                    .with_arguments(json!({"job_id":id}).as_object().unwrap().clone()),
            )
            .await
            .unwrap()
            .structured_content
            .unwrap();
        if result["ready"] == true {
            finished = Some(result);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        finished.as_ref().unwrap()["state"],
        "completed",
        "{finished:?}; {}",
        supervisor_diagnostics(&c.worker.config.state_directory, &id)
    );
    assert_eq!(finished.as_ref().unwrap()["process_stopped"], true);
    let mut conflict = arguments;
    conflict["instructions"] = "different task".into();
    let conflict = second
        .call_tool(
            CallToolRequestParams::new("ag_submit")
                .with_arguments(conflict.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(conflict["error"]["code"], "IDEMPOTENCY_CONFLICT");
    let submitted=second.call_tool(CallToolRequestParams::new("ag_submit").with_arguments(json!({"kind":"review","instructions":"CASE:child","root":c.dir.path().join("source"),"timeout_seconds":0}).as_object().unwrap().clone())).await.unwrap().structured_content.unwrap();
    let long = submitted["job_id"].as_str().unwrap().to_owned();
    second.cancel().await.unwrap();
    let sentinel = c.dir.path().join("source/child-alive.txt");
    let ready_until = Instant::now() + Duration::from_secs(10);
    while !sentinel.exists() && Instant::now() < ready_until {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        sentinel.exists(),
        "Detached descendant did not become ready; {}",
        supervisor_diagnostics(&c.worker.config.state_directory, &long)
    );
    let third = connect().await;
    let listed = third
        .call_tool(
            CallToolRequestParams::new("ag_list")
                .with_arguments(json!({"limit":1}).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(listed["jobs"].as_array().unwrap().len(), 1);
    assert!(listed["next_after"].is_string());
    let cancelled = third
        .call_tool(
            CallToolRequestParams::new("ag_cancel")
                .with_arguments(json!({"job_id":long}).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(cancelled["process_stopped"], true);
    let stopped = fs::read(&sentinel).unwrap();
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(fs::read(&sentinel).unwrap(), stopped);
    let audit = std::path::PathBuf::from(
        finished.as_ref().unwrap()["audit_directory"]
            .as_str()
            .unwrap(),
    );
    let bytes = audit_bytes(&audit, &format!("job-{id}.cli.stdout"));
    assert!(String::from_utf8(bytes).unwrap().contains("SUCCESS"));
    third.cancel().await.unwrap();
    c.worker.shutdown().await;
}

#[cfg(windows)]
#[tokio::test]
async fn restricted_windows_launcher_refuses_background_work_without_dispatch() {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::{JobObjects::*, Threading::*};
    let c = Context::new(8);
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_antigravity-worker-mcp"));
    command.args([
        "--config",
        c.dir.path().join("config.json").to_str().unwrap(),
    ]);
    let transport = TokioChildProcess::new(command).unwrap();
    let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    assert!(!raw.is_null());
    let job = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    assert_ne!(
        unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
        },
        0
    );
    let raw = unsafe {
        OpenProcess(
            PROCESS_SET_QUOTA | PROCESS_TERMINATE,
            0,
            transport.id().unwrap(),
        )
    };
    assert!(!raw.is_null());
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    assert_ne!(
        unsafe { AssignProcessToJobObject(job.as_raw_handle(), process.as_raw_handle()) },
        0
    );
    let client = ().serve(transport).await.unwrap();
    let arguments = json!({"kind":"review","instructions":"CASE:full-permissions","root":c.dir.path().join("source"),"idempotency_key":"restricted-launcher"});
    let submit = || {
        CallToolRequestParams::new("ag_submit")
            .with_arguments(arguments.as_object().unwrap().clone())
    };
    let submitted = client
        .call_tool(submit())
        .await
        .unwrap()
        .structured_content
        .unwrap();
    let id = submitted["job_id"].as_str().unwrap();
    let result = client
        .call_tool(
            CallToolRequestParams::new("ag_result")
                .with_arguments(json!({"job_id":id}).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(result["state"], "failed");
    assert_eq!(result["error"]["code"], "BACKGROUND_UNAVAILABLE");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("OS error 5")
    );
    assert_eq!(result["process_stopped"], true);
    assert!(!c.dir.path().join("source/direct.txt").exists());
    let duplicate = client
        .call_tool(submit())
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(duplicate["job_id"], id);
    assert_eq!(duplicate["deduplicated"], true);
    client.cancel().await.unwrap();
    c.worker.shutdown().await;
}

#[tokio::test]
async fn shared_quota_pause_keeps_waiting_jobs_and_disposal_preserves_keys() {
    use antigravity_worker_mcp::broker::Broker;
    let c = Context::new(8);
    let broker = Broker::new(
        c.worker.config.clone(),
        env!("CARGO_BIN_EXE_antigravity-worker-mcp").into(),
        c.worker.audit.clone(),
    )
    .unwrap();
    let input = |text: &str, key: &str, timeout: u64| {
        serde_json::from_value(json!({"kind":"review","instructions":text,"root":c.dir.path().join("source"),"idempotency_key":key,"timeout_seconds":timeout})).unwrap()
    };
    let first = broker
        .submit(input("CASE:quota", "quota", 0))
        .await
        .unwrap();
    let id = first["job_id"].as_str().unwrap();
    for _ in 0..100 {
        if broker.status(id).unwrap()["state"] == "failed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        broker.result(id, 0, 1000, false).unwrap()["error"]["code"],
        "QUOTA_EXHAUSTED",
        "{}",
        supervisor_diagnostics(&c.worker.config.state_directory, id)
    );
    assert_eq!(
        broker
            .submit(input("fixture", "blocked", 0))
            .await
            .unwrap_err()
            .code,
        "QUOTA_PAUSED"
    );
    broker.resume().await.unwrap();
    let long = broker
        .submit(input("CASE:wait", "long", 3601))
        .await
        .unwrap();
    let long = long["job_id"].as_str().unwrap();
    let queued = broker.submit(input("fixture", "waiting", 0)).await.unwrap();
    let queued = queued["job_id"].as_str().unwrap();
    fs::write(c.worker.config.state_directory.join("dispatch.pause"), "{}").unwrap();
    broker.cancel(long).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        broker.status(queued).unwrap()["state"],
        "queued",
        "{}",
        supervisor_diagnostics(&c.worker.config.state_directory, queued)
    );
    assert_eq!(broker.forget(queued).await.unwrap_err().code, "JOB_ACTIVE");
    broker.resume().await.unwrap();
    for _ in 0..100 {
        if broker.status(queued).unwrap()["state"] == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        broker.status(queued).unwrap()["state"],
        "completed",
        "{}; {:?}",
        supervisor_diagnostics(&c.worker.config.state_directory, queued),
        broker.status(queued)
    );
    broker.forget(queued).await.unwrap();
    assert_eq!(
        broker.result(queued, 0, 1000, false).unwrap_err().code,
        "RESULT_EXPIRED"
    );
    let duplicate = broker.submit(input("fixture", "waiting", 0)).await.unwrap();
    assert_eq!(duplicate["job_id"], queued);
    assert_eq!(duplicate["deduplicated"], true);
    assert_eq!(duplicate["result_expired"], true);
    assert_eq!(broker.cancel(queued).await.unwrap()["state"], "completed");
    c.worker.shutdown().await;
}
