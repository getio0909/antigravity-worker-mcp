# Verification record

## Version 0.2 background lifecycle

Linux engineering checks cover detached stdio reconnection, shared lookup and cancellation, identical retry keys, conflicting keys, deadlines longer than 900 seconds, queued quota pause/resume, terminal-result disposal with deduplication preserved, and streams exceeding the former cumulative 2 MiB limit. Local warning-free Clippy, twenty-one Rust checks and seven launcher tests passed.

A real Flash High task completed after 1,231,330 ms with the submitting MCP connection already closed. Its shell operation alone took 920 seconds. Fresh stdio connections retrieved the completed result, and the 20-byte output file matched independently. The terminal result reported the exact configured model, CLI `SUCCESS`, exit code zero, no deadline and no result expiry. Both the submitting transport audit and detached supervisor audit remained on disk.

Three native Gemini 3.8 Flash High design rounds compared daemon and per-job supervision, then examined lifecycle and failure scenarios. Corrections include retaining paused queued jobs, preserving completed reports during late cancellation, supervising storage-failure cleanup and reaping exited supervisors. These are independent reviews rather than a task-quality certification.

A real isolated Gemini 3.8 Flash High review completed with native command and file tools. The final captured trace contained fourteen tool events and zero tool errors, with always-proceed permissions. The command results matched an independently computed source hash and eight-tool count. A writable temporary helper cache removed the observed agentapi read-only-filesystem error; selected inputs remained read-only.

The initial Windows CI launch failed with OS error 5 because the runner's Job Object prohibited breakaway. Windows CI now uses an account-free WMI process outside that runner job, with the official [process startup flags](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-processstartup). An additional test explicitly places the MCP adapter in a restrictive job and checks that it refuses dispatch, reports the stopped process and preserves the retry key. The production launcher retains the host restriction; it does not change Job Object policy or install a service.

The first Opus follow-up hit the old 900-second deadline. A second run used the detached, unlimited backend and reached a provider capacity error after 787,716 ms. Its partial output remains auditable and is not counted as a completed design review. The three completed Flash High rounds inform the version 0.2 design; the completed Opus assessment recorded below applies to version 0.1.2.

Additional live background, release and deployment checks are recorded below when completed.

## Version 0.1.2 historical acceptance

Recorded on 2026-10-02 for experimental version 0.1.2. Engineering checks establish the tested runtime behavior. Product acceptance also requires actual client calls, inspection of changed files and review of the execution trace.

## Local engineering checks

Environment: Linux x86-64, glibc 2.41, Rust and Cargo 1.97.1, official RMCP 3.5.0, Bubblewrap and working unprivileged user namespaces.

Format, warning-free Clippy and eighteen Rust tests passed: three unit tests and fifteen integration tests. Coverage includes host edits and permissions; isolated patches and unchanged originals; analysis write denial; deadlines; malformed/excessive output; cancellation and supervised descendants; quota pause; shared execution locks; input-path checks; Unicode pagination; a real RMCP stdio connection; full audit capture surviving shutdown; both complete off switches; byte-preserving rotation; and cancellation after an actual audit write failure.

All six native jobs and the release publisher passed in the [v0.1.2 tag run](https://github.com/getio0909/antigravity-worker-mcp/actions/runs/36993481261). Each target passed compilation, lint, tests and archive checks. Rust test counts were eighteen on Linux and seventeen on macOS/Windows, where unsupported isolation is rejected. The matching source commit also passed the main-branch CI run.

| Target | Native runner | Checks |
| --- | --- | --- |
| x86_64-unknown-linux-gnu | ubuntu-22.04 | Host, isolation, audit, protocol, launcher and archive. |
| aarch64-unknown-linux-gnu | ubuntu-22.04-arm | Host, isolation, audit, protocol, launcher and archive. |
| x86_64-apple-darwin | macos-15-intel | Host, isolation rejection, audit, protocol, launcher and archive. |
| aarch64-apple-darwin | macos-15 | Host, isolation rejection, audit, protocol, launcher and archive. |
| x86_64-pc-windows-msvc | windows-2025 | Host, Job Objects, isolation rejection, audit, protocol, launcher and archive. |
| aarch64-pc-windows-msvc | windows-11-arm | Host, Job Objects, isolation rejection, audit, protocol, launcher and archive. |

The dependency-free npm launcher passed seven local tests: target/runtime selection, exact manifest entries, hash rejection before extraction, cache verification and tamper detection, concurrent installation, HTTP/download bounds, and argument/cwd/environment/stdio/exit-code forwarding. CI repeats them on every runner and extracts actual native archives through the launcher. A Windows test comparison now resolves both paths before comparing, covering short and long spellings of the same directory.

Package checks exclude private configurations, native test fixtures and build output. Every native archive includes the executable, licenses and root maintenance documents; both native and npm packages include the audit/client guides. The [published v0.1.2 release](https://github.com/getio0909/antigravity-worker-mcp/releases/tag/v0.1.2) contains six native archives, one npm tarball and `SHA256SUMS`. All seven package checksums matched after download. Archive inspection confirmed the declared binary architectures, README and license, with no absolute or parent-traversal entry paths.

The public GitHub-tarball npx command downloaded the Linux x86-64 release and returned version 0.1.2. A subsequent launch used the verified cache and completed stdio initialization, listed all five tools and called `ag_capabilities`. The response reported CLI 1.2.14, fourteen model catalog entries, default host execution, isolation disabled and audit logging enabled. The process exited normally with logs retained. npm registry publication remains separate.

## Model discussion and reasoning controls

Product-level reviews used fixed `gemini-3.8-flash-high` and `claude-opus-4-6-thinking` slugs through the installed official CLI. The brief contained no source files or implementation recipes. Source inspection and tool use were disabled. Current [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp), [Claude Code MCP](https://code.claude.com/docs/en/mcp) and [headless](https://code.claude.com/docs/en/headless) documentation informed later rounds and client checks.

Flash High accepted `--effort high`; `--effort max` conflicted with the model variant. Opus Thinking rejected the effort control, and the installed CLI exposed no maximum reasoning-budget setting for it. No alternate model was substituted.

Gemini completed its initial review, peer-assessment follow-up, documentation-informed review and live-trace follow-up. Opus completed its initial assessment; later peer/documentation/live-trace follow-ups ended with provider HTTP 503 capacity errors. Partial text from those failures is not a completed review. The completed Opus assessment reported zero separately classified thinking tokens; this does not establish an absence of internal reasoning or a configurable maximum.

The reviews supported focused background delegation and local auditability. Storage-failure behavior and minimal opt-out metadata drew different recommendations across rounds. The implemented contract is full local capture, no automatic deletion, a complete off switch with no retained marker, and cancellation after enabled log I/O fails. It does not claim unanimous approval, immutable evidence or strict host command enforcement.

## Actual Codex and Claude Code sessions

Installed versions were Codex 0.160.0 and Claude Code 2.1.285; both had existing authenticated accounts. Temporary headless sessions connected independently to the Rust release executable without changing global registrations. Both shared a private execution-lock directory and used CLI 1.2.14 with `gemini-3.8-flash-high`.

Both clients called capabilities, submitted a task, kept the connection alive while polling status and retrieved the final result. Capabilities showed default host execution, `isolation: false`, automatic host approval and full audit logging. Parent-client reads and an independent byte check verified each resulting file: six bytes with SHA-256 `ed1a545bb85e55816bbf9566b028b2a0bc456b88f49f6f266c0401048824194b`.

| Client | Runtime outcome | File check | Trace inspection |
| --- | --- | --- | --- |
| Codex | `completed`, CLI `SUCCESS`, exit 0; wrapper duration 19,301 ms. | Expected bytes confirmed. | Native file read/write tools; no command or historical-transcript read in the captured trace. |
| Claude Code | `completed`, CLI `SUCCESS`, exit 0; wrapper duration 104,509 ms. | Expected bytes confirmed. | One unrelated command and five reads of previous CLI transcripts outside the project. |

The second worker used additional tools despite the probe's request to avoid commands, and its valid report did not disclose those actions. Claude Code independently found them in the retained trace. The edit passed; the trace exposed additional working steps. Task wording was not an enforced tool policy. The final adapter passes goals, input context and report formatting without adding behavioral instructions, leaving tool selection to the autonomous CLI. Acceptance checks actual results and side effects rather than assuming that a formatted report describes the entire process.

The two audit directories retained eighteen files totaling 112,879 bytes after client shutdown. Unix directories were mode 0700 and every file was 0600. Both recorded connection closure, exact MCP streams, selected inputs, CLI streams and terminal metadata. Private configuration and full logs remain outside the public project and release packages.

### Autonomous coding follow-up

Both clients then submitted an objective to complete a small JavaScript module against existing project checks. The final adapter supplied task context and report formatting without behavioral restrictions. The agents chose their own reads, edits and shell commands. Both returned `completed`, CLI `SUCCESS` and exit 0.

| Client | Wrapper runtime duration | Observed command invocations | Independent acceptance |
| --- | --- | --- | --- |
| Codex | 48,018 ms. | 5, including npm checks and directory/Git inspection. | Correct module, original checks unchanged, npm check passed and five additional numeric cases passed. |
| Claude Code | 105,008 ms. | 17, including npm checks and inspection outside the project. | Correct module, original checks unchanged, npm check passed and five additional numeric cases passed. |

The resulting modules were identical, with SHA-256 `5b63136552577a64d788dc3cd4552739d0d60f9e1adb63ec4dfb6932d56fc75d`. The wrapper allowed the observed tool choices and retained them. No Python or environment-manager command appeared in the captured task trace. Two additional closed audit directories retained twenty-two files totaling 157,511 bytes, all mode 0600. Autonomous execution can include substantial exploratory work; these small tasks do not establish quota efficiency.

### CC Switch registration

A local CC Switch 3.20.4 build imported the verified native v0.1.2 executable through its MCP import dialog, with Codex and Claude Code enabled. Database flags and both client configuration files confirmed the registration. Existing MCP registrations, provider records, provider selections and the Codex authentication file were preserved. Claude Code received an allow rule for this server's tools while its other settings were preserved.

Fresh Codex and Claude Code sessions then loaded the global registrations without an explicit replacement MCP configuration. Both called `ag_capabilities` successfully and exited with code 0. The Codex session took 20,221 ms and the Claude Code session 19,790 ms; these are session durations, not inference latency. The native connection check and two sessions retained three closed audit directories containing fifteen files and 29,559 bytes, all files mode 0600 and directories mode 0700. Capability queries performed no model inference through the worker.

The registration check confirms client connectivity and configuration. The coding sessions above provide separate evidence of actual delegated work. Existing interactive sessions need to restart to load the added server.

## Additional official CLI checks

A live capability query returned fourteen catalog entries without inference. Earlier release-executable checks used a temporary official TypeScript MCP SDK client, not a runtime dependency:

| Check | Observed result |
| --- | --- |
| Optional isolated single-file edit. | Validated report; original bytes unchanged; returned patch contained the actual edit. |
| Default host single-file edit. | Validated report; target bytes changed as intended. |
| Cancellation after real CLI progress began. | Cancelled with `process_stopped: true`. |

The earlier edits used `gemini-3.8-flash-medium`. CLI-reported usage:

| Check | Input | Output | Thinking | Cache read | Reported total |
| --- | --- | --- | --- | --- | --- |
| Earlier workspace | 52,358 | 669 | 468 | 0 | 53,027 |
| Earlier host | 72,280 | 8,830 | 8,501 | 0 | 81,110 |
| Codex-submitted host | 55,799 | 2,348 | 2,008 | 0 | 58,147 |
| Claude-submitted host | 109,103 | 10,498 | 9,286 | 179,206 | 119,601 |
| Codex autonomous coding | 158,770 | 3,160 | 2,278 | 24,304 | 161,930 |
| Claude autonomous coding | 236,338 | 7,735 | 5,084 | 433,117 | 244,073 |

No subscription allowance, price or quota efficiency is inferred from these fields. Direct binary deployment needs no Node.js runtime; the optional npx launcher needs Node.js and npm.

## Unverified behavior and limits

- Real-provider authentication and complete client workflows on macOS/Windows remain unmeasured; native CI uses an account-free fixture.
- Complex coding quality, research accuracy and sustained reliability remain unmeasured.
- Opus peer/documentation follow-ups were not completed because of provider capacity errors. Maximum Opus reasoning intensity could not be selected.
- Live cancellation was not repeated inside both parent clients; fixture supervision and a separate real-CLI cancellation were checked.
- The declared Rust 1.89 minimum has not been compiled locally; the tested CI compiler is 1.97.1.
- No benchmark establishes submission P95, task success rate, quota efficiency or remaining allowance.
- Additional-platform isolation, shared results, durable restart recovery, immutable logging and strict command policies remain outside version 0.1.
- Host tasks can change original files, inspect unrelated materials or affect external services. Cancellation does not certify rollback or termination of detached Unix sessions or external services.

This is an experimental release with observed autonomous working steps disclosed. Engineering CI needs no Google credentials and consumes no provider quota. CLI behavior, model catalogs and client settings can change independently of this wrapper.
