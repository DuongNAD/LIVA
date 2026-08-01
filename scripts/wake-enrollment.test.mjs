import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import {
  encodePcm16Wav,
  resampleLinear,
} from '../liva-ui/public/wake-enrollment-core.js'

test('enrollment export tạo WAV PCM16 mono 16 kHz hợp lệ', () => {
  const source = new Float32Array(48_000)
  for (let i = 0; i < source.length; i += 1) {
    source[i] = 0.5 * Math.sin((2 * Math.PI * 220 * i) / 48_000)
  }

  const resampled = resampleLinear(source, 48_000, 16_000)
  const wav = encodePcm16Wav(resampled, 16_000)
  const view = new DataView(wav)

  assert.equal(String.fromCharCode(...new Uint8Array(wav, 0, 4)), 'RIFF')
  assert.equal(String.fromCharCode(...new Uint8Array(wav, 8, 4)), 'WAVE')
  assert.equal(view.getUint16(20, true), 1)
  assert.equal(view.getUint16(22, true), 1)
  assert.equal(view.getUint32(24, true), 16_000)
  assert.equal(view.getUint16(34, true), 16)
  assert.equal(view.getUint32(40, true), resampled.length * 2)
})

test('trang microphone có enrollment rõ ràng và không tự ghi âm ngầm', () => {
  const html = readFileSync('liva-ui/public/wake-word-test.html', 'utf8')

  assert.match(html, /id="btnRecordSample"/u)
  assert.match(html, /id="enrollmentCount"/u)
  assert.match(html, /Hey Liva/u)
  assert.match(html, /chỉ ghi khi bạn bấm/u)
})
