# Changelog

## 0.3.0

- Pass task instructions unchanged and preserve native text, Markdown, code and JSON answers. Remove the required report schema, evidence validator, report prompt and answer rewriting.
- Require only instructions. Make kind an arbitrary optional label, resolve omitted root from the submitting connection, and support an optional native model slug.
- Disable the starting-directory allowlist when allowedRoots is absent or empty; explicit lists remain opt-in.
- Follow terminal CLI status and exit code without overriding successful recovery from intermediate tool or permission diagnostics.
- Remove deprecated summary, finding-preview and limitations fields from ag_result. Existing stored text and historical states remain readable without replay.
- Keep unlimited detached execution, cancellation, optional Linux isolation and full retained audit logging.

## 0.2.1

- Support Windows MCP hosts that refuse direct Job Object breakaway through a temporary current-user Task Scheduler launch. The existing interactive sign-in supplies authentication; no password or task-input file is created.
- Authenticate both endpoints of a private, local-only named pipe and transfer bounded startup input in memory. Startup cleanup uses an open process handle rather than a saved PID.
- Remove temporary task registrations after startup, with an expiry fallback for abandoned registration. Scheduled supervisors have no task-engine execution limit or restart policy.
- Add windowsDesktopFallback, enabled by default; disable it to require direct background launch. Record the selected launch route and launcher diagnostics in enabled audit logs.
- Correct Windows verification: the earlier Node-based probe measured a descendant's Job Object; a direct MCP process probe exposed Codex's restrictive job.

## 0.2.0

- Default task and result retention to unlimited; retain optional positive deadlines without the former 900-second cap.
- Detach each job supervisor from the MCP connection and persist shared status and results for reconnecting Codex and Claude Code clients.
- Add retry keys, shared FIFO scheduling, quota pause/resume, job discovery and explicit terminal-result disposal without deleting audit logs.
- Preserve completed results during late cancellation and stop supervised work after state-write failures. Never automatically replay interrupted work.
- Allow native tools in isolated analysis while enforcing read-only inputs through mounts; provide a writable temporary CLI helper cache.
- Remove obsolete researchDomains settings; existing configurations must remove that field and explicitly set zero timeouts/retention to adopt the new defaults.
- Publish all six native targets and the npx launcher through the existing release pipeline.

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
