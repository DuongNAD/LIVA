/**
 * speakerFrame.ts — VoiceBinaryProtocol OP_SPEAKER_OUT payload parsing
 * ====================================================================
 * The Rust core (liva-native-core/src/webrtc) streams TTS audio as binary
 * VoiceFrames: [opcode u8][seqId u32 LE][payloadSize u32 LE][payload…].
 * For OP_SPEAKER_OUT the payload is raw PCM:
 *
 *   [u32 little-endian sample_rate][f32 little-endian mono PCM samples…]
 *
 * sample_rate is 22050 (Piper voices) or 24000 (Kokoro).
 */

/** VoiceFrame header size: opcode (u8) + seqId (u32 LE) + payloadSize (u32 LE). */
export const VOICE_FRAME_HEADER_SIZE = 9;

/** Opcode: TTS speaker audio streamed from the core to the client. */
export const OP_SPEAKER_OUT = 0x02;
/** Opcode: barge-in — immediately clear audio queues and stop playback. */
export const OP_FLUSH = 0x03;

const SAMPLE_RATE_BYTES = 4;
const BYTES_PER_SAMPLE = 4;
/** Web Audio AudioBuffers accept 8kHz–96kHz; anything outside is malformed. */
const MIN_SAMPLE_RATE = 8000;
const MAX_SAMPLE_RATE = 96000;

export interface SpeakerChunk {
  sampleRate: number;
  samples: Float32Array<ArrayBuffer>;
}

/**
 * Parse an OP_SPEAKER_OUT payload; returns null if malformed
 * (the caller may then fall back to the legacy MP3 decode path).
 */
export function parseSpeakerPayload(payload: ArrayBuffer | Uint8Array): SpeakerChunk | null {
  const bytes = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
  const byteLength = bytes.byteLength;

  // Must hold the sample-rate header plus at least one whole f32 sample.
  if (byteLength < SAMPLE_RATE_BYTES + BYTES_PER_SAMPLE) return null;
  if ((byteLength - SAMPLE_RATE_BYTES) % BYTES_PER_SAMPLE !== 0) return null;

  // bytes may be a view into a larger buffer (e.g. a WS frame sliced past its
  // 9-byte header) — always honour its byteOffset, never read from offset 0.
  const view = new DataView(bytes.buffer, bytes.byteOffset, byteLength);
  const sampleRate = view.getUint32(0, true);
  if (sampleRate < MIN_SAMPLE_RATE || sampleRate > MAX_SAMPLE_RATE) return null;

  const sampleCount = (byteLength - SAMPLE_RATE_BYTES) / BYTES_PER_SAMPLE;
  const samples = new Float32Array(sampleCount);
  const samplesByteOffset = bytes.byteOffset + SAMPLE_RATE_BYTES;

  if (samplesByteOffset % Float32Array.BYTES_PER_ELEMENT === 0) {
    // 4-byte aligned — bulk copy through a typed-array view.
    samples.set(new Float32Array(bytes.buffer, samplesByteOffset, sampleCount));
  } else {
    // VoiceFrame payloads start at byte 9 of the WS frame, which is not
    // 4-byte aligned, so read sample-by-sample via the DataView.
    for (let i = 0; i < sampleCount; i++) {
      samples[i] = view.getFloat32(SAMPLE_RATE_BYTES + i * BYTES_PER_SAMPLE, true);
    }
  }

  return { sampleRate, samples };
}
