import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { ensureActionlint, runActionlint } from './actionlint.mjs';

test('CI cache actionlint theo đúng phiên bản binary', async () => {
  const workflow = await readFile(
    new URL('../.github/workflows/test.yml', import.meta.url),
    'utf8',
  );

  assert.match(workflow, /node_modules\/.cache\/liva-actionlint/);
  assert.match(workflow, /key:\s*\$\{\{ runner\.os \}\}[^\n]*actionlint-1\.7\.12/);
  assert.ok(
    workflow.indexOf('node_modules/.cache/liva-actionlint')
      < workflow.indexOf('Check AI DevKit Workspace Contract'),
    'cache phải được restore trước devkit:lint',
  );
});

test('lỗi tải actionlint được phân loại riêng với lỗi cú pháp workflow', async (t) => {
  const cacheRoot = await mkdtemp(path.join(tmpdir(), 'liva-actionlint-download-'));
  t.after(() => rm(cacheRoot, { recursive: true, force: true }));

  await assert.rejects(
    ensureActionlint({
      cacheRoot,
      fetchImpl: async () => ({ ok: false, status: 503 }),
    }),
    (error) => {
      assert.equal(error?.code, 'ACTIONLINT_DOWNLOAD_FAILED');
      assert.match(error.message, /không tải được binary actionlint/i);
      assert.doesNotMatch(error.message, /syntax|workflow/i);
      return true;
    },
  );
});

test('actionlint từ chối workflow có key thụt sai', async (t) => {
  const dir = await mkdtemp(path.join(tmpdir(), 'liva-actionlint-'));
  t.after(() => rm(dir, { recursive: true, force: true }));

  const workflow = path.join(dir, 'bad.yml');
  await writeFile(
    workflow,
    [
      'name: malformed',
      'on: push',
      'jobs:',
      '  test:',
      '    runs-on: ubuntu-latest',
      '    steps:',
      '      - name: broken indentation',
      '       run: echo broken',
      '',
    ].join('\n'),
    'utf8',
  );

  const result = await runActionlint([workflow]);
  assert.notEqual(result.status, 0, 'workflow sai phải làm actionlint thất bại');
  assert.match(`${result.stdout}\n${result.stderr}`, /could not parse as YAML|syntax-check/i);
});
