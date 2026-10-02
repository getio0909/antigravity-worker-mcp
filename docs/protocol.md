# MCP protocol

Version 0.1 exposes five tools through stdio. Inputs reject unknown fields. Tool failures return `isError: true` with `{ "error": { "code", "message" } }`. Execution failures appear in the job's result after an accepted submission.

## ag_submit

```json
{
  "kind": "code",
  "root": "/absolute/path/to/project",
  "instructions": "Implement the scoped change and report verification results.",
  "isolation": false,
  "model_profile": "fast",
  "timeout_seconds": 300
}
```

`kind` is `repository_analysis`, `review`, `research`, `extraction`, or `code`. `instructions` is required and limited to 16,000 Unicode characters. `root` is an allowed absolute starting directory; it is required for host execution or selected files. `files` defaults to an empty array and is limited to 100 relative file paths.

`isolation` defaults to **false**. `execution_mode` defaults to **host**. Setting `isolation: true` selects `workspace`; explicit `analysis` remains read-only. Explicit `workspace` also enables isolation. The response reports the resolved mode. `model_profile` is `fast` or `deep`, defaulting to `fast`. The requested profile must be configured. `timeout_seconds` is 1–900 and includes queue time.

Host tasks execute in the original starting directory with automatic CLI approval and the current user's environment. Workspace tasks receive only selected input files and return a patch. File selection limits do not restrict the host CLI's subsequent file or command access.

Host mode is available on Linux, macOS and Windows. Workspace and analysis require Linux; other systems fail isolated execution with `ISOLATION_UNSUPPORTED` and preserve the requested mode without host fallback.

Submission returns `job_id`, `state`, `execution_mode`, and `deadline`. All timestamp fields are Unix milliseconds. Submission is not idempotent; do not retry blindly after losing its response.

## ag_status

Input: `{ "job_id": "uuid" }`.

States: `queued`, `running`, `cancelling`, `completed`, `failed`, `cancelled`. Progress contains `events`, `steps`, and `output_bytes`; these are counters, not a completion percentage. `completion` is `pending`, `complete`, or `incomplete`. `verification_status` is always `unverified`.

Jobs belong to the submitting stdio connection. A second connection cannot query their IDs. The shared execution lock coordinates runtime concurrency, not result access.

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

Input: `{ "job_id": "uuid" }`. Returns terminal state after cancelling and waiting for supervised runtime termination. `process_stopped` refers to the supervised CLI and process group. PID isolation also constrains descendants; host-mode detached processes and external side effects are outside that guarantee.

## ag_capabilities

No arguments. Queries the official CLI's version and live model catalog without inference. Reports platform, architecture, isolation support, available execution modes, configured profiles, `default_isolation: false`, `default_execution_mode: "host"`, resource limits, lock scope, and connection-local dispatch pause.

Remaining subscription quota is reported as unavailable. Token counts are not converted into remaining quota. A provider quota or capacity failure pauses new dispatch in that connection until restart.

## CLI compatibility

The adapter uses `--input-format stream-json`, `--output-format stream-json`, `--model`, `--mode`, `--disable-slash-commands`, `--print-timeout`, and `--json-schema`. Host/workspace add `--dangerously-skip-permissions`. Requests are sent as stdin `user` events; output parses `init`, `step_update`, and one terminal `result`. Unknown event types are ignored. Malformed JSON, duplicate terminal results, missing success, permission denial, and field-limit failures are explicit failures.
