# Verification record

Recorded on 2026-10-02 for experimental version 0.1.0. A passed engineering check establishes the behavior described in that check, not the accuracy of arbitrary model tasks.

## Local engineering checks

Environment: Linux x86-64, glibc 2.41, Rust and Cargo 1.97.1, official RMCP 3.5.0, Bubblewrap and working unprivileged user namespaces.

The format and warning-free Clippy checks passed, along with fifteen tests (three unit tests and twelve integration tests). Tests cover default host permissions and edits; optional isolated patches and unchanged originals; analysis write denial; deadlines; malformed and excessive output; queued and running cancellation; supervised descendant termination; quota pause; model and result limits; shared execution locks; input path, link and text checks; Unicode pagination; and an actual RMCP stdio client connection.

CI is configured to run native tests and package releases on these six targets:

| Target | Runner | Intended checks |
| --- | --- | --- |
| x86_64-unknown-linux-gnu | ubuntu-22.04 | Host, isolation, protocol and archive. |
| aarch64-unknown-linux-gnu | ubuntu-22.04-arm | Host, isolation, protocol and archive. |
| x86_64-apple-darwin | macos-15-intel | Host, isolation rejection, protocol and archive. |
| aarch64-apple-darwin | macos-15 | Host, isolation rejection, protocol and archive. |
| x86_64-pc-windows-msvc | windows-2025 | Host, Job Objects, isolation rejection, protocol and archive. |
| aarch64-pc-windows-msvc | windows-11-arm | Host, Job Objects, isolation rejection, protocol and archive. |

Check the repository's Actions run for current outcomes. A configured matrix alone is not evidence that every job has passed.

## Official CLI checks

The installed official CLI was version 1.2.14. A capability query returned fourteen model entries without inference, with default host mode and `default_isolation: false`.

| Check | Observed result |
| --- | --- |
| Structured task through an official TypeScript MCP SDK client connected to the Rust server. | Completed with a validated structured report; actual model was gemini-3.8-flash-medium. |
| Isolated single-file edit through the Rust release executable. | Completed; original bytes unchanged; returned patch contained the actual edit. |
| Cancellation after real CLI progress began. | Returned cancelled with `process_stopped: true`. |

Observed usage for the structured sample was 12,504 input, 238 output, 195 thinking and 12,742 total tokens. For the file-edit sample it was 25,419 input, 468 output, 342 thinking and 25,887 total tokens. These are CLI-reported fields; no subscription quota or price is inferred from them. No cached authentication values or real task instructions are included in this record.

The provider checks used Linux isolation and cached official authentication. Native fixture tests separately verify automatic default host edits. The TypeScript SDK was a test client only; deployment has no Node.js dependency.

## Unverified behavior and limits

- Complete real-provider workflows inside actual Codex and Claude Code sessions have not been tested. Registration commands follow their official documentation; SDK stdio interoperability has been tested.
- Real-provider host execution, provider authentication on macOS/Windows, complex coding quality and research accuracy have not been measured.
- The declared Rust 1.89 minimum has not been compiled locally; the tested and pinned CI compiler is 1.97.1.
- No benchmark establishes submission P95, task success rate, quota efficiency or remaining account allowance.
- macOS/Windows isolation, a shared result broker, durable restart recovery, strict interpreter controls and command resource quotas are outside version 0.1.
- Host execution can affect original files and external services. Supervised cancellation does not certify rollback or termination of detached Unix sessions or services started outside the supervised runtime.

The release is experimental. Tests require no Google credentials and do not consume provider quota. The official CLI and model catalogs can change independently of this wrapper.
