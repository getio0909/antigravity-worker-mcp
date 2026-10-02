# MCP protocol

Version 0.2 exposes eight tools through stdio. Inputs reject unknown fields. Tool failures return `isError: true` with `{ "error": { "code", "message" } }`. Execution failures appear in the job's result after an accepted submission.

## ag_submit

```json
{
  "kind": "code",
  "root": "/absolute/path/to/project",
  "instructions": "Implement the scoped change and report verification results.",
  "isolation": false,
  "model_profile": "fast",
  "timeout_seconds": 0,
  "idempotency_key": "scoped-change-001"
}
```

`kind` is `repository_analysis`, `review`, `research`, `extraction`, or `code`. `instructions` is required and limited to 16,000 Unicode characters. `root` is an allowed absolute starting directory; it is required for host execution or selected files. `files` defaults to an empty array and is limited to 100 relative file paths.

`isolation` defaults to **false**. `execution_mode` defaults to **host**. Setting `isolation: true` selects `workspace`; explicit `analysis` remains read-only. Explicit `workspace` also enables isolation. The response reports the resolved mode. `model_profile` is `fast` or `deep`, defaulting to `fast`. The requested profile must be configured. `timeout_seconds: 0` disables the deadline. Omitted or null values use `timeoutSeconds`, which defaults to zero. Positive values enable an optional deadline including queue time, with no 900-second cap. Values that cannot be represented are rejected.

Host tasks execute in the original starting directory with automatic CLI approval and the current user's environment. Workspace tasks receive only selected input files and return a patch. File selection limits do not restrict the host CLI's subsequent file or command access.

Host mode is available on Linux, macOS and Windows. Workspace and analysis require Linux; other systems fail isolated execution with `ISOLATION_UNSUPPORTED` and preserve the requested mode without host fallback.

Submission returns `job_id`, `state`, `execution_mode`, `deadline`, `background`, `supervisor_ready`, `deduplicated` and `result_expired`. A disabled deadline is null. Timestamps are Unix milliseconds. An optional `idempotency_key` (1–128 UTF-8 bytes) safely deduplicates identical retries within the shared state directory. Different parameters with that key return `IDEMPOTENCY_CONFLICT`. Failed, interrupted and expired jobs retain their key; a deliberate new attempt needs a new key. Without a key, do not retry blindly after response loss.

## ag_status

Input: `{ "job_id": "uuid" }`.

States: `queued`, `running`, `cancelling`, `completed`, `failed`, `cancelled`. Progress contains `events`, `steps`, and `output_bytes`; these are counters, not a completion percentage. `completion` is `pending`, `complete`, or `incomplete`. `verification_status` is always `unverified`.

Jobs belong to the shared private state directory and survive client disconnection. Another connection can query, list or cancel the same IDs. Status adds `background`, `supervisor_ready`, `process_stopped`, `result_expired`, `expires_at` and shared `dispatch_paused`. A lost supervisor becomes `RUNNER_INTERRUPTED`; no task is replayed.

## ag_result

```json
{
  "job_id": "uuid",
  "offset": 0,
  "limit": 8000,
  "section": "response"
}
```

`section` is `response` or `patch`. `offset` defaults to 0; `limit` defaults to 8,000 and has a maximum of 16,000. Offsets count Unicode characters. Follow `next_offset` until it becomes null. A not-yet-finished job returns `ready: false`.

Completed jobs include model selection, CLI status, token usage if available, a summary, a bounded `findings_preview`, the total finding count, limitations, input fingerprints, a text page, and expiry. The complete validated report is serialized in `response`; previews are shortened and do not replace it. `patch_truncated: true` means the patch is incomplete.

Terminal responses also include `exit_code`, `duration_ms`, `audit_logging`, `audit_directory` and `audit_stream_prefix`. Exit code and duration can be null if execution never started. Audit references are null when logging is disabled. Audit files remain after result disposal or expiry and are not served through MCP. Results do not expire by default; `retentionSeconds: 0` retains them, and `ag_forget` explicitly frees a terminal result slot. A positive retention interval begins only after completion. Expired results return `RESULT_EXPIRED`, while job metadata remains.

Report shape:

```json
{
  "summary": "Report summary",
  "findings": [
    {
      "title": "Finding title",
      "detail": "Description and verification method",
      "severity": "low",
      "evidence": [
        { "file": "src/example.rs", "line": 12, "url": null, "excerpt": "Relevant evidence" }
      ]
    }
  ],
  "limitations": []
}
```

Severity is `info`, `low`, `medium`, or `high`. Evidence needs a file or an HTTP(S) URL. In isolated modes, file references must match the input manifest and its original line count. Host-mode file references remain unverified because tasks can access files beyond the captured input set. URLs are not fetched by the validator, and excerpts are not semantically verified.

Limits: summary 8,000 characters; 100 findings; finding title 300; detail 4,000; 20 evidence entries per finding; excerpt 2,000; 30 limitations of 2,000 characters each. A structurally valid report can still contain incorrect claims.

## ag_cancel

Input: `{ "job_id": "uuid" }`. Requests cancellation and waits up to ten seconds for supervised cleanup. If still pending, poll `ag_status`; `process_stopped` remains false. Completed runtime results win over late cancellation. `process_stopped` refers to the supervised CLI and process group. PID isolation also constrains descendants; host-mode detached processes and external side effects are outside that guarantee.

## ag_capabilities

No arguments. Queries the official CLI's version and live model catalog without inference. Reports platform, architecture, isolation support, available execution modes, configured profiles, `default_isolation: false`, `default_execution_mode: "host"`, resource limits, lock scope, and shared dispatch pause.

Audit fields are `audit_logging`, `audit_connection_directory`, `audit_rotation_bytes` and `audit_auto_delete: false`. See the [audit contract](audit.md) for full retention, the off switch and failure behavior. `AUDIT_UNSAFE` rejects unsafe startup storage; `AUDIT_FAILED` stops dispatch and cancels affected supervised jobs after enabled log I/O fails.

Remaining subscription quota is reported as unavailable. Token counts are not converted into remaining quota. A provider quota or capacity failure pauses shared dispatch. Queued jobs wait until `ag_resume`, cancellation or their explicit deadline. Failed jobs are never replayed.

## ag_list, ag_resume and ag_forget

`ag_list` accepts optional `after` (the previous `next_after`) and `limit` (1–50, default 20). It returns newest-first status records in `jobs`, including expired-result metadata, and `next_after`. It returns no task text.

`ag_resume` accepts `{}` and explicitly clears the shared quota pause. Check provider availability first; `replayed_jobs` is always zero.

`ag_forget` accepts `{ "job_id": "uuid" }`. It rejects active jobs with `JOB_ACTIVE` and discards only a terminal result or patch. Job metadata, idempotency and enabled audit logs remain. It is safe to repeat. A full retained-result store rejects new submissions with `RESULT_STORE_FULL`; it never automatically evicts reports.

## CLI compatibility

The adapter uses `--input-format stream-json`, `--output-format stream-json`, `--model`, `--mode`, `--print-timeout`, and `--json-schema`. All modes add `--dangerously-skip-permissions`. Analysis relies on read-only input mounts while allowing useful native tools and writable scratch space. Unlimited jobs use native `--print-timeout 0`, which waits for turn completion. Only isolated requests add `--disable-slash-commands`; host mode retains the CLI's normal custom-command behavior. Requests contain the task objective, input context and final report format, without extra behavioral instructions. Output parses `init`, `step_update`, and one terminal `result`. Unknown event types are ignored. Malformed JSON, duplicate terminal results, missing success, permission denial, and field-limit failures are explicit failures.
