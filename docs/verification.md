# Verification record

## Version 0.3 invocation and output

Local source checks passed twenty Rust checks, seven npm launcher checks, warning-free native Clippy and Windows cross-target Clippy. The native protocol test requires only instructions, resolves the submitting cwd and operates without a configured starting-directory allowlist. Original-answer paging covers plain text, Markdown, unrelated JSON and an old-report-shaped body with invalid evidence; all complete successfully and remain byte-for-byte text. A recovered permission diagnostic no longer overrides terminal CLI success.

The runtime passes instructions unchanged and does not supply --json-schema. The old report types, field validator and preview generator were removed. Existing stored answers remain readable, with historical errors preserved and no automatic replay. A real Flash High task completed in 14,375 ms. It wrote the independently checked five-byte result, returned a 371-character non-JSON answer and reported SUCCESS with exit code zero. Captured CLI input matched the submitted task exactly; the full MCP answer matched the native terminal response exactly. The initial MCP connection closed after submission, and a new connection retrieved the result.

A separate Gemini 3.8 Flash High design discussion completed in 99,660 ms with a 13,027-character native Markdown answer. It supported keeping lifecycle, transport and audit responsibilities in the bridge while leaving answer interpretation to the caller. Its references to the current project make it an advisory discussion, not a blind review or quality benchmark. Captured task text and returned answer both matched their native counterparts exactly.

### Version 0.3 release

Source commit 5aa1c4690c4b534deccf5f7ceb30a67d24aaaf2c passed [all six main-branch native jobs](https://github.com/getio0909/antigravity-worker-mcp/actions/runs/37065732275). The [v0.3.0 tag run](https://github.com/getio0909/antigravity-worker-mcp/actions/runs/37065735534) also passed and published six native archives, the npm tarball and SHA256SUMS. All seven downloaded packages matched the manifest and contained no private configuration, build output or unsafe extraction paths.

The public GitHub-tarball npx command returned 0.3.0, initialized MCP, listed all eight tools and reported only instructions as a required submission field. It also retrieved a retained answer from an older failed job without changing its historical status or replaying it. The unqualified npm registry package remains unpublished.

### Version 0.3 installed clients

The published x86-64 release executables are installed through CC Switch 3.20.4 on Linux and Windows, with Codex and Claude Code enabled. Both machines use the existing private state and full audit logging, host execution, no task deadline, no result expiry and an empty starting-directory allowlist. Windows desktop fallback remains enabled. Database entries and both global client configurations agree; comparisons preserved all other MCP entries, providers, CC Switch settings, remaining client configuration, Claude settings and Codex authentication files.

Fresh Codex sessions used the global registration, submitted only instructions and exited before completion. Fresh Claude Code sessions selected the same registered entry, retrieved each job and independently read the output file. No executable or worker configuration override was supplied to Claude Code.

| Machine | Native task duration | Native answer | Independent file check |
| --- | --- | --- | --- |
| Linux x86-64 | 39,718 ms. | 402 characters, non-JSON. | Five bytes: decimal 1369 followed by a newline. |
| Windows x86-64 desktop | 44,438 ms. | 483 characters, non-JSON. | Five bytes: decimal 1369 followed by a newline. |

Both tasks used gemini-3.8-flash-high and reported completed, CLI SUCCESS, exit code zero and supervised processes stopped. Captured CLI task text matched the MCP submission, the native cwd matched the submitting client, and the full ag_result text matched the native terminal response. Neither dispatch supplied --json-schema or required kind/root. No temporary Windows startup registration remained. These checks establish invocation, lifetime and answer delivery; they do not benchmark model quality.

## Historical version 0.2.1 evidence

Recorded on 2026-10-02 for experimental version 0.2.1. Native CI, real provider execution, client integration and independent file checks provide separate evidence. A completed model report remains unverified until its claims and effects are checked.

### Release and engineering checks

The release source passed [all six main-branch native jobs](https://github.com/getio0909/antigravity-worker-mcp/actions/runs/37041068244). The [v0.2.1 tag run](https://github.com/getio0909/antigravity-worker-mcp/actions/runs/37042870570) also passed and published six native archives, an npm tarball and SHA256SUMS.

| Platform | Native targets | Rust checks |
| --- | --- | --- |
| Linux | x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu | 21 per target. |
| macOS | x86_64-apple-darwin, aarch64-apple-darwin | 20 per target. |
| Windows | x86_64-pc-windows-msvc, aarch64-pc-windows-msvc | 22 per target, repeated three times. |

Each native job checks format, warning-free Clippy, tests, release compilation and archive contents. All runners also run seven dependency-free npm launcher tests. Windows compiles tests with Cargo and executes them through an account-free WMI process outside Cargo's restrictive Job Object. CI disables desktop fallback and performs no authenticated model inference. The positive Windows named-pipe test verifies the executable identity check and in-memory handoff with a same-user child process.

The tag run's ARM64 macOS readiness check initially exceeded a two-second fixture wait. Its rerun passed. A separate main-branch test change now allows ten seconds for the descendant sentinel and includes supervisor diagnostics on failure; the targeted Linux check and [all six native CI jobs for that change](https://github.com/getio0909/antigravity-worker-mcp/actions/runs/37044817421) passed. The published tag remains unchanged.

Engineering coverage includes host permissions, optional Linux isolation, cancellation and supervised descendants, shared FIFO scheduling, quota pause/resume, disconnect and reconnect, retry-key deduplication, conflicting submissions, explicit result disposal, deadlines beyond 900 seconds, late cancellation after completion, storage failures, full audit capture and its off switches. Native output may exceed the former cumulative 2 MiB cap; individual events remain bounded.

All seven downloaded packages matched their published SHA-256 values. Archive inspection found English documentation and licenses, with no private configuration, native fixtures, build output or unsafe extraction paths. The public GitHub-tarball npx command returned version 0.2.1. A subsequent npx connection initialized MCP, listed all eight tools and called ag_capabilities with CLI 1.2.15, host execution, isolation disabled, zero task timeout, zero result expiry and audit logging enabled. npm registry publication remains separate.

### Unlimited background execution

A real Gemini 3.8 Flash High task completed after 1,231,330 ms, with the submitting MCP connection already closed. Its shell operation alone lasted 920 seconds. Fresh stdio connections retrieved the completed result, and an independent read confirmed the 20-byte output file. The result reported the configured model, CLI SUCCESS, exit code zero, no deadline and no expiry. Both transport and detached-supervisor audit streams remained on disk.

Separate actual Codex sessions submitted work and exited before the native CLI finished. Fresh Claude Code sessions retrieved the same job IDs and checked the resulting files.

| Machine | Native task duration | Independent result | Background route |
| --- | --- | --- | --- |
| Linux x86-64 | 67,328 ms. | Four bytes, decimal 437 followed by a newline. | Detached supervisor. |
| Windows x86-64 desktop | 99,118 ms. | Four bytes, decimal 437 followed by a newline. | Current-user interactive task. |

Both tasks used gemini-3.8-flash-high and returned completed, CLI SUCCESS and exit code zero. Results had no deadline or expiry, and supervised processes were stopped. The Windows execution used a native CI artifact built from the exact release source; final installed-release checks then retrieved its retained result.

These checks establish transport-independent execution and shared result retrieval. They do not establish model quality or sustained reliability.

### Windows desktop launch

A real Windows Codex submission with version 0.2.0 failed with BACKGROUND_UNAVAILABLE and OS error 5 before inference. A probe inside the actual MCP process found a kill-on-close Job Object without breakaway permission. An earlier Node-based probe had measured its spawned child's libuv Job Object and did not establish MCP breakaway.

Version 0.2.1 first attempts direct breakaway, then uses a temporary current-user Task Scheduler registration when windowsDesktopFallback is enabled. The actual Codex task exercised this fallback, used the existing interactive Google sign-in and completed after Codex exited. The audit recorded windows-interactive-task and successful registration removal. No temporary registration remained while the model task was still running.

The route requests no elevation, saves no password or startup-input file, and transfers bounded input through a local named pipe restricted to the current user. Both endpoints verify the peer executable and user identity. Scheduled execution has no task-engine lifetime limit or restart policy. Startup cleanup uses a process handle; abandoned registration has an expiry fallback.

Direct launch remains available. The fallback requires a usable interactive sign-in and Task Scheduler permission, and receives profile environment settings rather than every transient parent override. It can be disabled explicitly.

### Linux isolation and actual tools

A real isolated Gemini 3.8 Flash High review completed with native command and file tools. Its captured trace contained fourteen tool events and zero tool errors. Returned command results matched an independently computed source hash and the eight-tool count. Selected inputs remained read-only.

The observed agentapi EROFS failure came from the CLI trying to populate its helper cache under a read-only home mount. A writable temporary helper cache fixed that failure. Automatic CLI permission approval and read-only input mounts remain separate controls. Host execution is still the default.

The shared Linux task's native trace contained two completed run_command operations and one completed view_file operation. Its supervisor audit retained three files totaling 18,587 bytes, all mode 0600. One completed tool event omitted its output payload; full wrapper logging captures what the CLI emits and cannot reconstruct missing provider data.

Earlier autonomous coding checks under version 0.1.2 produced correct JavaScript modules through both clients. Their native agents chose shell commands, reads and edits; independent checks confirmed the modules and unchanged original checks. One earlier task inspected unrelated CLI transcripts without mentioning those reads in its final report. Audit inspection exposed the additional activity. Task wording is not an enforced tool policy, and successful edits do not certify the accuracy or completeness of the report.

### Model discussions and controls

Reviews used the installed official CLI with fixed gemini-3.8-flash-high and claude-opus-4-6-thinking slugs. Current [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp), [Claude Code MCP](https://code.claude.com/docs/en/mcp) and [headless execution](https://code.claude.com/docs/en/headless) documentation informed integration work.

Three completed Flash High rounds examined per-job supervision, shared state, failure recovery, cancellation, quota pause, retry keys and result disposal. Their findings informed fixes for paused queued jobs, completed results during late cancellation, storage-failure cleanup and supervisor reaping.

A fourth completed Flash High round examined the Windows desktop route. It favored explicit refusal and raised questions about interactive-token availability, environment differences, pipe access and registration cleanup. The implementation keeps the route configurable, verifies private IPC, requests no elevation and removes or expires temporary registration. This is an advisory review, not unanimous model endorsement.

Opus completed an initial version 0.1.2 assessment. Later follow-ups encountered provider capacity errors. One attempt hit the old 900-second deadline; an unlimited detached follow-up reached a capacity error after 787,716 ms. Partial output is retained for audit and is not counted as a completed review.

Flash High accepted effort high; effort max conflicted with that model variant. Opus Thinking rejected the effort control, and the installed CLI exposed no selectable maximum reasoning budget for it. No alternate model was substituted and no unavailable reasoning setting is claimed.

### Installed clients and CC Switch

The version 0.2.1 installation used CC Switch 3.20.4 on Linux and Windows, with Codex and Claude Code enabled. Database flags and both client configuration files agreed. Linux used a stable executable symlink; Windows used a versioned native executable. Existing job IDs and audit records were preserved in shared private state.

| Machine | Codex | Claude Code | Antigravity CLI |
| --- | --- | --- | --- |
| Linux | 0.160.0 | 2.1.285 | 1.2.15 |
| Windows | 0.159.0-alpha.12.1 | 2.1.280 | 1.2.15 |

After final installation, fresh sessions in all four clients queried capabilities, retrieved the existing completed task and independently checked its four-byte output. Codex used the global registration. Claude Code selected the same registered entry for a temporary strict MCP connection, without replacing its executable or configuration paths. These sessions submitted no new worker inference.

Capabilities reported version 0.2.1, default host execution, isolation false, audit logging true, task timeout zero and result retention zero. Windows reported desktop fallback enabled. Hash comparisons preserved every other MCP registration, provider record, CC Switch setting, remaining client configuration, Claude settings and Codex authentication file. Existing interactive sessions need to restart to load the eight tools.

## Remaining limits

Real authenticated provider execution was checked on Linux and a signed-in Windows desktop. macOS and Windows ARM64 have native CI coverage, but their real provider/client workflows remain unmeasured. Complex coding quality, research accuracy, submission latency percentiles, quota efficiency and sustained reliability have no benchmark.

Optional isolation is Linux-only. Live cancellation was checked separately through the real CLI and native fixtures, rather than repeated inside all four parent clients. The declared Rust 1.89 minimum has not been compiled; the tested compiler is 1.97.1. Interrupted work after a crash or reboot is reported without automatic replay.

Host agents can access current-user files and services. Cancellation stops supervised work; it does not roll back effects or certify termination of detached external services. Logs are local operational evidence and are not immutable. CLI behavior, model catalogs and client settings can change independently of this wrapper.
