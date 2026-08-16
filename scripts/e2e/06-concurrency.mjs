/**
 * Test Group 6: Concurrency & Connection Isolation
 *
 * Verifies:
 * - Simultaneous WebSocket client handshakes
 * - Non-interfering parallel command dispatch across distinct connections
 * - Clean connection teardown without corrupting other active sessions
 */

import assert from 'node:assert/strict'
import { connectWebSocket, sleep } from './helpers.mjs'

export async function runConcurrencyTests(reporter, port) {
  reporter.startSection(6, 'Concurrency & Connection Isolation')

  const CLIENT_COUNT = 5
  const connections = []

  try {
    // 6.1 Parallel WebSocket handshakes
    await (async () => {
      const connPromises = Array.from({ length: CLIENT_COUNT }, (_, i) =>
        connectWebSocket({ port, timeout: 10000 })
      )
      const results = await Promise.all(connPromises)

      reporter.test(`6.1 Thiết lập đồng thời ${CLIENT_COUNT} kết nối WebSocket độc lập thành công`, () => {
        results.forEach((c, idx) => {
          assert.equal(c.ok, true, `Client #${idx + 1} phải kết nối thành công`)
          connections.push(c.ws)
        })
      })
    })()

    if (connections.length < CLIENT_COUNT) {
      reporter.test('Không đủ số lượng client để tiếp tục kiểm tra đồng thời', () => {
        throw new Error('Thiếu client kết nối')
      })
      reporter.endSection()
      return
    }

    // 6.2 Concurrent interleaved commands
    await (async () => {
      const [c1, c2, c3, c4, c5] = connections

      const p1 = c1.sendEvent('ping', {})
      const p2 = c2.sendEvent('status', {})
      const p3 = c3.sendEvent('llm:health_check', {})
      const p4 = c4.sendEvent('voice:stt_start', {})
      const p5 = c5.sendIpc('ping', {}, 'ipc-concurrency-client-5')

      const [r1, r2, r3, r4, r5] = await Promise.all([p1, p2, p3, p4, p5])

      reporter.test('6.2 Giao tiếp đồng thời 5 lệnh khác nhau không bị lẫn lộn dữ liệu giữa các socket', () => {
        assert.equal(r1.event, 'ping_response')
        assert.equal(r1.payload?.pong, true)

        assert.equal(r2.event, 'status_response')
        assert.equal(r2.payload?.engine, 'LIVA Native Engine')

        assert.equal(r3.event, 'llm:health_check_response')
        assert.equal(r3.payload?.status, 'healthy')

        assert.equal(r4.event, 'voice:stt_start_response')
        assert.equal(r4.payload?.success, true)

        assert.equal(r5.id, 'ipc-concurrency-client-5')
        assert.equal(r5.status, 'ok')
        assert.deepEqual(r5.data, { pong: true })
      })
    })()

    // 6.3 Selective client disconnect and continuity for surviving clients
    await (async () => {
      // Disconnect client 1 & 2
      connections[0].close()
      connections[1].close()

      await sleep(100)

      // Test that client 3, 4, 5 are still fully functional
      const r3 = await connections[2].sendEvent('ping', {})
      const r4 = await connections[3].sendEvent('ping', {})
      const r5 = await connections[4].sendEvent('ping', {})

      reporter.test('6.3 Đóng kết nối của một số client không làm ảnh hưởng đến các client còn lại', () => {
        assert.equal(r3.event, 'ping_response')
        assert.equal(r4.event, 'ping_response')
        assert.equal(r5.event, 'ping_response')
      })
    })()

  } finally {
    connections.forEach((ws) => {
      try { ws.close() } catch { /* ignore */ }
    })
  }

  reporter.endSection()
}
