# Requirements and product design

Version 0.3. Product: Antigravity Worker MCP.

## 1. Purpose

Expose the official Google Antigravity CLI as a callable agent for Codex, Claude Code and other MCP hosts. A host delegates a task, continues other work, checks progress and retrieves the native answer.

The wrapper manages invocation and lifetime. The official agent decides how to complete the task. No task taxonomy, report template, evidence validator or extra behavioral prompt is imposed.

This community project has no Google affiliation. It uses an installed official CLI and existing login, without private model APIs, account scraping or fabricated subscription-quota estimates.

## 2. Goals and scope

- Make a normal task submission require only its instructions.
- Pass task text unchanged and preserve the native answer.
- Default to host execution, automatic CLI permissions, no task deadline and no isolation.
- Let accepted work survive client disconnection and remain accessible from another client.
- Preserve full local audit streams by default, with a complete opt-out.
- Ship a Rust executable and an optional dependency-free npx launcher for six native platforms.

The wrapper does not evaluate model truth, create commits or deployments, manage Google accounts, or automatically resume interrupted tasks. It is not a host command sandbox. Optional Linux isolation and patch delivery remain explicit operator choices.

## 3. Use cases

Hosts can delegate coding, research, design discussions, repository inspection, extraction or other tasks accepted by the native agent. These examples do not limit the task interface.

Simple questions can return a sentence. A coding task can return Markdown and change files. A caller can request JSON in its own task wording. None requires a wrapper-specific report.

The host chooses when to delegate. Registration does not guarantee tool use or automatic result polling.

## 4. Defaults and execution

| Setting | Default |
| --- | --- |
| Execution | Original host working directory. |
| Isolation | False. |
| Native permissions | Automatic, using current-user OS permissions. |
| Task deadline | None. |
| Result expiry | None. |
| Audit logging | Full local capture, no automatic deletion. |
| Working-directory allowlist | Disabled when absent or empty. |
| Task kind | Optional arbitrary metadata label. |
| Model | Configured fast profile unless a native model slug or deep profile is supplied. |
| Inference retry | None. |
| Shared concurrency | One native task per private state directory. |

root defaults to the submitting MCP connection's current directory. Resolve it before detaching so the supervisor's storage directory cannot become the task workspace. An explicitly configured nonempty allowedRoots list validates the initial directory only.

Headless mode itself does not approve tools. The wrapper supplies --dangerously-skip-permissions; no agy-yolo alias is needed. No elevation is requested, and OS access controls still apply.

## 5. Functional requirements

| ID | Requirement | Acceptance |
| --- | --- | --- |
| F-01 | Stdio initialization and discovery. | Eight MCP tools with valid input schemas. |
| F-02 | Minimal submission. | Only instructions is required; labels and cwd overrides are optional. |
| F-03 | Transparent invocation. | Captured CLI user-message content equals the submitted instructions. |
| F-04 | Native answer delivery. | Preserve plain text, Markdown, code, JSON, empty responses and Unicode. |
| F-05 | Stable control envelope. | Wrapper-generated job/status/runtime metadata never depends on model formatting. |
| F-06 | Background lifetime. | Closing the submitting client does not stop accepted work. |
| F-07 | Cross-client access. | Fresh clients sharing state can inspect, collect and cancel one job. |
| F-08 | Safe retry keys. | An identical retry returns one ID; conflicts fail without new inference. |
| F-09 | Cancellation. | Request supervised cleanup and accurately report whether it completed. |
| F-10 | Optional deadlines. | Default zero; positive deadlines include queue wait without a 900-second ceiling. |
| F-11 | Shared scheduling and quota pause. | FIFO dispatch; provider failures pause queued work until explicit resume. |
| F-12 | Native model selection. | Accept an explicit slug or configured profile without silent substitution. |
| F-13 | Full audit. | Retain observed MCP/CLI streams, selected inputs and lifecycle after disconnect. |
| F-14 | Audit opt-out. | Config or --no-audit creates no wrapper audit files or disabled markers. |
| F-15 | Storage failure handling. | Stop affected supervised work after enabled audit/state I/O fails. |
| F-16 | Optional Linux isolation. | Workspace edits generate patches; analysis inputs remain read-only. |
| F-17 | Honest platform behavior. | Non-Linux isolation fails explicitly; it never becomes host execution. |
| F-18 | Retention and disposal. | Preserve results by default; explicit disposal keeps logs and retry records. |
| F-19 | Crash reconciliation. | Report interrupted work without replay or signalling a saved, unverified PID. |
| F-20 | Public release. | English documentation, license, six native archives, npm tarball and checksums. |

## 6. MCP interface

The [protocol reference](protocol.md) specifies all fields.

| Tool | Contract |
| --- | --- |
| ag_submit | Accept task instructions and optional invocation settings; return a job ID. |
| ag_status | Return execution state and progress counters. |
| ag_result | Page the original answer or optional workspace patch with runtime metadata. |
| ag_cancel | Request cancellation of an accepted job. |
| ag_capabilities | Query native CLI/model capabilities and wrapper defaults without inference. |
| ag_list | Discover retained and expired job metadata without task text. |
| ag_resume | Clear shared provider pause without replaying failed work. |
| ag_forget | Free terminal result capacity while preserving metadata and audit. |

Submission options include root, model, model_profile, kind, files, isolation, execution_mode, timeout_seconds and idempotency_key. They are invocation controls; they do not add task instructions.

The result envelope is produced by Rust. Its text field carries the native answer. No summary/findings/limitations schema, severity taxonomy or source-location requirement is enforced. Unknown answer structures and recovered tool diagnostics do not create format failures.

## 7. Lifecycle and completion

States are queued, running, cancelling, completed, failed and cancelled.

Completed requires successful native terminal status, exit code zero and successful wrapper operations. It indicates CLI completion, not verified task quality. The host evaluates the answer and any changed files. An intermediate denied tool can be followed by successful recovery; the wrapper does not override the final status from diagnostic keywords.

Failure categories cover configuration, unavailable executables/models, input transport, native CLI errors, provider quota/capacity, process startup, deadlines, cancellation and storage. Model-body formatting and semantic evidence checks are excluded.

A task reserves its ID and retry record before dispatch. Terminal results win over late cancellation. Queued cancellation launches no inference. Recovery distinguishes undispatched and interrupted work, and never repeats a task automatically.

## 8. Runtime architecture

An ephemeral Rust RMCP stdio process handles control calls. A detached supervisor owns each accepted job. Shared private filesystem state coordinates admission, FIFO execution, quota pause, status, results and cancellation.

There is no installed daemon, TCP listener or cross-machine broker. Anonymous pipes transfer direct-launch startup input. The Windows desktop route uses authenticated local named-pipe handoff. Task inputs are not saved as replayable requests; enabled audit captures their observed bytes.

Status and results use synchronized temporary writes and atomic replacement. Native locks identify active ownership. Lock files must not be removed during execution. State must be on a local filesystem with working locking and renames; SMB/NFS sharing is unsupported.

See [architecture](architecture.md) for lock ordering, supervision and failure recovery.

## 9. Windows background execution

Attempt direct detached Job Object breakaway first. If the host denies it and windowsDesktopFallback is enabled, use one temporary Task Scheduler registration with the current user's existing interactive token.

No password, elevation, service installation or startup-input file is created. A private local pipe rejects remote callers and verifies each peer's user and executable. Startup waits are bounded independently of task duration. Scheduled execution uses PT0S, with no task-engine deadline or restart policy.

Remove registration after startup, with expiry cleanup for abandoned registration. Deleting registration does not stop the running supervisor. This route requires a usable signed-in desktop and receives profile environment settings rather than every transient parent override. Unavailable launch returns BACKGROUND_UNAVAILABLE.

## 10. Optional isolation and patches

Default host mode uses the original working directory and native tools. Host commands may access other current-user resources.

Linux workspace mode copies selected UTF-8 files to /work, allows writes and returns the actual baseline diff. Analysis mounts the selected inputs read-only and provides writable scratch storage. A temporary writable helper cache supports native CLI helper downloads without making input files writable.

Bubblewrap isolates PID, IPC, UTS and mount namespaces. Configured toolchains and native authentication are mounted read-only. The original home and project remain outside the namespace. Network access is available; this is not an exfiltration firewall.

Only selected inputs are copied. No complete repository, .git, authentication contents or dependency installation is inferred. File-selection transport checks reject traversal, unsafe links, excluded secret/configuration paths, non-text input and oversized snapshots. They do not police later host-agent reads or commands.

Patches remain paged and carry truncation metadata. The host verifies applicability and resulting behavior before applying an isolated patch.

## 11. Data and audit

| Data | Storage and lifetime |
| --- | --- |
| Task input | In-memory invocation handoff; enabled private audit retains observed bytes. |
| Operational status | Private shared state, retained for recovery and retry identity. |
| Native answer and patch | Private result files; no default expiry. |
| MCP/CLI traffic | Full private audit, byte-preserving rotation and no automatic deletion. |
| Google authentication | Official CLI storage; mounted read-only for isolation without inspecting values. |
| Model catalog and usage | Native CLI queries/results; no inferred remaining allowance. |

Audit is enabled by default. --no-audit and auditLogging false disable wrapper capture completely. Operational status and answers still persist for reconnecting clients. Client, official CLI and provider retention follow their own settings.

An enabled audit write failure stops the affected supervised work. Raw capture includes what the CLI emits, not undisclosed provider data. Logs are operational evidence and are not immutable against the same user.

ag_forget frees a terminal answer/patch slot without removing audit, status or deduplication. Capacity exhaustion rejects new work rather than silently evicting existing data.

## 12. Configuration and packaging

| Field | Meaning and default |
| --- | --- |
| agyPath | Absolute installed official CLI executable. |
| models | Configured fast slug and optional deep slug. |
| allowedRoots | Optional exact starting-directory allowlist; empty disables it. |
| allowHostExecution | True; false rejects host mode. |
| windowsDesktopFallback | True; optional current-user desktop launch. |
| stateDirectory | Private local shared state; defaults under the current user's profile. |
| auditLogging | True. |
| auditDirectory | Private audit location under state by default. |
| auditRotateBytes | 16 MiB segment target; no deletion. |
| runtimePaths | Optional read-only toolchain mounts for Linux isolation. |
| timeoutSeconds | Zero, unlimited. |
| maxQueue | Eight unfinished jobs by default. |
| maxJobs | 64 retained result slots by default. |
| retentionSeconds | Zero, no expiry; optional positive retention starts after completion. |

Paths and control parameters require valid serialization and operational bounds. Resource limits are transport/storage controls, not model-output templates or command policies.

Publish Linux, macOS and Windows executables for x86-64 and ARM64. CI runs native format, lint, account-free tests, compilation and archive checks. Tags publish six native archives, an npm tarball and SHA256SUMS only after all targets pass.

The npx launcher requires Node.js 20.11+ and npm, verifies immutable release checksums, installs atomically and verifies cached bytes on reuse. It has no third-party npm dependencies. Unix uses tar; Windows uses PowerShell. Direct native execution needs no Node runtime. Registry publication is separate from the working GitHub-tarball entry point.

## 13. Acceptance and verification

Engineering checks must verify verbatim input, unconstrained native answers, original-content pagination, native model override, optional labels/cwd, explicit directory opt-in, lifetime, recovery, cancellation, retention and logging.

Real provider acceptance must include a native task with no wrapper report instructions, successful non-report output, captured input/output equality and an independently checked effect or calculation. Existing results must remain readable without re-execution.

Fresh Codex and Claude Code sessions on both installed machines must load final CC Switch registrations and retrieve results. Preserve unrelated MCP entries, providers, client settings and authentication. Six-platform packaging establishes native delivery; real provider coverage must be stated separately.

A real task exceeding 900 seconds has already established unlimited execution in the retained version 0.2 verification. Changes to runtime lifetime require renewed evidence when relevant. Format simplification alone does not require spending another twenty minutes on the same unchanged lifecycle.

Complex coding quality, research accuracy, submission latency and quota efficiency need reproducible evaluation. CI success is not product-quality certification. The [verification record](verification.md) identifies measured behavior and gaps.

## 14. Migration and maintenance

Version 0.3 removes compulsory task kinds and wrapper report generation/validation. Existing kind values become optional labels. The old report-preview fields are removed from ag_result; clients read native text pages.

Stored earlier answers remain readable. Historical terminal states and errors are retained, including old report-validation failures. Read their preserved text instead of assuming a new submission is required. No upgrade replays side effects.

A nonempty existing allowedRoots remains an explicit opt-in. Remove it or set an empty list for unrestricted starting directories. Existing timeoutSeconds and retentionSeconds must be zero for unlimited defaults.

Public source, comments, examples, guides and release notes use English. Do not publish credentials, private configurations, actual task transcripts or audit logs. Keep source licenses, dependency notices and immutable version tags. Tests require no Google account.

## 15. References

- [Official Antigravity headless CLI](https://www.antigravity.google/docs/cli/headless/).
- [Model Context Protocol tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
- [Official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk).
- [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp).
- [Claude Code MCP](https://code.claude.com/docs/en/mcp).
- [j-agy-mcp](https://github.com/PichurChill/j-ai-kit/tree/main/packages/j-agy-mcp).
- [antigravity-mcp-server](https://github.com/x51xxx/antigravity-mcp-server).
- [ask-llm Antigravity adapter](https://github.com/Lykhoyda/ask-llm/tree/main/packages/antigravity-mcp).

Community implementations are comparison references, not Google-operated MCP services or runtime dependencies.
