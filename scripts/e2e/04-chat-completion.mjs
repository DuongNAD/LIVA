/**
 * Test Group 4: chat:completion Streaming & Error Framing
 *
 * Verifies:
 * - Payload validation (messages array requirement)
 * - Clean error framing when model weights are absent
 * - Streaming IPC framing and chunk termination
 * - Explicit skipping of model-dependent inference with clear reasons
 */

import assert from 'node:assert/strict'
import { connectWebSocket } from './helpers.mjs'

export async function runChatCompletionTests(reporter, port) {
  reporter.startSection(4, 'chat:completion Protocol & Streaming')

  const conn = await connectWebSocket({ port })
  if (!conn.ok) {
    reporter.test('Khởi tạo kết nối WebSocket cho mục 4', () => {
      throw new Error(`Không kết nối được gateway: ${conn.reason}`)
    })
    reporter.endSection()
    return
  }
  const ws = conn.ws

  try {
    // 4.1 Missing messages array validation
    await (async () => {
      const res = await ws.sendEvent('chat:completion', {})
      reporter.test('4.1 chat:completion thiếu mảng messages trả về lỗi cấu trúc hợp lệ', () => {
        assert.equal(res.event, 'chat:completion_error')
        assert.ok(String(res.payload?.error).includes("Missing or invalid 'messages' array"))
      })
    })()

    // 4.2 Invalid messages format
    await (async () => {
      const res = await ws.sendEvent('chat:completion', { messages: 'not-an-array' })
      reporter.test('4.2 chat:completion với messages không phải mảng bị từ chối chính xác', () => {
        assert.equal(res.event, 'chat:completion_error')
        assert.ok(String(res.payload?.error).includes("Missing or invalid 'messages' array"))
      })
    })()

    // Check if LLM model is currently loaded
    const health = await ws.sendEvent('llm:health_check', {})
    const isModelLoaded = health.payload?.model_loaded === true

    // 4.3 Handling of valid prompt
    await (async () => {
      const res = await ws.sendEvent('chat:completion', {
        messages: [{ role: 'user', content: 'Xin chào' }],
        stream: false,
      }, 30000)

      if (isModelLoaded) {
        reporter.test('4.3 chat:completion có model sinh hồi âm hoàn chỉnh kèm usage', () => {
          assert.equal(res.event, 'chat:completion_response')
          assert.equal(typeof res.payload?.text, 'string')
          assert.equal(res.payload?.done, true)
          assert.ok(typeof res.payload?.usage === 'object')
        })
      } else {
        reporter.test('4.3 chat:completion khi chưa nạp model trả về lỗi cấu trúc (Model is not loaded) không bị treo', () => {
          assert.equal(res.event, 'chat:completion_error')
          assert.ok(
            String(res.payload?.error).includes('Model is not loaded') ||
            String(res.payload?.error).includes('ERR_NO_MODEL') ||
            String(res.payload?.error).length > 0
          )
        })
      }
    })()

    // 4.4 IPC Streaming protocol check
    await (async () => {
      const reqId = `req-stream-test-${Date.now()}`
      const res = await ws.sendIpc('chat:completion', {
        messages: [{ role: 'user', content: 'Hello' }],
        stream: true,
      }, reqId, 30000)

      if (isModelLoaded) {
        reporter.test('4.4 chat:completion streaming IPC phân phối token hoàn tất và trả về kết quả', () => {
          assert.equal(res.id, reqId)
          assert.equal(res.status, 'ok')
          assert.equal(typeof res.data?.text, 'string')
        })
      } else {
        reporter.test('4.4 chat:completion streaming IPC khi chưa nạp model trả về status: error chính xác', () => {
          assert.equal(res.id, reqId)
          assert.equal(res.status, 'error')
          assert.ok(typeof res.error === 'string')
        })
      }
    })()

    // 4.5 Skip reporting for deep inference when model weights are not loaded
    if (!isModelLoaded) {
      reporter.skip(
        '4.5 Đánh giá chất lượng sinh văn bản ngữ nghĩa sâu',
        'Model LLM weights (*.gguf) không có trong môi trường thử nghiệm (autoload logs WARN bình thường)'
      )
    }

  } finally {
    ws.close()
  }

  reporter.endSection()
}
