# Changelog

## 0.1.1

- Add a dependency-free npm launcher with six-target selection, SHA-256 verification, atomic local caching and unchanged MCP stdio.
- Publish an npx-compatible launcher tarball with every GitHub release; npm registry publication remains separate.
- Test launcher behavior and extract every native release archive in CI.
- Include contribution, security and changelog documents in every platform archive.
- Derive command-line and capability versions from Cargo package metadata.

This is the first binary release. The 0.1.0 source tag remains available.

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
