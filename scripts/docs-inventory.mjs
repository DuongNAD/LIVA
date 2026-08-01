#!/usr/bin/env node

import fs from 'node:fs/promises'
import path from 'node:path'
import { execFile } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
const ROOT = path.resolve(import.meta.dirname, '..')
const DOCS_ROOT = path.join(ROOT, 'docs')
const REGISTRY_PATH = path.join(DOCS_ROOT, '_data', 'document-inventory.json')
const OUTPUT_PATH = path.join(DOCS_ROOT, '_generated', 'kiem-ke-tai-lieu.md')
const CHECK_ONLY = process.argv.includes('--check')

const ALLOWED_DISPOSITIONS = new Set(['KEEP', 'SPLIT', 'GENERATE', 'FREEZE', 'MERGE'])
const DISPOSITION_ORDER = ['KEEP', 'SPLIT', 'GENERATE', 'FREEZE', 'MERGE']

function fail(message) {
  console.error(`docs-inventory: ${message}`)
  process.exitCode = 1
}

function toPosix(value) {
  return value.split(path.sep).join('/')
}

async function walkMarkdown(directory) {
  const output = []
  const entries = await fs.readdir(directory, { withFileTypes: true })
  for (const entry of entries) {
    const absolute = path.join(directory, entry.name)
    if (entry.isDirectory()) {
      output.push(...await walkMarkdown(absolute))
    } else if (entry.isFile() && entry.name.toLowerCase().endsWith('.md')) {
      output.push(toPosix(path.relative(ROOT, absolute)))
    }
  }
  return output.sort()
}

export function assertInventory(registry, discoveredPaths) {
  if (registry.schema_version !== 1) throw new Error('schema_version phải bằng 1')
  if (!Array.isArray(registry.documents) || registry.documents.length === 0) {
    throw new Error('documents phải là một mảng không rỗng')
  }

  const registered = new Set()
  for (const document of registry.documents) {
    for (const key of ['path', 'disposition', 'wave', 'targets', 'rationale']) {
      if (!(key in document)) throw new Error(`${document.path ?? '<unknown>'}: thiếu ${key}`)
    }
    if (registered.has(document.path)) throw new Error(`trùng document path: ${document.path}`)
    registered.add(document.path)
    if (!document.path.startsWith('docs/') || !document.path.endsWith('.md')) {
      throw new Error(`${document.path}: path phải là Markdown bên trong docs/`)
    }
    if (!ALLOWED_DISPOSITIONS.has(document.disposition)) {
      throw new Error(`${document.path}: disposition không hợp lệ: ${document.disposition}`)
    }
    if (!Array.isArray(document.targets)) {
      throw new Error(`${document.path}: targets phải là một mảng`)
    }
    if (['SPLIT', 'MERGE'].includes(document.disposition) && document.targets.length === 0) {
      throw new Error(`${document.path}: ${document.disposition} phải có target`)
    }
    if (typeof document.rationale !== 'string' || document.rationale.trim().length < 10) {
      throw new Error(`${document.path}: rationale quá ngắn`)
    }
  }

  const discovered = new Set(discoveredPaths)
  const missing = [...discovered].filter((item) => !registered.has(item)).sort()
  const stale = [...registered].filter((item) => !discovered.has(item)).sort()
  if (missing.length > 0) throw new Error(`chưa phân loại: ${missing.join(', ')}`)
  if (stale.length > 0) throw new Error(`registry trỏ tới file không tồn tại: ${stale.join(', ')}`)
}

function normalizeLink(sourcePath, rawTarget) {
  const clean = rawTarget.trim().replace(/^<|>$/g, '')
  if (
    clean === '' ||
    clean.startsWith('#') ||
    /^[a-z][a-z0-9+.-]*:/i.test(clean) ||
    clean.startsWith('//')
  ) {
    return null
  }

  const withoutQuery = clean.split('#', 1)[0].split('?', 1)[0]
  if (!withoutQuery.toLowerCase().endsWith('.md')) return null
  const base = clean.startsWith('/')
    ? path.join(ROOT, clean.slice(1))
    : path.resolve(ROOT, path.dirname(sourcePath), withoutQuery)
  return toPosix(path.relative(ROOT, base))
}

export function collectInboundLinks(documents) {
  const inbound = new Map(documents.map(({ path: documentPath }) => [documentPath, new Set()]))
  const markdownLink = /\[[^\]]*]\(([^)\s]+)(?:\s+["'][^"']*["'])?\)/g

  for (const document of documents) {
    for (const match of document.content.matchAll(markdownLink)) {
      const target = normalizeLink(document.path, match[1])
      if (target && inbound.has(target) && target !== document.path) {
        inbound.get(target).add(document.path)
      }
    }
  }
  return inbound
}

async function shortHead() {
  const { stdout } = await execFileAsync('git', ['rev-parse', '--short', 'HEAD'], {
    cwd: ROOT,
    encoding: 'utf8'
  })
  return stdout.trim()
}

function escapeCell(value) {
  return String(value).replaceAll('|', '\\|').replace(/\s+/g, ' ').trim()
}

export function render(registry, inbound, commit) {
  const counts = Object.fromEntries(DISPOSITION_ORDER.map((item) => [item, 0]))
  for (const document of registry.documents) counts[document.disposition] += 1

  const rows = [...registry.documents]
    .sort((left, right) => left.path.localeCompare(right.path))
    .map((document) => {
      const sources = [...(inbound.get(document.path) ?? [])].sort()
      return [
        `\`${document.path}\``,
        `**${document.disposition}**`,
        document.wave,
        sources.length,
        document.targets.length > 0
          ? document.targets.map((target) => `\`${target}\``).join('<br>')
          : '—',
        document.rationale
      ].map(escapeCell).join(' | ')
    })

  const migrationsWithLinks = registry.documents
    .filter((document) => ['SPLIT', 'MERGE'].includes(document.disposition))
    .map((document) => ({
      path: document.path,
      count: (inbound.get(document.path) ?? new Set()).size
    }))
    .filter((document) => document.count > 0)
    .sort((left, right) => right.count - left.count || left.path.localeCompare(right.path))

  return `---
title: "Kiểm kê và disposition tài liệu LIVA"
updated: ${registry.updated}
commit: ${commit}
status: index
owns:
  - inventory-disposition-tai-lieu
covers:
  - docs/_data/document-inventory.json
  - scripts/docs-inventory.mjs
---
# Kiểm kê và disposition tài liệu LIVA

[⬆ Mục lục](../README.md) · [Quy hoạch tài liệu](../07-dong-gop/quy-hoach-tai-lieu.md) · [Master roadmap](../06-ke-hoach/roadmap.md)

> File này được sinh từ [\`docs/_data/document-inventory.json\`](../_data/document-inventory.json)
> và link Markdown trong \`docs/\`. Không sửa tay.

## Tóm tắt

| Disposition | Số tài liệu |
|---|---:|
${DISPOSITION_ORDER.map((item) => `| ${item} | ${counts[item]} |`).join('\n')}
| **Tổng** | **${registry.documents.length}** |

## Quy ước

| Nhãn | Quyết định |
|---|---|
| KEEP | Giữ vai trò hiện tại trong giai đoạn chuyển tiếp |
| SPLIT | Tách theo subsystem/contract trước khi để lại redirect |
| GENERATE | Dữ liệu phải sinh tự động, không duy trì bảng bằng tay |
| FREEZE | Đóng băng như bằng chứng lịch sử |
| MERGE | Nhập phần còn giá trị vào canonical owner rồi để lại redirect |

## Danh sách đầy đủ

| File | Disposition | Đợt | Link vào | Đích | Lý do |
|---|---|---|---:|---|---|
${rows.map((row) => `| ${row} |`).join('\n')}

## File chưa được phép di chuyển ngay

Các file sau có disposition \`SPLIT/MERGE\` và đang có link Markdown trỏ vào. Phải cập nhật
inbound link trước, sau đó giữ redirect ít nhất một chu kỳ release.

${migrationsWithLinks.length > 0
    ? migrationsWithLinks.map((item) => `- \`${item.path}\`: ${item.count} file trỏ vào.`).join('\n')
    : '- Không có.'}

## Gate

- Mọi \`docs/**/*.md\` phải có đúng một disposition.
- \`SPLIT\` và \`MERGE\` phải có ít nhất một target.
- Registry không được trỏ tới file nguồn đã biến mất.
- Chạy \`npm run docs:inventory:check\` để phát hiện thiếu file hoặc generated drift.
`
}

async function loadDocuments(paths) {
  return Promise.all(paths.map(async (documentPath) => ({
    path: documentPath,
    content: await fs.readFile(path.join(ROOT, documentPath), 'utf8')
  })))
}

async function main() {
  const registry = JSON.parse(await fs.readFile(REGISTRY_PATH, 'utf8'))
  const discoveredPaths = await walkMarkdown(DOCS_ROOT)
  const outputRelative = toPosix(path.relative(ROOT, OUTPUT_PATH))
  const validationPaths = discoveredPaths.includes(outputRelative)
    ? discoveredPaths
    : [...discoveredPaths, outputRelative].sort()
  assertInventory(registry, validationPaths)
  const inbound = collectInboundLinks(await loadDocuments(discoveredPaths))
  const output = render(registry, inbound, await shortHead())

  if (CHECK_ONLY) {
    let current = ''
    try {
      current = await fs.readFile(OUTPUT_PATH, 'utf8')
    } catch {
      fail(`thiếu file sinh: ${path.relative(ROOT, OUTPUT_PATH)}`)
      return
    }
    if (current.replace(/\r\n/g, '\n') !== output.replace(/\r\n/g, '\n')) {
      fail('báo cáo inventory đã drift; chạy npm run docs:inventory')
      return
    }
    console.log(`docs-inventory: OK — ${registry.documents.length} tài liệu, không thiếu disposition`)
    return
  }

  await fs.mkdir(path.dirname(OUTPUT_PATH), { recursive: true })
  await fs.writeFile(OUTPUT_PATH, output, 'utf8')
  console.log(`docs-inventory: đã sinh ${path.relative(ROOT, OUTPUT_PATH)} (${registry.documents.length} tài liệu)`)
}

const IS_MAIN = process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))

if (IS_MAIN) {
  main().catch((error) => {
    fail(error instanceof Error ? error.message : String(error))
  })
}
