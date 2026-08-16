/**
 * Test Group 1: Protocol Framing & Correlation
 *
 * Verifies over a real WebSocket connection:
 * - Event-based framing (<cmd> -> <cmd>_response / <cmd>_error)
 * - IPC id correlation on success and error
 * - Malformed JSON handling
 * - Oversized frame rejection (> 1MB)
 * - Unknown commands error response (anti-silent-swallow regression check)
 */

import assert from 'node:assert/strict'
import { connectWebSocket, sleep } from './helpers.mjs'

export async function runProtocolFramingTests(reporter, port) {
  reporter.startSection(1, 'Protocol Framing & Correlation')

  const conn = await connectWebSocket({ port })
  if (!conn.ok) {
    reporter.test('Khởi tạo kết nối WebSocket cho mục 1', () => {
      throw new Error(`Không kết nối được gateway: ${conn.reason}`)
    })
    reporter.endSection()
    return
  }
  const ws = conn.ws

  try {
    // 1.1 Legacy event framing
    await (async () => {
      const res = await ws.sendEvent('ping', {})
      reporter.test('1.1 Giao thức Event: ping trả đúng sự kiện ping_response', () => {
        assert.equal(res.event, 'ping_response')
        assert.equal(res.payload?.pong, true)
      })
    })()

    // 1.2 IPC framing on success & ID correlation
    await (async () => {
      const customId = `req-test-success-${Date.now()}`
      const res = await ws.sendIpc('ping', {}, customId)
      reporter.test('1.2 Giao thức IPC: ID request được ánh xạ chính xác về ID response', () => {
        assert.equal(res.id, customId)
        assert.equal(res.status, 'ok')
        assert.deepEqual(res.data, { pong: true })
      })
    })()

    // 1.3 IPC ID correlation on error
    await (async () => {
      const customId = `req-test-err-${Date.now()}`
      const res = await ws.sendIpc('lenh_khong_ton_tai_123', {}, customId)
      reporter.test('1.3 Giao thức IPC: ID request được ánh xạ chính xác trên phản hồi lỗi', () => {
        assert.equal(res.id, customId)
        assert.equal(res.status, 'error')
        assert.ok(typeof res.error === 'string' && res.error.length > 0)
      })
    })()

    // 1.4 Malformed JSON text frame
    await (async () => {
      const p = ws.waitForNextJson(5000)
      ws.sendRawText('{"this_is_malformed_json: [true,')
      const res = await p
      reporter.test('1.4 Frame JSON lỗi cú pháp trả về frame lỗi có cấu trúc id=unknown', () => {
        assert.ok(res !== null, 'Phải nhận được frame hồi đáp')
        assert.equal(res.id, 'unknown')
        assert.equal(res.status, 'error')
        assert.ok(String(res.error).includes('Invalid JSON query'))
      })
    })()

    // 1.6 Unknown command generates <cmd>_error (regression for swallowed error)
    await (async () => {
      const res = await ws.sendEvent('lenh_khong_ton_tai_456', {}, 10000)
      reporter.test('1.6 Lệnh không tồn tại trả *_error thay vì im lặng (chống lỗi nuốt nhánh Err)', () => {
        assert.equal(res.event, 'lenh_khong_ton_tai_456_error')
      })
      reporter.test('1.7 Payload lỗi lệnh không tồn tại chứa trường command và error', () => {
        assert.equal(res.payload?.command, 'lenh_khong_ton_tai_456')
        assert.ok(typeof res.payload?.error === 'string')
      })
    })()

    ws.close()

    // 1.5 Oversized frame rejection (> 1024 * 1024 bytes)
    await (async () => {
      const connLarge = await connectWebSocket({ port })
      if (!connLarge.ok) {
        reporter.test('1.5 Từ chối frame vượt quá dung lượng giới hạn (> 1MB)', () => {
          throw new Error('Không mở được kết nối thử nghiệm frame lớn')
        })
        return
      }

      let closed = false
      connLarge.ws.on('close', () => { closed = true })

      // Send 1.5 MB text frame
      const bigPayload = 'x'.repeat(1024 * 1024 + 1024)
      connLarge.ws.sendRawText(bigPayload)

      await sleep(500)

      reporter.test('1.5 Frame vượt giới hạn 1MB bị máy chủ đóng ngắt kết nối', () => {
        assert.ok(closed || connLarge.ws.rawSocket.destroyed, 'Kết nối phải bị đóng khi gửi payload quá lớn')
      })
      connLarge.ws.close()
    })()

  } catch (err) {
    ws.close()
    throw err
  }

  reporter.endSection()
}
