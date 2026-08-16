/**
 * Test Group 3: Reachable Core Commands (ping, status, llm:health_check)
 *
 * Verifies the actual response shapes and field types of reachable commands
 * executed against the running native engine.
 */

import assert from 'node:assert/strict'
import { connectWebSocket } from './helpers.mjs'

export async function runReachableCommandsTests(reporter, port) {
  reporter.startSection(3, 'Reachable Commands Contract (ping, status, llm:health_check)')

  const conn = await connectWebSocket({ port })
  if (!conn.ok) {
    reporter.test('Khởi tạo kết nối WebSocket cho mục 3', () => {
      throw new Error(`Không kết nối được gateway: ${conn.reason}`)
    })
    reporter.endSection()
    return
  }
  const ws = conn.ws

  try {
    // 3.1 ping
    await (async () => {
      const res = await ws.sendEvent('ping', {})
      reporter.test('3.1 Lệnh ping trả về đúng kiểu dữ liệu { pong: true }', () => {
        assert.equal(res.event, 'ping_response')
        assert.equal(typeof res.payload, 'object')
        assert.equal(res.payload?.pong, true)
      })
    })()

    // 3.2 status
    await (async () => {
      const res = await ws.sendEvent('status', {})
      reporter.test('3.2 Lệnh status trả về đúng thông tin định danh lõi và phiên bản', () => {
        assert.equal(res.event, 'status_response')
        assert.equal(res.payload?.engine, 'LIVA Native Engine')
        assert.equal(res.payload?.status, 'healthy')
        assert.equal(typeof res.payload?.version, 'string')
        assert.ok(res.payload?.version.length > 0)
      })
    })()

    // 3.3 llm:health_check
    await (async () => {
      const res = await ws.sendEvent('llm:health_check', {})
      reporter.test('3.3 Lệnh llm:health_check trả về trạng thái cấu trúc mô hình và tham số', () => {
        assert.equal(res.event, 'llm:health_check_response')
        assert.equal(res.payload?.status, 'healthy')
        assert.equal(typeof res.payload?.model_loaded, 'boolean')
        assert.equal(typeof res.payload?.model_path, 'string')
        assert.equal(typeof res.payload?.n_ctx, 'number')
        assert.equal(typeof res.payload?.n_gpu_layers, 'number')
      })
    })()

  } finally {
    ws.close()
  }

  reporter.endSection()
}
