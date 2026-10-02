# Security and execution scope

The optional npm launcher downloads versioned assets from this repository's GitHub Releases. It verifies the archive against the release SHA-256 manifest before extraction and verifies cached executable bytes on reuse. These checks rely on the release publisher and HTTPS; checksums are not independent signatures. Unix cache directories reject symlinks and other-user write access. Windows caches use inherited profile ACLs. The launcher forwards the original environment and stdio without reading MCP messages, and has no install hook or third-party npm dependencies.

Default execution is unrestricted host mode: `isolation: false`, automatic CLI approval, the current user's environment, and the original allowed starting directory. Use trusted tasks and materials. The starting-directory allowlist is not a host filesystem sandbox. The official CLI may use its existing hooks, plugins, MCP connections and history settings.

Set `isolation: true` for a separate working copy, or `execution_mode: "analysis"` for read-only inputs. A deployment can set `allowHostExecution: false` to reject host tasks. Bubblewrap failure is an error; it never silently selects host execution.

Isolation requires Linux. Windows and macOS support host mode and reject isolated modes explicitly. Windows state data uses inherited profile ACLs rather than a custom enforced ACL; deployments selecting another directory must restrict its access. Windows input handle checks reject reparse points and paths outside the root, but do not provide Unix-style atomic parent traversal.

Isolation keeps the original input root outside the mount namespace. It exposes system programs, configured toolchains, outbound network access, and the CLI's read-only authentication file. It does not provide an exfiltration barrier against hostile commands, a network firewall, or CPU/disk quotas. Secret-name filtering does not detect all secrets inside otherwise ordinary files.

Full local audit logging is enabled by default. It retains task instructions, selected file bytes, MCP traffic, unfiltered CLI streams, diagnostics and terminal results in private per-connection directories, with rotation and no automatic deletion. These materials can contain sensitive content. The wrapper does not dump environment values or inspect authentication caches, but model/tool output can expose private data. Do not upload audit logs to public issues. See the [audit contract](docs/audit.md).

Use `--no-audit` or `auditLogging: false` to disable wrapper logging completely. No audit metadata or disabled marker is retained in that mode. CLI, MCP-host and provider retention follow their own settings. An enabled logger's storage failure stops dispatch and cancels the affected connection's supervised jobs rather than silently dropping records.

The host-mode agent chooses its own tools and working steps. The wrapper adds task context and report formatting without behavioral restrictions. Task wording does not enforce command or filesystem policy. With current-user authority, the agent can also modify or delete audit files; the trace is not tamper-proof. Inspect actual results and side effects independently.

Isolated CLI history is ephemeral. A forced wrapper termination may leave private temporary files; follow the architecture cleanup instructions.

Cancellation supervises the CLI and process group. Host-mode detached processes, modified files and external side effects can outlive cancellation. Reports are always unverified until independently checked.

For a vulnerability, use the repository's private vulnerability-reporting channel when available. Include affected version, a synthetic reproducer and observed behavior. Do not place credentials, real conversations or sensitive logs in public issues.
