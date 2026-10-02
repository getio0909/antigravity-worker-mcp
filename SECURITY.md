# Security and execution scope

The optional npm launcher downloads versioned assets from this repository's GitHub Releases. It verifies the archive against the release SHA-256 manifest before extraction and verifies cached executable bytes on reuse. These checks rely on the release publisher and HTTPS; checksums are not independent signatures. Unix cache directories reject symlinks and other-user write access. Windows caches use inherited profile ACLs. The launcher forwards the original environment and stdio without reading MCP messages, and has no install hook or third-party npm dependencies.

Default execution is unrestricted host mode: `isolation: false`, automatic CLI approval, the current user's environment, and the original working directory. Use trusted tasks and materials. An absent or empty allowedRoots list permits any existing starting directory; an explicit list is not a host filesystem sandbox. The official CLI may use its existing hooks, plugins, MCP connections and history settings.

Set `isolation: true` for a separate working copy, or `execution_mode: "analysis"` for read-only inputs. A deployment can set `allowHostExecution: false` to reject host tasks. Bubblewrap failure is an error; it never silently selects host execution.

Isolation requires Linux. Windows and macOS support host mode and reject isolated modes explicitly. Windows state data uses inherited profile ACLs rather than a custom enforced ACL; deployments selecting another directory must restrict its access. Windows input handle checks reject reparse points and paths outside the root, but do not provide Unix-style atomic parent traversal.

Isolation keeps the original input root outside the mount namespace. It exposes system programs, configured toolchains, outbound network access, and the CLI's read-only authentication file. It does not provide an exfiltration barrier against hostile commands, a network firewall, or CPU/disk quotas. Secret-name filtering does not detect all secrets inside otherwise ordinary files.

Full local audit logging is enabled by default. It retains task instructions, selected file bytes, MCP traffic, unfiltered CLI streams, diagnostics and terminal results in private per-connection directories, with rotation and no automatic deletion. These materials can contain sensitive content. The wrapper does not dump environment values or inspect authentication caches, but model/tool output can expose private data. Do not upload audit logs to public issues. See the [audit contract](docs/audit.md).

Use `--no-audit` or `auditLogging: false` to disable wrapper logging completely. No audit metadata or disabled marker is retained in that mode. CLI, MCP-host and provider retention follow their own settings. An enabled logger's storage failure stops dispatch and cancels the affected supervised execution rather than silently dropping records.

The host-mode agent chooses its own tools and working steps. The wrapper passes task text unchanged and does not impose a report format or evidence schema. Task wording does not enforce command or filesystem policy. With current-user authority, the agent can also modify or delete audit files; the trace is not tamper-proof. Inspect actual results and side effects independently.

Isolated CLI history is ephemeral. A forced wrapper termination may leave private temporary files; follow the architecture cleanup instructions.

Cancellation supervises the CLI and process group. Host-mode detached processes, modified files and external side effects can outlive cancellation. Reports are always unverified until independently checked.

For a vulnerability, use the repository's private vulnerability-reporting channel when available. Include affected version, a synthetic reproducer and observed behavior. Do not place credentials, real conversations or sensitive logs in public issues.

Accepted jobs outlive their submitting client. Shared private state grants the same current-user clients cross-client lookup and cancellation. Keep this directory local and private. Operational metadata and final results persist even when audit logging is off; original task inputs are not saved as replay requests. Results do not expire by default and reach a configured capacity; explicit disposal preserves audit logs and retry records. Abrupt runner failure is reported without automatic task replay or a claim that all host processes stopped.

Windows desktop fallback is enabled by default. It can use the built-in Task Scheduler when a client-owned Job Object prevents direct detached launch. This changes task lifetime beyond that client job, while retaining the current user's interactive identity and requesting no elevation. Set `windowsDesktopFallback: false` when deployment policy requires direct launch. The temporary registration contains no task text or credentials and is removed after startup or expires. Startup IPC is local-only, restricted to the current user, and verifies peer identity and executable. Same-user agents remain outside this security boundary. The desktop route requires an existing interactive sign-in and uses the profile environment rather than transient MCP-process overrides.
