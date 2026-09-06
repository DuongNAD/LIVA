#!/usr/bin/env node

import fs from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(import.meta.dirname, '..')
const REGISTRY_PATH = path.join(ROOT, 'docs', '_data', 'capabilities.json')
const OUTPUT_PATH = path.join(ROOT, 'docs', '_generated', 'ma-tran-nang-luc.md')
const CHECK_ONLY = process.argv.includes('--check')

const ALLOWED_STATUS = new Set(['working', 'partial', 'experimental', 'missing', 'blocked'])
const ALLOWED_PRIORITY = new Set(['P0', 'P1', 'P2', 'P3', 'P4'])
const STATUS_LABEL = {
  working: '[OK]',
  partial: '[MỘT PHẦN]',
  experimental: '[THỬ NGHIỆM]',
  missing: '[THIẾU]',
  blocked: '[BỊ CHẶN]'
}

function fail(message) {
  console.error(`docs-capabilities: ${message}`)
  process.exitCode = 1
}

export function assertRegistry(registry) {
  if (registry.schema_version !== 1) throw new Error('schema_version phải bằng 1')
  if (!Array.isArray(registry.capabilities) || registry.capabilities.length === 0) {
    throw new Error('capabilities phải là một mảng không rỗng')
  }

  const ids = new Set()
  for (const capability of registry.capabilities) {
    for (const key of [
      'id',
      'title',
      'domain',
      'status',
      'priority',
      'target_phase',
      'current_state',
      'evidence',
      'next_milestone'
    ]) {
      if (!(key in capability)) throw new Error(`${capability.id ?? '<unknown>'}: thiếu ${key}`)
    }
    if (ids.has(capability.id)) throw new Error(`trùng capability id: ${capability.id}`)
    ids.add(capability.id)
    if (!ALLOWED_STATUS.has(capability.status)) {
      throw new Error(`${capability.id}: status không hợp lệ: ${capability.status}`)
    }
    if (!ALLOWED_PRIORITY.has(capability.priority)) {
      throw new Error(`${capability.id}: priority không hợp lệ: ${capability.priority}`)
    }
    if (!Number.isInteger(capability.target_phase) || capability.target_phase < 0) {
      throw new Error(`${capability.id}: target_phase phải là số nguyên không âm`)
    }
    if (!Array.isArray(capability.evidence) || capability.evidence.length === 0) {
      throw new Error(`${capability.id}: evidence phải là một mảng không rỗng`)
    }
  }
}

function escapeCell(value) {
  return String(value).replaceAll('|', '\\|').replace(/\s+/g, ' ').trim()
}

// ⚠️ `commit` của file sinh PHẢI là chuỗi cố định `'auto'`, không được là
// `git rev-parse HEAD`: chế độ `--check` sinh lại rồi so với file đã lưu, mà
// file lưu ghi sha của HEAD *lúc sinh* thì lần so sau (HEAD mới) khác đúng một
// trường ⇒ đỏ vĩnh viễn sau mọi commit. Tiền lệ: `docs-check.mjs --map` ghi
// `commit: auto` cho `ban-do-code-tai-lieu.md` vì cùng lý do. `docs-check`
// bỏ qua stale-check cho các file này vì chúng có `status: index`.
export function render(registry, commit = 'auto') {
  const counts = Object.fromEntries([...ALLOWED_STATUS].map((status) => [status, 0]))
  for (const capability of registry.capabilities) counts[capability.status] += 1

  const rows = registry.capabilities.map((capability) => {
    const evidence = capability.evidence.map((item) => `\`${item}\``).join('<br>')
    return [
      `\`${capability.id}\``,
      capability.title,
      STATUS_LABEL[capability.status],
      capability.priority,
      `GĐ${capability.target_phase}`,
      capability.current_state,
      evidence,
      capability.next_milestone
    ].map(escapeCell).join(' | ')
  })

  return `---
title: "Ma trận năng lực LIVA → JARVIS"
updated: ${registry.updated}
commit: ${commit}
status: index
owns:
  - ma-tran-nang-luc-jarvis
covers:
  - docs/_data/capabilities.json
  - scripts/docs-capabilities.mjs
---
# Ma trận năng lực LIVA → JARVIS

[⬆ Mục lục](../README.md) · [Tầm nhìn](../00-san-pham/tam-nhin-jarvis.md) · [Master roadmap](../06-ke-hoach/roadmap.md)

> File này được sinh từ [\`docs/_data/capabilities.json\`](../_data/capabilities.json).
> Không sửa tay. Chạy \`npm run docs:capabilities\` để sinh lại hoặc
> \`npm run docs:capabilities:check\` để kiểm tra drift.

## Tóm tắt

| Trạng thái | Số năng lực |
|---|---:|
| [OK] | ${counts.working} |
| [MỘT PHẦN] | ${counts.partial} |
| [THỬ NGHIỆM] | ${counts.experimental} |
| [THIẾU] | ${counts.missing} |
| [BỊ CHẶN] | ${counts.blocked} |
| **Tổng** | **${registry.capabilities.length}** |

## Danh sách

| ID | Năng lực | Trạng thái | Ưu tiên | Đích | Hiện trạng | Bằng chứng | Mốc tiếp theo |
|---|---|---|---|---|---|---|---|
${rows.map((row) => `| ${row} |`).join('\n')}

## Quy ước cập nhật

1. Sửa trạng thái trong \`docs/_data/capabilities.json\`, không sửa bảng này.
2. Mọi trạng thái \`working\` phải có bằng chứng đường sản phẩm và acceptance test.
3. Khi capability đổi trạng thái, cập nhật cùng lúc master roadmap và tài liệu subsystem canonical.
4. \`experimental\` không được quảng cáo như hành vi sản phẩm mặc định.
5. \`blocked\` phải ghi dependency hoặc quyết định sản phẩm đang thiếu.
`
}

async function main() {
  const registry = JSON.parse(await fs.readFile(REGISTRY_PATH, 'utf8'))
  assertRegistry(registry)

  for (const capability of registry.capabilities) {
    for (const evidencePath of capability.evidence) {
      try {
        await fs.access(path.join(ROOT, evidencePath))
      } catch {
        throw new Error(`${capability.id}: evidence không tồn tại: ${evidencePath}`)
      }
    }
  }

  const output = render(registry)
  if (CHECK_ONLY) {
    let current = ''
    try {
      current = await fs.readFile(OUTPUT_PATH, 'utf8')
    } catch {
      fail(`thiếu file sinh: ${path.relative(ROOT, OUTPUT_PATH)}`)
      return
    }
    if (current.replace(/\r\n/g, '\n') !== output.replace(/\r\n/g, '\n')) {
      fail('ma trận năng lực đã drift; chạy npm run docs:capabilities')
      return
    }
    console.log(`docs-capabilities: OK — ${registry.capabilities.length} năng lực, không drift`)
    return
  }

  await fs.mkdir(path.dirname(OUTPUT_PATH), { recursive: true })
  await fs.writeFile(OUTPUT_PATH, output, 'utf8')
  console.log(`docs-capabilities: đã sinh ${path.relative(ROOT, OUTPUT_PATH)} (${registry.capabilities.length} năng lực)`)
}

const IS_MAIN = process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))

if (IS_MAIN) {
  main().catch((error) => {
    fail(error instanceof Error ? error.message : String(error))
  })
}
