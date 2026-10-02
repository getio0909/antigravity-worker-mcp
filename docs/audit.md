# Audit logging

Windows desktop launch records its route and launcher diagnostics when audit capture is enabled. Temporary Task Scheduler registration contains only executable and pipe metadata; task text stays in the private startup pipe and enabled task audit. Disabling wrapper logs does not disable Windows operational event logs.

Full local audit logging is enabled by default. Logs survive result expiry, client disconnection and normal server shutdown. Rotation creates additional files without deleting older files. There is no automatic age or size retention limit.

## Configuration

```json
{
  "auditLogging": true,
  "auditDirectory": "/absolute/path/to/private/audit",
  "auditRotateBytes": 16777216
}
```

All three settings are optional. The default directory is `stateDirectory/audit`; each MCP connection and detached supervisor gets a unique `connection-UUID` subdirectory. The default rotation threshold is 16 MiB, with a supported range of 4 KiB to 256 MiB.

To disable wrapper logging completely, set `auditLogging: false` or launch with:

```bash
antigravity-worker-mcp --config /absolute/path/config.local.json --no-audit
```

The command-line switch overrides the configuration. Disabled logging creates no audit directory, streams, metadata records or disabled marker. It does not remove existing logs or change the official CLI's history, the MCP client's records or provider retention.

## Recorded material

| Stream | Content |
| --- | --- |
| `events.NNNNNN.jsonl` | Schema version, assigned sequence number, timestamp, connection and job transitions, dispatch arguments, working directory, PID, exit status, interruption, manifests and terminal results. |
| `mcp.stdin.NNNNNN.log` / `mcp.stdout.NNNNNN.log` | Exact incoming and outgoing MCP transport bytes, including tool parameters and responses. |
| `job-UUID.input-NNNN.NNNNNN.log` | Complete bytes of explicitly selected input files, with fingerprints in the event manifest. |
| `job-UUID.cli.stdin.NNNNNN.log` | Exact task bytes delivered to the official CLI. |
| `job-UUID.cli.stdout.NNNNNN.log` / `job-UUID.cli.stderr.NNNNNN.log` | Unfiltered CLI stream bytes, including progress, exposed tool details, terminal output and diagnostics. |
| Probe and patch streams | Version/model queries and supervised Git patch-generation streams. |

Raw stream segments concatenate in numeric suffix order to reproduce the captured bytes. Rotation can split a UTF-8 character or NDJSON line across segments; concatenate before decoding. JSON event records remain whole and can exceed the configured rotation threshold. Raw streams preserve bytes even when the in-memory parser rejects malformed output or reaches its delivery limit; capture ends when supervision stops the process.

`ag_capabilities` reports `audit_logging`, `audit_connection_directory`, `audit_rotation_bytes` and `audit_auto_delete: false`. A job uses its own supervisor audit directory; closing the transport does not stop its capture. Terminal `ag_result` responses include `audit_logging`, `audit_directory` and `audit_stream_prefix`, alongside CLI exit status and duration. File references are local paths, not remotely served download links.

## Scope and privacy

This is a complete record of the wrapper's observed streams, not a complete record of the host. Files read later by the CLI are captured only to the extent exposed in those streams. The logger does not copy the whole project, dump environment values, inspect authentication caches or obtain hidden model reasoning. A model or command can still expose sensitive content in its output; full logging preserves that output. Tool traces do not establish every subsequent filesystem or network effect.

Unix directories require current-user ownership and mode 0700; files use mode 0600. Symlinks are rejected. Windows rejects reparse points and relies on inherited profile ACLs; use a directory restricted to the intended account. Logs stay outside the repository and release packages. Keep them out of public issues and review their contents before sharing.

Host tasks run with the same current-user authority as the logger. They can alter or delete audit files. These logs are useful for inspection, but are not immutable evidence, a trusted remote ledger or a replay/recovery journal. Archive checksums protect release downloads; they do not make runtime logs tamper-proof.

## Storage failure and cleanup

When logging is enabled, an unsafe directory fails startup with `AUDIT_UNSAFE`. A write, rotation or checkpoint-sync failure triggers `AUDIT_FAILED`, stops dispatch and cancels the affected supervised execution. A broken MCP transport may prevent the client from receiving that error. Existing file changes are not rolled back, and bytes already emitted immediately before an I/O failure are not guaranteed durable.

Writes are synced at job lifecycle and connection checkpoints, and at rotation. Abrupt termination or power loss can still leave an incomplete tail. Restore available private storage before restarting, or explicitly start with logging disabled. The wrapper never silently disables logging after a storage error.

Supervisor startup and fatal diagnostics are retained in `job-<id>.supervisor.stderr.log` in the accepting adapter's audit directory. This small error stream uses the supervisor's inherited file handle; it is not a rotated CLI stream and may lose its final bytes on abrupt termination. The detached supervisor's own connection directory contains the captured task streams and terminal records.

Disk use grows with retained tasks and outputs. Stop the relevant connections before inspecting, archiving or deleting their audit directories. `retentionSeconds` and `ag_forget` control persisted result availability only; neither deletes audit files or deduplication metadata. With audit disabled, the operational job status and final report still persist so disconnected clients can retrieve their work; task inputs are not saved as replay requests.
