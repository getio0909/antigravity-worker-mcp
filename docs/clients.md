# Codex and Claude Code integration

The server uses stdio MCP and five asynchronous tools. Each client connection owns its jobs and in-memory results. Keep the submitting connection alive through status polling and result retrieval. A shared `stateDirectory` serializes CLI execution across clients, but does not share their job IDs.

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

`claude --print --no-session-persistence` supports headless invocation without saving the host session. MCP-tool approval remains a Claude Code setting; `--allowedTools` can authorize the selected server's tools for an invocation. These controls do not disable wrapper audit logging. Existing Claude account authentication and the CLI's Google authentication are independent.

Server working directories depend on Claude Code's configuration scope. Use absolute server/configuration paths and explicit configured `allowedRoots` plus task `root`; do not infer the project root from the server's inherited directory. The headless `--bare` mode skips OAuth/keychain lookup, so it is unsuitable for a check relying on an existing subscription login without separate credentials. See [Claude Code MCP](https://code.claude.com/docs/en/mcp) and [headless execution](https://code.claude.com/docs/en/headless).

## npx and platform details

Replace the native command with the versioned npx command in the [README](../README.md). On Windows, use `cmd /c npx` to launch npm's command shim. An initial network download adds startup latency; run the README's `--version` command once before adding the server.

Linux supports all three execution modes. macOS and Windows explicitly reject workspace/analysis isolation rather than switching to host mode. Client connection success does not establish model correctness or task-scope compliance. Inspect the actual changes and [audit trace](audit.md), including when a task returns `completed`.
