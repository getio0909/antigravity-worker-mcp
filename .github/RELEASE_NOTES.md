Experimental Rust MCP worker for the official Google Antigravity CLI.

- Five asynchronous tools for submission, status, reports, cancellation and capabilities.
- Default host execution with full current-user permissions and automatic CLI approval.
- Optional Linux workspace isolation, paged patches and read-only analysis.
- Native CI builds and tests for Linux, macOS and Windows on x86-64 and ARM64.
- Optional dependency-free npx launcher, published as a GitHub release tarball.
- Full private audit logs by default, rotation without automatic deletion, and an explicit complete off switch.
- Autonomous agent execution without added behavioral instructions; live Codex and Claude Code checks with independently inspected results and traces.

Download the archive matching your operating system and architecture, and verify it against `SHA256SUMS`. Archives include the executable, MIT license, dependency notices and English documentation. Install and authenticate the official `agy` separately, then configure its executable path and model profiles.

The README documents npx usage with the included npm tarball URL. That entry point does not need an npm registry publication; the unqualified npm package name is not yet published. The launcher verifies and caches the native executable and passes MCP stdio through unchanged.

Isolation requires Linux. Windows and macOS support host mode and explicitly reject isolation requests. See the README, complete RPD and verification record for permission boundaries and checks that remain unverified. This project has no Google affiliation.
