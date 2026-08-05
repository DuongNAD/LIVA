import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { spawnSync } from 'node:child_process'

const REPO = path.resolve(import.meta.dirname, '..')

function repoGia() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'liva-models-test-'))
  for (const dir of ['scripts', 'data', 'models']) {
    fs.mkdirSync(path.join(root, dir), { recursive: true })
  }
  fs.copyFileSync(path.join(REPO, 'scripts/models.mjs'), path.join(root, 'scripts/models.mjs'))
  fs.copyFileSync(
    path.join(REPO, 'data/models-manifest.json'),
    path.join(root, 'data/models-manifest.json'),
  )
  return root
}

test('doctor báo weights Parakeet đời cũ là file mồ côi', (t) => {
  const root = repoGia()
  t.after(() => fs.rmSync(root, { recursive: true, force: true }))
  fs.writeFileSync(path.join(root, 'models/parakeet_vi.onnx.data'), 'legacy')

  const result = spawnSync(process.execPath, ['scripts/models.mjs', 'doctor'], {
    cwd: root,
    encoding: 'utf8',
  })

  assert.match(result.stdout, /mồ côi/u)
  assert.match(result.stdout, /models[\\/]parakeet_vi\.onnx\.data/u)
})

test('fetch chấp nhận profile full dạng positional do npm chuyển tiếp', (t) => {
  const root = repoGia()
  t.after(() => fs.rmSync(root, { recursive: true, force: true }))

  const result = spawnSync(process.execPath, ['scripts/models.mjs', 'fetch', 'full', '--dry-run'], {
    cwd: root,
    encoding: 'utf8',
  })

  assert.match(result.stdout, /profile "full"/u)
})
