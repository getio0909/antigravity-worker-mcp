# Architecture and operation

The executable has two roles: an ephemeral RMCP stdio adapter and one detached supervisor per accepted job. The supervisor owns execution; the client connection owns transport and control requests. Closing Codex, Claude Code or their MCP connection leaves accepted jobs running. Another connection using the same private local `stateDirectory` can discover, query or cancel them. No service installation, TCP listener or cross-machine broker is required.

## Design principles

- Task duration and MCP request duration are separate. Submission returns a job ID; polling does not wait for inference. Tasks have no deadline by default.
- The official CLI remains autonomous. Default host mode inherits the current user's environment, settings and native automatic permissions. Task context and report formatting add no behavioral policy.
- Acceptance reserves one job identity before dispatch. An optional caller key deduplicates retries, including after response loss, failure or result expiry. The wrapper never automatically replays interrupted work.
- Runtime locks are the authority for ownership. Recovery does not signal a saved PID, avoiding accidental termination after PID reuse.
- Reports, job metadata and raw audit streams have separate retention. Capacity exhaustion rejects new work without silently discarding reports.
- Optional isolation limits mounted resources and permits useful agent tools inside that boundary.

## Shared local state

`broker.rs` stores status in `jobs/UUID/status.json` and terminal reports in `result.json`. Instructions and validated configuration travel to the supervisor through an anonymous stdin pipe; they are not saved as a replayable job request. Enabled audit logging records delegated task bytes separately. Metadata and results remain necessary with audit logging disabled.

Records use temporary files, synchronized writes and atomic replacement. Unix also synchronizes the parent directory. On Windows, transient access or sharing errors during replacement receive a bounded one-second retry; model execution is never retried. These operations assume a local filesystem with working advisory locks and atomic renames. NFS, SMB and cross-machine state sharing are unsupported.

`admission.lock` serializes reservations and shared capacity checks. `execution.lock` permits one CLI task across all instances using this directory. `supervisor.lock` belongs to one supervisor's lifetime. `transition.lock` orders cancellation requests, terminal publication and result disposal. Lock files must never be unlinked while processes may use them.

Queued supervisors use submission time and UUID to select the oldest pending job. An unlimited task can occupy the lane until completion or explicit cancellation. There is no hidden inactivity deadline or preemption. Quota failures set the shared pause marker; queued work waits until `ag_resume`, cancellation or an explicitly requested deadline. Resume does not retry failed jobs.

Idempotency indexes store a hash of the caller key and its job ID. Fingerprints cover submitted parameters, resolved model, effective timeout and CLI path. A different task with the same key returns `IDEMPOTENCY_CONFLICT`. Keys survive result expiry and disposal: an old key returns its original job, never a new inference.

## Execution and isolation

Host mode is the default. The root allowlist checks only the starting directory. Existing CLI hooks, plugins, MCP connections, history and overage settings may apply. The wrapper does not add commits, deployments or rollback operations.

Linux workspace mode copies selected files into a private directory mounted at `/work`. A baseline outside the namespace supplies its patch. Analysis mounts the input copy read-only; writable scratch space remains available. Both use automatic CLI permissions and accept-edits mode. Attempts to modify read-only inputs fail at the mount boundary.

Bubblewrap provides private PID, IPC, UTS and mount namespaces. System programs and selected OS configuration files are read-only. Additional `runtimePaths` are read-only, with their `bin` directories added to PATH. Generated settings omit inherited MCP connections and allow native tools. The helper cache is writable temporary storage, so `agentapi` and downloaded helpers can be created without changing the personal cache.

The original project and personal home contents are absent except for documented runtime and authentication mounts. The existing authentication file is mounted read-only without the wrapper reading or copying its contents. Token renewal can require a writable authentication store; refresh the official login outside isolation when needed. Isolation does not filter the network or protect mounted authentication against hostile commands. Windows and macOS reject isolated requests without host fallback.

## Completion, cancellation and crashes

Supervisors inherit the submitting process's user identity and authentication environment. They do not use a system service or copy credentials. Windows launch requests Job Object breakaway and a detached console context. A parent job prohibiting breakaway produces `BACKGROUND_UNAVAILABLE`; disconnection survival is not promised in that host session. See [Microsoft's process creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags).

The startup handshake is bounded independently of task execution. A supervisor that has not become ready is stopped before startup failure is published. Its key identifies the failed job; a deliberate new attempt needs a new key. An interrupted reservation can briefly remain queued during reconciliation. A supervisor seeing a terminal reservation does not execute it.

`ag_cancel` writes a durable request and waits at most ten seconds for supervised cleanup. If cleanup remains pending, `process_stopped` stays false and the caller polls status. Completed runtime results win over late cancellation; cancellation does not rewrite their report as failure. Terminal states are immutable. Cancellation does not reverse host changes or external operations.

Unix supervision sends TERM to the CLI process group, then KILL after two seconds if required. Linux also requests parent-death termination for the direct CLI process. Windows uses a kill-on-close Job Object assigned before task stdin is delivered. Linux isolated PID namespaces constrain descendants. Host commands can create services or detached Unix sessions outside the supervised group. Abrupt supervisor death on macOS can leave host processes running; reconciliation reports `process_stopped: false` instead of claiming cleanup. See [Microsoft Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).

Lost ownership produces `RUNNER_INTERRUPTED`, preserving available audit material without replay. Reboot continuation, rollback and hostile-process containment are unsupported. Graceful termination and state-write failures request cancellation before exit. Storage exhaustion can prevent a final failure checkpoint; restore storage before querying state.

## Storage and output

Task streams have no cumulative byte ceiling. Each NDJSON event is bounded at 2 MiB, the final report has field limits, and retained diagnostic text is bounded. Raw audit streams rotate without deletion. Version/catalog queries and patch generation retain bounded output. Commands have no wrapper CPU, memory or disk quotas.

Results do not expire by default. Positive `retentionSeconds` starts expiry after completion, never during execution. `ag_forget` discards a terminal result and frees capacity. Metadata, deduplication indexes and enabled audit logs remain. New work fails with `RESULT_STORE_FULL` at `maxJobs`; active work is never evicted.

Unix storage requires current-user ownership and private modes. Windows rejects reparse points and relies on inherited profile ACLs; restrict directories to the intended account. These files are not a security boundary against the same current-user agent. Forced termination can leave temporary `job-*` directories. Inspect them after confirming work is stopped; preserve active lock files.

Each transport and supervisor has a separate audit directory. Result references point to supervisor logs, which continue after transport closure. Enabled audit failure stops affected work without cancelling unrelated supervisors. Logs support inspection; they are not immutable evidence or a replay journal. See [audit logging](audit.md).
