import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { VERSION, checksumFor, extractArchive, hashFile } from '../../npm/lib/launcher.mjs';

const target = process.argv[2];
assert.match(target, /^(x86_64|aarch64)-(unknown-linux-gnu|apple-darwin|pc-windows-msvc)$/);
const windows = target.endsWith('windows-msvc');
const filename = `antigravity-worker-mcp-v${VERSION}-${target}.${windows ? 'zip' : 'tar.gz'}`;
const archive = join(process.cwd(), 'artifacts', filename);
assert.equal(await hashFile(archive), checksumFor(await readFile(`${archive}.sha256`, 'utf8'), filename));
const cargo = await readFile('Cargo.toml', 'utf8');
assert.equal(/^version = "([^"]+)"$/m.exec(cargo)[1], VERSION);
const directory = await mkdtemp(join(tmpdir(), 'ag-mcp-package-'));
try {
  await extractArchive(archive, directory, windows ? 'win32' : process.platform);
  const binary = join(directory, `antigravity-worker-mcp${windows ? '.exe' : ''}`);
  assert.equal(execFileSync(binary, ['--version'], { encoding: 'utf8' }).trim(), VERSION);
  for (const file of ['README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'CONTRIBUTING.md', 'SECURITY.md', 'CHANGELOG.md', 'docs/RPD.md', 'docs/protocol.md', 'docs/architecture.md', 'docs/verification.md', 'examples/config.example.json', 'examples/config.windows.example.json']) {
    assert.ok((await readFile(join(directory, file))).length > 0, `Missing archive document: ${file}`);
  }
  console.log(`Verified archive, executable version and documentation for ${target}.`);
} finally {
  await rm(directory, { recursive: true, force: true });
}
