/**
 * Test Group 5: Voice Pipeline Lifecycle (STT & TTS)
 *
 * Verifies the 5 voice commands in REMOTE_COMMANDS:
 * - voice:stt_start
 * - voice:stt_chunk
 * - voice:stt_stop
 * - voice:tts_speak
 * - voice:tts_stop
 *
 * Validates parameter constraints, audio decoding, lifecycle framing,
 * and error behavior when model weights are absent.
 */

import assert from 'node:assert/strict'
import { connectWebSocket } from './helpers.mjs'

export async function runVoiceLifecycleTests(reporter, port) {
  reporter.startSection(5, 'Voice Pipeline Lifecycle (STT & TTS)')

  const conn = await connectWebSocket({ port })
  if (!conn.ok) {
    reporter.test('Khởi tạo kết nối WebSocket cho mục 5', () => {
      throw new Error(`Không kết nối được gateway: ${conn.reason}`)
    })
    reporter.endSection()
    return
  }
  const ws = conn.ws

  try {
    // 5.1 voice:stt_start
    await (async () => {
      const res = await ws.sendEvent('voice:stt_start', {})
      reporter.test('5.1 voice:stt_start khởi tạo lại audio stream và trả về { success: true }', () => {
        assert.equal(res.event, 'voice:stt_start_response')
        assert.equal(res.payload?.success, true)
      })
    })()

    // 5.2 voice:stt_chunk missing chunk
    await (async () => {
      const res = await ws.sendEvent('voice:stt_chunk', {})
      reporter.test('5.2 voice:stt_chunk thiếu trường chunk trả về lỗi cấu trúc hợp lệ', () => {
        assert.equal(res.event, 'voice:stt_chunk_error')
        assert.ok(String(res.payload?.error).includes("Missing 'chunk'"))
      })
    })()

    // 5.3 voice:stt_chunk invalid base64
    await (async () => {
      const res = await ws.sendEvent('voice:stt_chunk', { chunk: '!!!invalid_base64!!!' })
      reporter.test('5.3 voice:stt_chunk nhận chuỗi base64 lỗi trả về lỗi giải mã hợp lệ', () => {
        assert.equal(res.event, 'voice:stt_chunk_error')
        assert.ok(String(res.payload?.error).includes('Base64 decode failed'))
      })
    })()

    // 5.4 voice:stt_chunk valid base64 audio samples (16 f32 zeroes)
    await (async () => {
      const pcmZeroes = Buffer.alloc(64).toString('base64')
      const res = await ws.sendEvent('voice:stt_chunk', { chunk: pcmZeroes, isLast: false }, 15000)
      reporter.test('5.4 voice:stt_chunk nạp audio f32 hợp lệ phản hồi có cấu trúc và không bị treo', () => {
        assert.ok(res.event === 'voice:stt_chunk_response' || res.event === 'voice:stt_chunk_error')
        if (res.event === 'voice:stt_chunk_response') {
          assert.ok('text' in (res.payload || {}))
        } else {
          assert.ok(typeof res.payload?.error === 'string')
        }
      })
    })()

    // 5.5 voice:stt_stop
    await (async () => {
      const res = await ws.sendEvent('voice:stt_stop', {}, 15000)
      reporter.test('5.5 voice:stt_stop hoàn tất chu trình stream STT và trả về phản hồi hợp lệ', () => {
        assert.ok(res.event === 'voice:stt_stop_response' || res.event === 'voice:stt_stop_error')
        if (res.event === 'voice:stt_stop_response') {
          assert.ok('text' in (res.payload || {}))
        }
      })
    })()

    // 5.6 voice:tts_speak missing text
    await (async () => {
      const res = await ws.sendEvent('voice:tts_speak', {})
      reporter.test('5.6 voice:tts_speak thiếu trường text trả về lỗi cấu trúc', () => {
        assert.equal(res.event, 'voice:tts_speak_error')
        assert.ok(String(res.payload?.error).includes("Missing 'text'"))
      })
    })()

    // 5.7 voice:tts_speak valid text
    await (async () => {
      const res = await ws.sendEvent('voice:tts_speak', { text: 'Kiểm tra phát âm' }, 15000)
      reporter.test('5.7 voice:tts_speak phản hồi có cấu trúc (thành công hoặc TTS chưa nạp) không bị treo', () => {
        assert.ok(res.event === 'voice:tts_speak_response' || res.event === 'voice:tts_speak_error')
        if (res.event === 'voice:tts_speak_response') {
          assert.equal(res.payload?.success, true)
        } else {
          assert.ok(String(res.payload?.error).includes('TTS engine not initialized') || typeof res.payload?.error === 'string')
        }
      })
    })()

    // 5.8 voice:tts_stop
    await (async () => {
      const res = await ws.sendEvent('voice:tts_stop', {})
      reporter.test('5.8 voice:tts_stop dừng audio player và trả về { success: true }', () => {
        assert.equal(res.event, 'voice:tts_stop_response')
        assert.equal(res.payload?.success, true)
      })
    })()

  } finally {
    ws.close()
  }

  reporter.endSection()
}
