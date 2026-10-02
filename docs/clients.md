# Codex and Claude Code integration

The server uses stdio MCP and eight asynchronous tools. Accepted jobs run independently of client connections. Use the same private `stateDirectory` in Codex and Claude Code to share IDs, status, results and cancellation. Closing a client does not cancel its jobs. Results remain until explicit disposal or configured expiry.

The official client documentation was checked on 2026-10-02. Local acceptance used Codex 0.160.0, Claude Code 2.1.285 and the official Antigravity CLI 1.2.14. See the [verification record](verification.md) for observed outcomes and gaps.

## Codex

Register the native executable:

```bash
codex mcp add antigravity-worker -- /absolute/path/antigravity-worker-mcp --config /absolute/path/config.local.json
```

An explicit TOML configuration can set startup and tool-call deadlines:

```toml
[mcp_servers.antigravity-worker]
command = "/absolute/path/antigravity-worker-mcp"
args = ["--config", "/absolute/path/config.local.json"]
required = true
startup_timeout_sec = 120
tool_timeout_sec = 60
default_tools_approval_mode = "auto"
```

The last setting authorizes MCP calls within Codex; it is separate from the wrapper's automatic CLI approval. Review its fit for the intended deployment. The documented default startup timeout is ten seconds and tool timeout is sixty seconds. An initial npx download can take longer: prewarm the cache or increase `startup_timeout_sec`. Task deadlines remain separate because submission and status calls do not wait for inference. See [Codex MCP configuration](https://learn.chatgpt.com/docs/extend/mcp).

For a temporary acceptance session, Codex supports `exec --ephemeral` and a per-invocation `--config` MCP table. This allows testing without changing global registrations. Restart existing interactive sessions after registration and inspect `/mcp`.

## Claude Code

Register the native executable with user scope:

```bash
claude mcp add --scope user --transport stdio antigravity-worker -- /absolute/path/antigravity-worker-mcp --config /absolute/path/config.local.json
```

For a temporary connection, pass a JSON configuration using `--mcp-config` and `--strict-mcp-config`:

```json
{
  "mcpServers": {
    "antigravity-worker": {
      "type": "stdio",
      "command": "/absolute/path/antigravity-worker-mcp",
      "args": ["--config", "/absolute/path/config.local.json"]
    }
  }
}
```

`claude --print --no-session-persistence` supports headless invocation without saving the host session. MCP-tool approval remains a Claude Code setting; `--allowedTools` can authorize the selected server's tools for an invocation. For later sessions, add `mcp__antigravity-worker__*` to `permissions.allow` in the user settings, preserving existing rules. This authorizes all tools from this server. See [Claude Code permissions](https://code.claude.com/docs/en/permissions). These controls do not disable wrapper audit logging. Existing Claude account authentication and the CLI's Google authentication are independent.

Server working directories depend on Claude Code's configuration scope. Use absolute server/configuration paths and explicit configured `allowedRoots` plus task `root`; do not infer the project root from the server's inherited directory. The headless `--bare` mode skips OAuth/keychain lookup, so it is unsuitable for a check relying on an existing subscription login without separate credentials. See [Claude Code MCP](https://code.claude.com/docs/en/mcp) and [headless execution](https://code.claude.com/docs/en/headless).

## CC Switch

Open the MCP manager in CC Switch, add a server named `antigravity-worker`, and enter the following configuration with the actual executable and configuration paths:

```json
{
  "type": "stdio",
  "command": "/absolute/path/antigravity-worker-mcp",
  "args": ["--config", "/absolute/path/config.local.json"],
  "startup_timeout_sec": 120,
  "tool_timeout_sec": 60,
  "default_tools_approval_mode": "auto"
}
```

Enable both **Codex** and **Claude Code** for this entry. CC Switch stores the registration and writes the client configurations: `~/.codex/config.toml` for Codex and `~/.claude.json` for Claude Code. The timeout and default approval fields configure Codex. Claude Code 2.1.285 accepted the same shared entry in the local check; its tool permissions are configured separately as described above. Keep the Google login in the official CLI's native storage.

Start new client sessions, then check `codex mcp get antigravity-worker --json` and `claude mcp get antigravity-worker`. Call `ag_capabilities` from each client to confirm the connection, selected profiles and logging state. A local CC Switch 3.20.4 build and both installed clients passed this check; see the [verification record](verification.md).

Manage removal through CC Switch so its stored registration and both client files stay consistent. Disabling the entry does not delete retained audit logs. See the [official CC Switch MCP guide](https://github.com/farion1231/cc-switch/blob/main/docs/user-manual/en/3-extensions/3.1-mcp.md).

## npx and platform details

Replace the native command with the versioned npx command in the [README](../README.md). On Windows, use `cmd /c npx` to launch npm's command shim. An initial network download adds startup latency; run the README's `--version` command once before adding the server.

Linux supports all three execution modes. macOS and Windows explicitly reject workspace/analysis isolation rather than switching to host mode. Client connection success does not establish model correctness or task-scope compliance. Inspect the actual changes and [audit trace](audit.md), including when a task returns `completed`.

## Background delegation

Version 0.2 separates short MCP control requests from unlimited task execution. Keep a normal per-call timeout for submission, polling and cancellation; increasing it to cover the full task is unnecessary. `timeoutSeconds: 0` is the default task lifetime. Supply an `idempotency_key` for uncertain retries, use `ag_list` to find IDs after reconnecting, and explicitly cancel unwanted work. `ag_forget` frees completed-result capacity while preserving enabled audit logs.

Windows requires the MCP launcher to permit Job Object breakaway. A launcher that forbids it produces `BACKGROUND_UNAVAILABLE` rather than silently tying work to client lifetime. Authentication follows the launching user session; an SSH or service session may not have the desktop keychain context. Test actual Codex and Claude Code sessions, not only an SSH shell.

On Windows, register the built `.exe` directly rather than launching the server through `cargo run`: Cargo creates its own restrictive Job Object. The Windows CI harness compiles tests first and runs their executables separately.
