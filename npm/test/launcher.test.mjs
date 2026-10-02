import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { chmod, mkdtemp, readdir, realpath, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { VERSION, checksumFor, downloadFile, ensureBinary, selectTarget, spawnWorker } from '../lib/launcher.mjs';

const executable = 'antigravity-worker-mcp';
const target = 'x86_64-unknown-linux-gnu';
const archiveName = `${executable}-v${VERSION}-${target}.tar.gz`;
const archiveBytes = Buffer.from('synthetic release archive');
const archiveHash = createHash('sha256').update(archiveBytes).digest('hex');

async function temporary(t) {
  const directory = await mkdtemp(join(tmpdir(), 'ag-mcp-launcher-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

function fixture(root, overrides = {}) {
  return {
    root, platform: 'linux', arch: 'x64', glibcVersion: '2.35', diagnostic: () => {},
    download: async (url, destination) => writeFile(destination, url.endsWith('/SHA256SUMS') ? `${archiveHash}  ${archiveName}\n` : archiveBytes),
    extract: async (archive, destination) => writeFile(join(destination, executable), 'synthetic executable'),
    ...overrides,
  };
}

test('selects all six targets and rejects unsupported Linux runtimes', () => {
  const expected = [
    ['linux', 'x64', 'x86_64-unknown-linux-gnu'], ['linux', 'arm64', 'aarch64-unknown-linux-gnu'],
    ['darwin', 'x64', 'x86_64-apple-darwin'], ['darwin', 'arm64', 'aarch64-apple-darwin'],
    ['win32', 'x64', 'x86_64-pc-windows-msvc'], ['win32', 'arm64', 'aarch64-pc-windows-msvc'],
  ];
  for (const [platform, arch, result] of expected) assert.equal(selectTarget(platform, arch, '2.35'), result);
  assert.throws(() => selectTarget('linux', 'x64', undefined), /glibc/);
  assert.throws(() => selectTarget('linux', 'x64', '2.31'), /glibc/);
  assert.throws(() => selectTarget('linux', 'arm', '2.35'), /Unsupported/);
});

test('requires one exact checksum entry', () => {
  assert.equal(checksumFor(`${archiveHash}  ${archiveName}\r\n`, archiveName), archiveHash);
  assert.throws(() => checksumFor(`${archiveHash}  other.tar.gz\n`, archiveName), /Missing/);
  assert.throws(() => checksumFor(`${archiveHash}  ${archiveName}\n`.repeat(2), archiveName), /duplicate/);
});

test('a checksum mismatch prevents extraction and cleans temporary files', async (t) => {
  const root = await temporary(t);
  let extracted = false;
  await assert.rejects(ensureBinary(fixture(root, {
    download: async (url, destination) => writeFile(destination, url.endsWith('/SHA256SUMS') ? `${'0'.repeat(64)}  ${archiveName}\n` : archiveBytes),
    extract: async () => { extracted = true; },
  })), /SHA-256 mismatch/);
  assert.equal(extracted, false);
  assert.deepEqual(await readdir(join(root, VERSION)), []);
});

test('verified cache needs no network and detects later executable changes', async (t) => {
  const root = await temporary(t);
  const binary = await ensureBinary(fixture(root, {
    extract: async (archive, destination) => {
      await writeFile(join(destination, executable), 'synthetic executable');
      if (process.platform !== 'win32') await chmod(destination, 0o775);
    },
  }));
  const offline = fixture(root, { download: () => { throw new Error('Unexpected network call.'); } });
  assert.equal(await ensureBinary(offline), binary);
  await writeFile(binary, 'changed bytes');
  await assert.rejects(ensureBinary(offline), /failed verification/);
});

test('concurrent first launches share one complete cache entry', async (t) => {
  const root = await temporary(t);
  let arrivals = 0;
  let release;
  const barrier = new Promise((resolve) => { release = resolve; });
  const options = fixture(root, {
    extract: async (archive, destination) => {
      await writeFile(join(destination, executable), 'synthetic executable');
      if (++arrivals === 2) release();
      await barrier;
    },
  });
  const binaries = await Promise.all([ensureBinary(options), ensureBinary(options)]);
  assert.equal(binaries[0], binaries[1]);
  assert.deepEqual(await readdir(join(root, VERSION)), [target]);
});

test('HTTP failures and oversized downloads fail before installation', async (t) => {
  const root = await temporary(t);
  const server = createServer((request, response) => {
    if (request.url === '/missing') response.writeHead(404).end();
    else response.writeHead(200).end('more than four bytes');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve) => { server.close(resolve); server.closeAllConnections(); }));
  const url = `http://127.0.0.1:${server.address().port}`;
  await assert.rejects(downloadFile(`${url}/missing`, join(root, 'missing'), 100), /HTTP 404/);
  await assert.rejects(downloadFile(`${url}/large`, join(root, 'large'), 4), /size limit/);
});

test('forwards arguments, cwd, environment, protocol bytes and exit status', async (t) => {
  const root = await temporary(t);
  const script = join(root, 'worker.mjs');
  await writeFile(script, `
import { createInterface } from 'node:readline';
process.stderr.write(JSON.stringify({ args: process.argv.slice(2), cwd: process.cwd(), marker: process.env.AG_MCP_TEST_MARKER }));
for await (const line of createInterface({ input: process.stdin })) process.stdout.write(line + '\\n');
process.exitCode = 7;
`);
  const child = spawnWorker(process.execPath, [script, '--config', 'a path with spaces.json', '--literal=$value'], {
    stdio: 'pipe', cwd: root, env: { ...process.env, AG_MCP_TEST_MARKER: 'inherited' },
  });
  let output = '';
  let errors = '';
  child.stdout.setEncoding('utf8').on('data', (chunk) => { output += chunk; });
  child.stderr.setEncoding('utf8').on('data', (chunk) => { errors += chunk; });
  const completed = new Promise((resolve, reject) => { child.on('error', reject); child.on('close', resolve); });
  const wire = '{"jsonrpc":"2.0","id":1,"method":"initialize"}\n';
  child.stdin.end(wire);
  assert.equal(await completed, 7);
  assert.equal(output, wire);
  const inherited = JSON.parse(errors);
  assert.deepEqual(inherited.args, ['--config', 'a path with spaces.json', '--literal=$value']);
  assert.equal(await realpath(inherited.cwd), await realpath(root));
  assert.equal(inherited.marker, 'inherited');
});
