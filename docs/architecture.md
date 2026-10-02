# Architecture and operation

The Rust executable hosts the official RMCP stdio server. Each connection owns a bounded job store and one Tokio execution lane. Rust's native file-lock API on `stateDirectory/execution.lock` coordinates one active CLI task across instances using the same directory, using Unix file locks or Windows file locking. Handles close on process exit, so a crash releases the execution lock. Queues and results are not shared.

`model.rs` defines configuration, task and report contracts. `snapshot.rs` captures selected inputs and validates evidence. Unix input traversal uses directory descriptors and `openat`; Windows rejects reparse points and validates the opened handle's final path and link count. `platform.rs` provides file locks, permissions and process supervision. `runtime.rs` launches the CLI, handles streams, provides optional Linux Bubblewrap mounts, and creates patches. `jobs.rs` coordinates deadlines, cancellation, retention and quota pause. `server.rs` maps these operations to MCP tools. `main.rs` handles configuration and connection shutdown.

## Default host execution

`isolation: false` is the default. The CLI starts in an allowed original directory, inherits the launch environment, and receives automatic approval. Existing CLI settings, plugins, MCP connections, hooks, history, and provider overage settings may apply. The root allowlist validates only the starting directory. It does not limit later host access.

The wrapper does not add Git commits, pushes, deployments, or rollback steps on its own. A fully authorized CLI task can perform those operations through its commands. The caller must define its intended scope.

## Optional isolated execution

`isolation: true` copies explicitly selected files into a private temporary directory and mounts it at `/work`. The original root is not mounted. A preserved baseline outside the namespace supplies patch generation. `analysis` mounts the same input read-only.

These modes require Linux. Windows and macOS reject isolation requests explicitly; the wrapper never translates an unavailable isolated mode into host execution.

Bubblewrap creates private PID, IPC, UTS and mount namespaces. System program directories and a short list of OS configuration files are mounted read-only. Temporary files and CLI history live inside ephemeral mounts. The existing authentication file is mounted read-only without the wrapper reading its contents; the CLI binary cache is also mounted read-only. Additional `runtimePaths` are mounted read-only at their original paths, with their `bin` directories added to PATH.

The isolated CLI receives generated settings rather than personal settings. Workspace commands and networking are allowed; inherited MCP connections are omitted. Analysis denies commands, file writes and MCP, and grants URL reading only to configured domains. These CLI policies supplement the mount boundary. Network access is not filtered by an OS firewall. Workspace commands can access the mounted authentication and runtime files; isolation does not promise protection against hostile tasks stealing those contents.

Token renewal can fail when the provider needs to update the read-only authentication file. Refresh the login through an interactive official CLI session outside the wrapper. The wrapper does not copy tokens into an alternative credential store.

## Lifecycle and limits

Arguments are passed directly to a subprocess without shell interpolation. Instructions travel through stdin. Output and stderr have a combined 2 MiB ceiling; only bounded diagnostics stay in memory for error classification. A task deadline includes its wait for the execution lane and host lock.

Unix cancellation sends TERM to the supervised process group, then KILL after two seconds if needed. Windows assigns the CLI to a kill-on-close Job Object before delivering task stdin; cancellation terminates the job. A very short-lived metadata process can exit before assignment and is accepted only if its exit is confirmed. Windows assignment is not a hostile-process startup isolation boundary. Linux isolated PID namespaces constrain descendants. Host tasks can create an external service or, on Unix, a detached session outside the supervised group. Cancellation does not reverse file changes or external operations. See the [Rust file-lock documentation](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock) and [Microsoft Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).

Unix state directories require current-user ownership and mode 0700. Windows uses `%LOCALAPPDATA%/antigravity-worker-mcp` by default and inherits its profile ACL. The wrapper rejects reparse points but does not audit or replace Windows ACLs; use a directory restricted to the intended account. Windows input checks validate the final handle inside its configured root, but do not promise the same atomic parent traversal as Unix `openat`.

The wrapper removes temporary directories on normal completion, failure and cancellation. A process crash or SIGKILL can leave `job-*` or `probe-*` directories in the private state directory. Stop all instances and inspect directory ownership before removing those remnants. Do not remove `execution.lock` while instances are active: unlinking an advisory lock can allow two groups of processes to lock different inodes.

Results remain in memory and expire after the configured retention interval, with a one-second sweep. Closing the connection cancels jobs and clears results. There is no persistent recovery or idempotency key in version 0.1.

Quota/capacity failures stop new dispatch only in the affected connection. The host-wide lock still serializes other connections, but does not share the pause decision. Different machines or state directories have independent locks. A shared broker is a later milestone.

Arbitrary host/workspace commands do not have cgroup CPU, memory or disk quotas. Input and output limits control material capture and delivery, not every possible command side effect. Runtime policies for languages and environment managers need project-specific enforcement when required.
