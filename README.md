# Antigravity Worker MCP

[Product requirements](docs/RPD.md) | [Protocol](docs/protocol.md) | [Verification](docs/verification.md)

A Rust MCP worker for the official Google Antigravity CLI. Codex, Claude Code, and other stdio MCP clients can submit background tasks, check status, read reports, and cancel execution. The wrapper ships as one executable and does not need Node.js.

**Default: no isolation, full current-user permissions, automatic execution.** Tasks run in the original configured workspace with `--dangerously-skip-permissions`. Set **`isolation: true`** for a separate working copy, or select **`execution_mode: "analysis"`** for read-only inputs.

Headless mode by itself does not approve every tool action. The wrapper passes the explicit automatic-approval flag to the official `agy`; no `agy-yolo` alias or separate executable is required.

Version 0.1 is experimental. It uses your installed `agy` and cached login on Linux, macOS or Windows. This community project has no Google affiliation and does not implement Google's private model APIs or change account overage settings.

## Why this exists

I built this because the Google Antigravity CLI in my Google One Ultra 20X subscription is awkward to use. The model allowance is generous; my patience with the CLI is less so. This wrapper lets Codex or Claude Code handle task submission, progress and reports through MCP, while the official CLI does the model work.

## Install

Download the archive matching your operating system and architecture from [GitHub Releases](https://github.com/getio0909/antigravity-worker-mcp/releases), verify its SHA-256 against `SHA256SUMS`, and extract it. Archives contain the executable, licenses, notices and English documentation. The official CLI is installed separately.

| Platform | x86-64 target | ARM64 target | Execution modes |
| --- | --- | --- | --- |
| Linux, glibc | `x86_64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` | Host, workspace, analysis. |
| macOS | `x86_64-apple-darwin` | `aarch64-apple-darwin` | Host. |
| Windows | `x86_64-pc-windows-msvc` | `aarch64-pc-windows-msvc` | Host. |

CI runs native compilation, lint, tests and packaging for all six targets. Linux/macOS archives use `.tar.gz`; Windows archives use `.zip` and contain `antigravity-worker-mcp.exe`. Tag releases publish all six archives together with checksums. See the [verification record](docs/verification.md) for actual tested environments and remaining gaps.

Alternatively, use an existing Rust toolchain 1.89 or newer:

```bash
git clone https://github.com/getio0909/antigravity-worker-mcp.git
cd antigravity-worker-mcp
cargo build --locked --release --bin antigravity-worker-mcp
./target/release/antigravity-worker-mcp --version
cp examples/config.example.json config.local.json
```

Install the official CLI through [Google's installation documentation](https://antigravity.google/docs/cli/install/) and authenticate once with interactive `agy`. List your available model slugs:

```bash
agy models
```

Edit `config.local.json`: set absolute executable and project paths, then replace the model placeholders with valid slugs. Local configurations are ignored by Git. Default host mode needs the official CLI; optional isolation also needs Bubblewrap, working unprivileged user namespaces, and Git for patches.

On Windows, use [the Windows configuration example](examples/config.windows.example.json), the actual installed `agy.exe` path and absolute Windows paths. JSON backslashes must be escaped. Client registration accepts the `.exe` path in place of the Unix executable path shown below.

```bash
antigravity-worker-mcp --config /absolute/path/config.local.json
```

The server speaks MCP on stdin/stdout. A terminal without a client waits for input. Configuration errors go to stderr as sanitized JSON.

## Add to Codex or Claude Code

```bash
codex mcp add antigravity-worker -- /absolute/path/antigravity-worker-mcp --config /absolute/path/config.local.json
claude mcp add --scope user antigravity-worker -- /absolute/path/antigravity-worker-mcp --config /absolute/path/config.local.json
```

Restart an existing session to load the server. Codex `/mcp` shows connection status; `claude mcp list` checks Claude Code's configured connection. See the [Codex documentation](https://learn.chatgpt.com/docs/extend/mcp) and [Claude Code documentation](https://code.claude.com/docs/en/mcp) for host-specific settings.

To remove registrations:

```bash
codex mcp remove antigravity-worker
claude mcp remove --scope user antigravity-worker
```

The [verification record](docs/verification.md) distinguishes tested MCP protocol behavior from actual host-client sessions. Installing this repository does not modify either client's global configuration automatically.

## Tools

| Tool | Behavior |
| --- | --- |
| `ag_submit` | Queue a task and return its ID without waiting for the model. |
| `ag_status` | Read state, deadlines, progress counters and failure indicators. |
| `ag_result` | Read an unverified report or patch in character pages. |
| `ag_cancel` | Cancel and wait for supervised runtime termination and cleanup. |
| `ag_capabilities` | Query CLI version, live model catalog, permission modes and limits. |

Default automatic execution:

```json
{
  "kind": "code",
  "root": "/absolute/path/to/project",
  "instructions": "Implement the scoped change and report the checks performed.",
  "isolation": false,
  "model_profile": "fast",
  "timeout_seconds": 300
}
```

Optional working copy:

```json
{
  "kind": "code",
  "root": "/absolute/path/to/project",
  "files": ["src/example.rs", "Cargo.toml"],
  "instructions": "Implement the scoped change and report the checks performed.",
  "isolation": true
}
```

Follow `next_offset` in `ag_result`; use `section: "patch"` for the working-copy patch. Review the complete patch before applying it. All model reports carry `verification_status: "unverified"`; the host agent must verify findings and test claims.

Isolated mode copies only selected UTF-8 files, up to 100 files, 256 KiB each and 4 MiB total. It does not copy the complete repository, `.git`, local settings, secrets or installed dependencies. Include materials needed for checks and use `runtimePaths` to mount additional toolchain directories read-only. Host mode can use the original full project.

## Execution and data

| Mode | File scope | Commands / network | Auto approval |
| --- | --- | --- | --- |
| `host` (default, `isolation: false`) | Original directory and current-user host access | Enabled; inherits launch environment and CLI settings | Enabled |
| `workspace` (`isolation: true`) | Writable copy of selected files | Enabled inside Bubblewrap | Enabled |
| `analysis` | Read-only copy | Commands denied; URL reads limited by configured domains | Disabled |

Workspace and analysis modes require Linux. macOS and Windows reject isolation requests with `ISOLATION_UNSUPPORTED`; they do not silently run them on the host.

`allowHostExecution` defaults to true. Set it to false to reject host tasks. The host root allowlist validates the starting directory, not paths later accessed by commands. Client-side tool approval remains controlled by the MCP host.

Isolated execution keeps the original project outside the mount namespace, but exposes system programs, configured runtimes, network access and the CLI's read-only authentication file. It is not a network firewall or an exfiltration barrier against hostile tasks. Host cancellation covers the supervised CLI and process group; detached services and external side effects can remain.

The wrapper does not persist instructions or raw conversations. Results stay in memory, expire after one hour by default, and clear when the connection closes. Isolated CLI history is temporary. Default host mode uses the official CLI's normal history and retention settings. Host clients and Google's services have their own retention behavior.

Instances sharing one `stateDirectory` share one execution lock and serialize CLI jobs. Each connection owns its queue and results. Different directories or machines have independent locks. Quota/capacity failures pause dispatch only in the affected connection. Token usage is reported when available; remaining subscription quota is unknown.

## Development

```bash
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --locked --release --bin antigravity-worker-mcp
```

Tests use a native CLI fixture and real processes, Bubblewrap, advisory locks and stdio MCP. They do not require Google authentication or invoke a model. See [architecture](docs/architecture.md), [contributing](CONTRIBUTING.md), [security](SECURITY.md), and the [complete RPD](docs/RPD.md).

The project uses the [MIT license](LICENSE). Dependencies retain their own licenses. Google Antigravity and its models remain subject to their service terms. GitHub hosts source and binary releases; crates.io and MCP Registry publication are separate roadmap items.
