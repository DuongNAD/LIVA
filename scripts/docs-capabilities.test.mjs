import test from 'node:test'
import assert from 'node:assert/strict'

import { assertRegistry, render } from './docs-capabilities.mjs'

function registry(overrides = {}) {
  return {
    schema_version: 1,
    updated: '2026-07-30',
    status_definitions: {},
    capabilities: [
      {
        id: 'voice.duplex',
        title: 'Voice duplex',
        domain: 'voice',
        status: 'partial',
        priority: 'P0',
        target_phase: 0,
        current_state: 'Có đường chạy thật.',
        evidence: ['liva-native-core/src/webrtc/pipeline.rs'],
        next_milestone: 'Đo SLO.'
      }
    ],
    ...overrides
  }
}

test('assertRegistry chấp nhận registry tối thiểu hợp lệ', () => {
  assert.doesNotThrow(() => assertRegistry(registry()))
})

test('assertRegistry từ chối ID trùng và status ngoài schema', () => {
  const duplicate = registry()
  duplicate.capabilities.push({ ...duplicate.capabilities[0] })
  assert.throws(() => assertRegistry(duplicate), /trùng capability id/)

  const invalidStatus = registry()
  invalidStatus.capabilities[0].status = 'done'
  assert.throws(() => assertRegistry(invalidStatus), /status không hợp lệ/)
})

test('render sinh frontmatter, summary và capability row tất định', () => {
  const output = render(registry(), 'abc1234')

  assert.match(output, /commit: abc1234/)
  assert.match(output, /\| \[MỘT PHẦN\] \| 1 \|/)
  assert.match(output, /`voice\.duplex`/)
  assert.match(output, /Không sửa tay/)
})
