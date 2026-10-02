# Security and execution scope

The optional npm launcher downloads versioned assets from this repository's GitHub Releases. It verifies the archive against the release SHA-256 manifest before extraction and verifies cached executable bytes on reuse. These checks rely on the release publisher and HTTPS; checksums are not independent signatures. Unix cache directories reject symlinks and other-user write access. Windows caches use inherited profile ACLs. The launcher forwards the original environment and stdio without reading MCP messages, and has no install hook or third-party npm dependencies.

Default execution is unrestricted host mode: `isolation: false`, automatic CLI approval, the current user's environment, and the original allowed starting directory. Use trusted tasks and materials. The starting-directory allowlist is not a host filesystem sandbox. The official CLI may use its existing hooks, plugins, MCP connections and history settings.

Set `isolation: true` for a separate working copy, or `execution_mode: "analysis"` for read-only inputs. A deployment can set `allowHostExecution: false` to reject host tasks. Bubblewrap failure is an error; it never silently selects host execution.

Isolation requires Linux. Windows and macOS support host mode and reject isolated modes explicitly. Windows state data uses inherited profile ACLs rather than a custom enforced ACL; deployments selecting another directory must restrict its access. Windows input handle checks reject reparse points and paths outside the root, but do not provide Unix-style atomic parent traversal.

Isolation keeps the original input root outside the mount namespace. It exposes system programs, configured toolchains, outbound network access, and the CLI's read-only authentication file. It does not provide an exfiltration barrier against hostile commands, a network firewall, or CPU/disk quotas. Secret-name filtering does not detect all secrets inside otherwise ordinary files.

The wrapper does not persist instructions, raw events, or diagnostics. Isolated CLI history is ephemeral; host-mode official history, MCP-host retention and provider-side retention follow their own settings. A forced wrapper termination may leave private temporary files; follow the architecture cleanup instructions.

Cancellation supervises the CLI and process group. Host-mode detached processes, modified files and external side effects can outlive cancellation. Reports are always unverified until independently checked.

For a vulnerability, use the repository's private vulnerability-reporting channel when available. Include affected version, a synthetic reproducer and observed behavior. Do not place credentials, real conversations or sensitive logs in public issues.
