import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream, readFileSync } from 'node:fs';
import { chmod, lstat, mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { constants, homedir } from 'node:os';
import { join } from 'node:path';
import { Readable, Transform } from 'node:stream';
import { pipeline } from 'node:stream/promises';

const metadata = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8'));
export const VERSION = metadata.version;
const NAME = 'antigravity-worker-mcp';
const RELEASE = `https://github.com/getio0909/${NAME}/releases/download/v${VERSION}`;
const HASH = /^[a-f0-9]{64}$/;

export function selectTarget(platform, arch, glibcVersion) {
  const targets = {
    linux: { x64: 'x86_64-unknown-linux-gnu', arm64: 'aarch64-unknown-linux-gnu' },
    darwin: { x64: 'x86_64-apple-darwin', arm64: 'aarch64-apple-darwin' },
    win32: { x64: 'x86_64-pc-windows-msvc', arm64: 'aarch64-pc-windows-msvc' },
  };
  const target = targets[platform]?.[arch];
  if (!target) throw new Error(`Unsupported platform: ${platform}/${arch}. Use a source build.`);
  if (platform === 'linux') {
    const [major, minor] = String(glibcVersion ?? '').split('.').map(Number);
    if (!(major > 2 || (major === 2 && minor >= 35))) {
      throw new Error('Prebuilt Linux executables require glibc 2.35 or newer. Use a source build for musl or older glibc.');
    }
  }
  return target;
}

export function checksumFor(manifest, filename) {
  const matches = manifest.split(/\r?\n/).flatMap((line) => {
    const entry = /^([a-f0-9]{64}) [ *](.+)$/.exec(line);
    return entry?.[2] === filename ? [entry[1]] : [];
  });
  if (matches.length !== 1) throw new Error(`Missing or duplicate release checksum for ${filename}.`);
  return matches[0];
}

export async function hashFile(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

function cacheRoot() {
  if (process.platform === 'win32') return join(process.env.LOCALAPPDATA || join(homedir(), 'AppData', 'Local'), NAME, 'Cache');
  if (process.platform === 'darwin') return join(homedir(), 'Library', 'Caches', NAME);
  return join(process.env.XDG_CACHE_HOME || join(homedir(), '.cache'), NAME);
}

async function checkDirectory(path) {
  const stat = await lstat(path);
  if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error(`Unsafe cache directory: ${path}`);
  if (process.platform !== 'win32' && (stat.uid !== process.getuid() || (stat.mode & 0o022) !== 0)) {
    throw new Error(`Cache directory must belong to the current user and deny other users write access: ${path}`);
  }
}

async function cachedBinary(directory, target, executable) {
  try {
    await checkDirectory(directory);
  } catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
  const receiptPath = join(directory, 'receipt.json');
  const binary = join(directory, executable);
  try {
    const receiptStat = await lstat(receiptPath);
    const binaryStat = await lstat(binary);
    if (!receiptStat.isFile() || receiptStat.isSymbolicLink() || receiptStat.size > 2048
      || !binaryStat.isFile() || binaryStat.isSymbolicLink() || binaryStat.nlink !== 1) throw new Error('Invalid cache files.');
    const receipt = JSON.parse(await readFile(receiptPath, 'utf8'));
    if (receipt.version !== VERSION || receipt.target !== target || !HASH.test(receipt.archiveSha256)
      || !HASH.test(receipt.binarySha256) || await hashFile(binary) !== receipt.binarySha256) throw new Error('Invalid cache checksum.');
    return binary;
  } catch {
    throw new Error(`Cached executable failed verification. Remove ${directory} and retry.`);
  }
}

export async function downloadFile(url, destination, maxBytes, signal) {
  const timeout = AbortSignal.timeout(120_000);
  const response = await fetch(url, { signal: signal ? AbortSignal.any([signal, timeout]) : timeout });
  if (!response.ok || !response.body) throw new Error(`Release download failed (HTTP ${response.status}). Check the GitHub release and network access.`);
  if (Number(response.headers.get('content-length')) > maxBytes) throw new Error('Release download exceeds the size limit.');
  let bytes = 0;
  const limit = new Transform({
    transform(chunk, encoding, callback) {
      bytes += chunk.length;
      callback(bytes > maxBytes ? new Error('Release download exceeds the size limit.') : null, chunk);
    },
  });
  await pipeline(Readable.fromWeb(response.body), limit, createWriteStream(destination, { flags: 'wx', mode: 0o600 }));
}

export async function extractArchive(archive, destination, platform, signal) {
  const timeout = AbortSignal.timeout(60_000);
  const options = {
    stdio: ['ignore', 'ignore', 'ignore'],
    windowsHide: true,
    signal: signal ? AbortSignal.any([signal, timeout]) : timeout,
  };
  let child;
  if (platform === 'win32') {
    child = spawn('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command',
      "$ErrorActionPreference = 'Stop'; Expand-Archive -LiteralPath $env:AG_MCP_EXTRACT_SOURCE -DestinationPath $env:AG_MCP_EXTRACT_DESTINATION"],
    { ...options, env: { ...process.env, AG_MCP_EXTRACT_SOURCE: archive, AG_MCP_EXTRACT_DESTINATION: destination } });
  } else {
    child = spawn('tar', ['-xzf', archive, '--no-same-owner', '--no-same-permissions', '-C', destination], options);
  }
  await new Promise((resolve, reject) => {
    child.once('error', () => reject(new Error('Cannot extract the release. Unix requires tar; Windows requires PowerShell.')));
    child.once('exit', (code) => code === 0 ? resolve() : reject(new Error('Release archive extraction failed.')));
  });
}

export async function ensureBinary({
  platform = process.platform,
  arch = process.arch,
  glibcVersion = platform === 'linux' ? process.report.getReport().header.glibcVersionRuntime : undefined,
  root = cacheRoot(),
  download = downloadFile,
  extract = extractArchive,
  diagnostic = (message) => process.stderr.write(`${NAME}: ${message}\n`),
  signal,
} = {}) {
  const target = selectTarget(platform, arch, glibcVersion);
  const executable = platform === 'win32' ? `${NAME}.exe` : NAME;
  const versionDirectory = join(root, VERSION);
  const directory = join(versionDirectory, target);
  await mkdir(root, { recursive: true, mode: 0o700 });
  await checkDirectory(root);
  await mkdir(versionDirectory, { mode: 0o700 }).catch((error) => { if (error.code !== 'EEXIST') throw error; });
  await checkDirectory(versionDirectory);
  const existing = await cachedBinary(directory, target, executable);
  if (existing) return existing;
  diagnostic(`Downloading v${VERSION} for ${target}; subsequent launches use the verified cache.`);
  const temporary = await mkdtemp(join(versionDirectory, `${target}.partial-`));
  try {
    const filename = `${NAME}-v${VERSION}-${target}.${platform === 'win32' ? 'zip' : 'tar.gz'}`;
    const manifestPath = join(temporary, 'SHA256SUMS');
    const archive = join(temporary, filename);
    await download(`${RELEASE}/SHA256SUMS`, manifestPath, 64 * 1024, signal);
    const expected = checksumFor(await readFile(manifestPath, 'utf8'), filename);
    await download(`${RELEASE}/${filename}`, archive, 64 * 1024 * 1024, signal);
    if (await hashFile(archive) !== expected) throw new Error('Release archive SHA-256 mismatch. The executable was not started.');
    const payload = join(temporary, 'payload');
    await mkdir(payload, { mode: 0o700 });
    await extract(archive, payload, platform, signal);
    if (platform !== 'win32') await chmod(payload, 0o700);
    await checkDirectory(payload);
    const binary = join(payload, executable);
    const stat = await lstat(binary);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) throw new Error('Release executable is missing or unsafe.');
    if (platform !== 'win32') await chmod(binary, 0o700);
    await writeFile(join(payload, 'receipt.json'), JSON.stringify({ version: VERSION, target, archiveSha256: expected, binarySha256: await hashFile(binary) }), { flag: 'wx', mode: 0o600 });
    try {
      await rename(payload, directory);
    } catch (error) {
      const concurrent = await cachedBinary(directory, target, executable);
      if (concurrent) return concurrent;
      throw error;
    }
    return join(directory, executable);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

export function spawnWorker(binary, args, options = {}) {
  return spawn(binary, args, { stdio: 'inherit', windowsHide: true, ...options });
}

export async function launch(args) {
  const controller = new AbortController();
  let child;
  let interrupted;
  const forward = (signal) => {
    interrupted = signal;
    if (child) child.kill(signal);
    else controller.abort();
  };
  const onInterrupt = () => forward('SIGINT');
  const onTerminate = () => forward('SIGTERM');
  process.on('SIGINT', onInterrupt);
  process.on('SIGTERM', onTerminate);
  try {
    const binary = await ensureBinary({ signal: controller.signal });
    if (controller.signal.aborted) return interrupted === 'SIGINT' ? 130 : 143;
    child = spawnWorker(binary, args[0] === '--' ? args.slice(1) : args);
    return await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', (code, signal) => resolve(code ?? 128 + (constants.signals[signal] || 1)));
    });
  } catch (error) {
    if (controller.signal.aborted) return interrupted === 'SIGINT' ? 130 : 143;
    process.stderr.write(`${NAME}: ${error.message}\n`);
    return 1;
  } finally {
    process.removeListener('SIGINT', onInterrupt);
    process.removeListener('SIGTERM', onTerminate);
  }
}
