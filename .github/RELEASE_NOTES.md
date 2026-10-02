Experimental Rust MCP worker for the official Google Antigravity CLI.

- Eight tools for background delegation, status, native answers, cancellation, discovery, quota control and result disposal.
- Default host execution with full current-user permissions and automatic CLI approval.
- No default task deadline; jobs survive client disconnects and results are shared across fresh client sessions.
- Idempotent retries, shared FIFO scheduling and quota pause, with explicit cancellation and no automatic replay.
- Optional Linux workspace isolation, paged patches and read-only analysis with useful native tools and writable helper caches.
- Native CI builds and tests for Linux, macOS and Windows on x86-64 and ARM64.
- Optional dependency-free npx launcher, published as a GitHub release tarball.
- Full private audit logs by default, rotation without automatic deletion, and an explicit complete off switch.
- Task instructions and native answers pass through unchanged, without a required report schema; live Codex and Claude Code checks with independently inspected results and traces.

Download the archive matching your operating system and architecture, and verify it against `SHA256SUMS`. Archives include the executable, MIT license, dependency notices and English documentation. Install and authenticate the official `agy` separately, then configure its executable path and model profiles.

The README documents npx usage with the included npm tarball URL. That entry point does not need an npm registry publication; the unqualified npm package name is not yet published. The launcher verifies and caches the native executable and passes MCP stdio through unchanged.

Isolation requires Linux. Windows and macOS support host mode and explicitly reject isolation requests. See the README, complete RPD and verification record for permission boundaries and checks that remain unverified. This project has no Google affiliation.

Upgrading from 0.1: remove the obsolete researchDomains field and set timeoutSeconds and retentionSeconds to zero in existing configuration files to adopt unlimited task duration and result retention. Results have bounded capacity; ag_forget explicitly frees terminal-result slots while preserving logs and deduplication. Windows supports direct Job Object breakaway or the configurable current-user desktop fallback. Upgrading to 0.3 removes required report fields and makes kind an optional label. Set allowedRoots to an empty list to disable an existing starting-directory allowlist.
