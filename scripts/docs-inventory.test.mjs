import test from 'node:test'
import assert from 'node:assert/strict'

import { assertInventory, collectInboundLinks, render } from './docs-inventory.mjs'

function inventory(overrides = {}) {
  return {
    schema_version: 1,
    updated: '2026-07-30',
    documents: [
      {
        path: 'docs/README.md',
        disposition: 'KEEP',
        wave: 'A',
        targets: [],
        rationale: 'Điểm vào chính của tài liệu.'
      },
      {
        path: 'docs/old.md',
        disposition: 'MERGE',
        wave: 'B',
        targets: ['docs/README.md'],
        rationale: 'Nhập nội dung còn giá trị vào mục lục.'
      }
    ],
    ...overrides
  }
}

test('assertInventory chấp nhận inventory bao phủ toàn bộ Markdown', () => {
  assert.doesNotThrow(() => assertInventory(inventory(), ['docs/README.md', 'docs/old.md']))
})

test('assertInventory từ chối file chưa phân loại và MERGE không có target', () => {
  assert.throws(
    () => assertInventory(inventory(), ['docs/README.md', 'docs/old.md', 'docs/new.md']),
    /chưa phân loại/
  )

  const invalid = inventory()
  invalid.documents[1].targets = []
  assert.throws(
    () => assertInventory(invalid, ['docs/README.md', 'docs/old.md']),
    /MERGE phải có target/
  )
})

test('collectInboundLinks chuẩn hóa link tương đối và loại self-link', () => {
  const inbound = collectInboundLinks([
    {
      path: 'docs/README.md',
      content: '[Old](old.md) [Self](README.md) [Web](https://example.com/a.md)'
    },
    {
      path: 'docs/old.md',
      content: '# Old'
    }
  ])

  assert.deepEqual([...inbound.get('docs/old.md')], ['docs/README.md'])
  assert.equal(inbound.get('docs/README.md').size, 0)
})

test('render sinh summary, inbound count và cảnh báo không sửa tay', () => {
  const inbound = new Map([
    ['docs/README.md', new Set()],
    ['docs/old.md', new Set(['docs/README.md'])]
  ])
  const output = render(inventory(), inbound, 'abc1234')

  assert.match(output, /commit: abc1234/)
  assert.match(output, /\| KEEP \| 1 \|/)
  assert.match(output, /`docs\/old\.md`.*\*\*MERGE\*\*.*\| 1 \|/)
  assert.match(output, /Không sửa tay/)
})
