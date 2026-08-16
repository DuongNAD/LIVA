/**
 * Test Group 2: Authorization Boundary & Origin Validation
 *
 * Verifies over real WebSocket connections:
 * - Origin header validation (allow-list enforcement)
 * - Self-proclaimed query parameter principal injection rejection
 * - Widget/Dashboard commands blocked for remote principal
 * - Anti-hang fast rejection on unauthorized commands
 */

import assert from 'node:assert/strict'
import { connectWebSocket, DEFAULT_ORIGIN_ALLOWED, DEFAULT_ORIGIN_DISALLOWED } from './helpers.mjs'

export async function runAuthorizationOriginTests(reporter, port) {
  reporter.startSection(2, 'Authorization Boundary & Origin Checking')

  // 2.1 Disallowed Origin
  await (async () => {
    const conn = await connectWebSocket({ port, origin: DEFAULT_ORIGIN_DISALLOWED })
    reporter.test('2.1 Origin không nằm trong allowlist bị từ chối với HTTP 403', () => {
      assert.equal(conn.ok, false)
      assert.equal(conn.statusCode, 403)
    })
    if (conn.ok) conn.ws.close()
  })()

  // 2.2 Spoofed domain Origin
  await (async () => {
    const conn = await connectWebSocket({ port, origin: 'http://localhost:5173.evil.example' })
    reporter.test('2.2 Origin giả mạo tiền tố (spoofed prefix) bị từ chối với HTTP 403', () => {
      assert.equal(conn.ok, false)
      assert.equal(conn.statusCode, 403)
    })
    if (conn.ok) conn.ws.close()
  })()

  // 2.3 Allowed Origin
  let ws = null
  await (async () => {
    const conn = await connectWebSocket({ port, origin: DEFAULT_ORIGIN_ALLOWED })
    reporter.test('2.3 Origin hợp lệ (localhost:5173) kết nối thành công (HTTP 101)', () => {
      assert.equal(conn.ok, true)
      assert.equal(conn.statusCode, 101)
      assert.ok(conn.ws !== null)
    })
    if (conn.ok) ws = conn.ws
  })()

  if (!ws) {
    reporter.test('Không thể kiểm tra phân quyền do kết nối gốc thất bại', () => {
      throw new Error('Kết nối allowlist thất bại')
    })
    reporter.endSection()
    return
  }

  try {
    // 2.4 Query param principal spoofing: principal=dashboard
    await (async () => {
      const conn = await connectWebSocket({ port, path: '/ws?principal=dashboard' })
      reporter.test('2.4 Tham số query ?principal=dashboard bị từ chối handshake (HTTP 403)', () => {
        assert.equal(conn.ok, false)
        assert.equal(conn.statusCode, 403)
      })
      if (conn.ok) conn.ws.close()
    })()

    // 2.5 Query param principal spoofing: principal=widget
    await (async () => {
      const conn = await connectWebSocket({ port, path: '/ws?principal=widget' })
      reporter.test('2.5 Tham số query ?principal=widget bị từ chối handshake (HTTP 403)', () => {
        assert.equal(conn.ok, false)
        assert.equal(conn.statusCode, 403)
      })
      if (conn.ok) conn.ws.close()
    })()

    // 2.6 Invalid session ticket
    await (async () => {
      const conn = await connectWebSocket({ port, path: '/ws?session=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef' })
      reporter.test('2.6 Session ticket giả / không hợp lệ bị từ chối handshake (HTTP 403)', () => {
        assert.equal(conn.ok, false)
        assert.equal(conn.statusCode, 403)
      })
      if (conn.ok) conn.ws.close()
    })()

    // 2.7 Widget command restriction: mcp:list_tools
    await (async () => {
      const res = await ws.sendEvent('mcp:list_tools', {})
      reporter.test('2.7 WebSocketRemote bị chặn khỏi lệnh widget/MCP (mcp:list_tools)', () => {
        assert.equal(res.event, 'mcp:list_tools_error')
        assert.ok(String(res.payload?.error).includes('not authorized'))
      })
    })()

    // 2.8 Dashboard command restriction: get_preflight_status
    await (async () => {
      const res = await ws.sendEvent('get_preflight_status', {})
      reporter.test('2.8 WebSocketRemote bị chặn khỏi lệnh dashboard (get_preflight_status)', () => {
        assert.equal(res.event, 'get_preflight_status_error')
        assert.ok(String(res.payload?.error).includes('not authorized'))
      })
    })()

    // 2.9 Vision command restriction: vision:ask + fast rejection latency
    await (async () => {
      const t0 = performance.now()
      const res = await ws.sendEvent('vision:ask', { question: 'Màn hình có gì?' }, 10000)
      const dt = performance.now() - t0
      reporter.test('2.9 WebSocketRemote bị chặn khỏi vision:ask và phản hồi từ chối ngay lập tức (< 5s)', () => {
        assert.equal(res.event, 'vision:ask_error')
        assert.ok(String(res.payload?.error).includes('not authorized'))
        assert.ok(dt < 5000, `Thời gian phản hồi phải < 5000ms (thực tế: ${dt.toFixed(1)}ms)`)
      })
    })()

  } finally {
    ws.close()
  }

  reporter.endSection()
}
