import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, chmod, copyFile, mkdir, rename, rm, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

export const ACTIONLINT_VERSION = '1.7.12';
const RELEASE_BASE = `https://github.com/rhysd/actionlint/releases/download/v${ACTIONLINT_VERSION}`;

// SHA-256 từ actionlint_1.7.12_checksums.txt trong GitHub Release chính thức.
const RELEASES = {
  'darwin-arm64': ['darwin_arm64.tar.gz', 'aba9ced2dee8d27fecca3dc7feb1a7f9a52caefa1eb46f3271ea66b6e0e6953f'],
  'darwin-x64': ['darwin_amd64.tar.gz', '5b44c3bc2255115c9b69e30efc0fecdf498fdb63c5d58e17084fd5f16324c644'],
  'linux-arm64': ['linux_arm64.tar.gz', '325e971b6ba9bfa504672e29be93c24981eeb1c07576d730e9f7c8805afff0c6'],
  'linux-x64': ['linux_amd64.tar.gz', '8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8'],
  'win32-arm64': ['windows_arm64.zip', 'cadcf7ea4efe3a68728893813643cebe1185e5b1d4be5b96245f65c9a4d5ea41'],
  'win32-x64': ['windows_amd64.zip', '6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9'],
};

async function exists(file) {
  try {
    await access(file);
    return true;
  } catch {
    return false;
  }
}

function runOrThrow(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(
      `Không giải nén được actionlint (${command}): ${result.stderr || result.stdout || result.error}`,
    );
  }
}

async function extractArchive(archive, destination) {
  await mkdir(destination, { recursive: true });
  if (process.platform === 'win32') {
    runOrThrow('powershell', [
      '-NoProfile',
      '-NonInteractive',
      '-Command',
      '& { param($Archive, $Destination) Expand-Archive -LiteralPath $Archive -DestinationPath $Destination -Force }',
      archive,
      destination,
    ]);
    return;
  }
  runOrThrow('tar', ['-xzf', archive, '-C', destination]);
}

function downloadError(detail, cause) {
  const error = new Error(`Không tải được binary actionlint ${ACTIONLINT_VERSION}: ${detail}`, {
    cause,
  });
  error.code = 'ACTIONLINT_DOWNLOAD_FAILED';
  return error;
}

export async function ensureActionlint({
  cacheRoot = path.resolve('node_modules', '.cache', 'liva-actionlint'),
  fetchImpl = fetch,
} = {}) {
  const key = `${process.platform}-${process.arch}`;
  const release = RELEASES[key];
  if (!release) {
    throw new Error(`actionlint chưa hỗ trợ nền tảng ${key}`);
  }

  const [suffix, expectedSha256] = release;
  const cacheDir = path.join(cacheRoot, ACTIONLINT_VERSION, key);
  const executable = path.join(cacheDir, process.platform === 'win32' ? 'actionlint.exe' : 'actionlint');
  if (await exists(executable)) return executable;

  await mkdir(cacheDir, { recursive: true });
  const assetName = `actionlint_${ACTIONLINT_VERSION}_${suffix}`;
  let response;
  try {
    response = await fetchImpl(`${RELEASE_BASE}/${assetName}`);
  } catch (error) {
    throw downloadError(error instanceof Error ? error.message : String(error), error);
  }
  if (!response.ok) {
    throw downloadError(`GitHub Releases trả HTTP ${response.status}`);
  }
  const archiveBytes = Buffer.from(await response.arrayBuffer());
  const actualSha256 = createHash('sha256').update(archiveBytes).digest('hex');
  if (actualSha256 !== expectedSha256) {
    throw new Error(`Checksum actionlint sai: cần ${expectedSha256}, nhận ${actualSha256}`);
  }

  const archive = path.join(cacheDir, assetName);
  const extractDir = path.join(cacheDir, `extract-${process.pid}`);
  const temporaryExecutable = `${executable}.tmp-${process.pid}`;
  await writeFile(archive, archiveBytes);
  try {
    await extractArchive(archive, extractDir);
    const extracted = path.join(
      extractDir,
      process.platform === 'win32' ? 'actionlint.exe' : 'actionlint',
    );
    await copyFile(extracted, temporaryExecutable);
    if (process.platform !== 'win32') await chmod(temporaryExecutable, 0o755);
    await rename(temporaryExecutable, executable);
  } finally {
    await rm(extractDir, { recursive: true, force: true });
    await rm(temporaryExecutable, { force: true });
  }
  return executable;
}

export async function runActionlint(args = []) {
  const executable = await ensureActionlint();
  const result = spawnSync(executable, args, { encoding: 'utf8' });
  return {
    status: result.status ?? 1,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? String(result.error ?? ''),
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const result = await runActionlint(process.argv.slice(2));
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  process.exitCode = result.status;
}
