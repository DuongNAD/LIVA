export function resampleLinear(input, fromRate, toRate) {
  if (!(input instanceof Float32Array)) {
    throw new TypeError('input phải là Float32Array')
  }
  if (fromRate <= 0 || toRate <= 0) {
    throw new RangeError('sample rate phải lớn hơn 0')
  }
  if (fromRate === toRate) return new Float32Array(input)
  if (input.length === 0) return new Float32Array()

  const ratio = fromRate / toRate
  const output = new Float32Array(Math.max(1, Math.round(input.length / ratio)))
  for (let i = 0; i < output.length; i += 1) {
    const source = i * ratio
    const lower = Math.min(Math.floor(source), input.length - 1)
    const upper = Math.min(lower + 1, input.length - 1)
    const fraction = source - lower
    output[i] = input[lower] * (1 - fraction) + input[upper] * fraction
  }
  return output
}

function writeAscii(view, offset, value) {
  for (let i = 0; i < value.length; i += 1) {
    view.setUint8(offset + i, value.charCodeAt(i))
  }
}

export function encodePcm16Wav(samples, sampleRate = 16_000) {
  if (!(samples instanceof Float32Array)) {
    throw new TypeError('samples phải là Float32Array')
  }
  if (!Number.isInteger(sampleRate) || sampleRate <= 0) {
    throw new RangeError('sampleRate phải là số nguyên dương')
  }

  const dataBytes = samples.length * 2
  const buffer = new ArrayBuffer(44 + dataBytes)
  const view = new DataView(buffer)
  writeAscii(view, 0, 'RIFF')
  view.setUint32(4, 36 + dataBytes, true)
  writeAscii(view, 8, 'WAVE')
  writeAscii(view, 12, 'fmt ')
  view.setUint32(16, 16, true)
  view.setUint16(20, 1, true)
  view.setUint16(22, 1, true)
  view.setUint32(24, sampleRate, true)
  view.setUint32(28, sampleRate * 2, true)
  view.setUint16(32, 2, true)
  view.setUint16(34, 16, true)
  writeAscii(view, 36, 'data')
  view.setUint32(40, dataBytes, true)

  for (let i = 0; i < samples.length; i += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[i]))
    const pcm = clamped < 0 ? Math.round(clamped * 32_768) : Math.round(clamped * 32_767)
    view.setInt16(44 + i * 2, pcm, true)
  }
  return buffer
}
