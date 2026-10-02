# Changelog

## 0.1.2

- Retain full private MCP/CLI audit streams, selected inputs and terminal results by default, including after connection shutdown.
- Add configurable audit storage and lossless rotation without automatic deletion; `--no-audit` completely overrides logging.
- Stop affected supervised execution after enabled audit I/O fails, and return local audit references, exit codes and duration.
- Keep the underlying CLI autonomous: add task context and report formatting without extra behavioral instructions.
- Verify live workflows with installed Codex and Claude Code; document observed working steps independently of successful edits.
- Add current client integration guidance and disclose unavailable reasoning controls and provider capacity errors in the verification record.
- Describe result-page limits in MCP metadata after live client calls exposed oversized-page retries.
- Verify the combined release checksum manifest through the launcher's exact filename parser before publication.
- Fix Windows launcher-test path comparison by canonicalizing both paths and derive MCP server metadata from Cargo's version.

This is the first binary release. The 0.1.0 and 0.1.1 source tags remain available.

## 0.1.1

- Add a dependency-free npm launcher with six-target selection, SHA-256 verification, atomic local caching and unchanged MCP stdio.
- Publish an npx-compatible launcher tarball with every GitHub release; npm registry publication remains separate.
- Test launcher behavior and extract every native release archive in CI.
- Include contribution, security and changelog documents in every platform archive.
- Derive command-line and capability versions from Cargo package metadata.

The source tag remains available; its release workflow was stopped before binary publication.

## 0.1.0

- Rust stdio MCP server with five asynchronous task tools.
- Default host execution with no isolation and automatic CLI approval.
- Optional `isolation: true` workspace copy and read-only analysis mode.
- Bounded queue, connection-local results, host-wide execution lock, cancellation and deadlines.
- Structured reports, evidence bounds, token usage and paged workspace patches.
- Complete English RPD, integration documentation, native fixtures and CI.
- Native Linux, macOS and Windows builds for x86-64 and ARM64, with checked archives and automatic tag releases.
- Portable file locks, Unix process-group supervision and Windows Job Objects.

This is an experimental release. The verification record lists tested behavior and remaining gaps.
