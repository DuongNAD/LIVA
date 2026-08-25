/**
 * speakerFrame.ts — VoiceBinaryProtocol OP_SPEAKER_OUT payload parsing
 * ====================================================================
 * The Rust core (liva-native-core/src/webrtc) streams TTS audio as binary
 * VoiceFrames: [opcode u8][seqId u32 LE][payloadSize u32 LE][payload…].
 * For OP_SPEAKER_OUT the payload is raw PCM:
 *
 *   [u32 little-endian turn_epoch][u32 little-endian sample_rate]
 *   [f32 little-endian mono PCM samples…]
 *
 * sample_rate is 22050 (Piper voices) or 24000 (Kokoro).
 */

/** VoiceFrame header size: opcode (u8) + seqId (u32 LE) + payloadSize (u32 LE). */
export const VOICE_FRAME_HEADER_SIZE = 9;

/** Opcode: TTS speaker audio streamed from the core to the client. */
export const OP_SPEAKER_OUT = 0x02;
/** Opcode: barge-in — immediately clear audio queues and stop playback. */
export const OP_FLUSH = 0x03;
/**
 * Opcode: phoneme→viseme timeline đi kèm các frame loa của cùng mẩu (VC-8).
 * Payload là JSON `{turnEpoch, baseSeqId, cues:[{v,tMs}…]}` trong đó `tMs`
 * tính từ mẫu PCM đầu tiên của chunk `base_seq_id`. Client cũ không nhận diện
 * opcode này sẽ bỏ qua an toàn (giữ đường lip-sync RMS).
 */
export const OP_VISME = 0x06;

const TURN_EPOCH_BYTES = 4;
const SAMPLE_RATE_BYTES = 4;
const SPEAKER_METADATA_BYTES = TURN_EPOCH_BYTES + SAMPLE_RATE_BYTES;
const BYTES_PER_SAMPLE = 4;
/** Web Audio AudioBuffers accept 8kHz–96kHz; anything outside is malformed. */
const MIN_SAMPLE_RATE = 8000;
const MAX_SAMPLE_RATE = 96000;

export interface SpeakerChunk {
  turnEpoch: number;
  sampleRate: number;
  samples: Float32Array<ArrayBuffer>;
}

/** Parse an OP_SPEAKER_OUT payload; callers must drop malformed binary frames. */
export function parseSpeakerPayload(payload: ArrayBuffer | Uint8Array): SpeakerChunk | null {
  const bytes = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
  const byteLength = bytes.byteLength;

  // Must hold epoch + sample-rate metadata plus at least one whole f32 sample.
  if (byteLength < SPEAKER_METADATA_BYTES + BYTES_PER_SAMPLE) return null;
  if ((byteLength - SPEAKER_METADATA_BYTES) % BYTES_PER_SAMPLE !== 0) return null;

  // bytes may be a view into a larger buffer (e.g. a WS frame sliced past its
  // 9-byte header) — always honour its byteOffset, never read from offset 0.
  const view = new DataView(bytes.buffer, bytes.byteOffset, byteLength);
  const turnEpoch = view.getUint32(0, true);
  const sampleRate = view.getUint32(TURN_EPOCH_BYTES, true);
  if (sampleRate < MIN_SAMPLE_RATE || sampleRate > MAX_SAMPLE_RATE) return null;

  const sampleCount = (byteLength - SPEAKER_METADATA_BYTES) / BYTES_PER_SAMPLE;
  const samples = new Float32Array(sampleCount);
  const samplesByteOffset = bytes.byteOffset + SPEAKER_METADATA_BYTES;

  if (samplesByteOffset % Float32Array.BYTES_PER_ELEMENT === 0) {
    // 4-byte aligned — bulk copy through a typed-array view.
    samples.set(new Float32Array(bytes.buffer, samplesByteOffset, sampleCount));
  } else {
    // VoiceFrame payloads start at byte 9 of the WS frame, which is not
    // 4-byte aligned, so read sample-by-sample via the DataView.
    for (let i = 0; i < sampleCount; i++) {
      samples[i] = view.getFloat32(SPEAKER_METADATA_BYTES + i * BYTES_PER_SAMPLE, true);
    }
  }

  return { turnEpoch, sampleRate, samples };
}

/** Monotonic client-side watermark that rejects speaker chunks from cancelled turns. */
export class SpeakerEpochGate {
  private minimumEpoch = 0;

  observeFlush(epoch: number): void {
    this.minimumEpoch = Math.max(this.minimumEpoch, epoch >>> 0);
  }

  accepts(epoch: number): boolean {
    return (epoch >>> 0) >= this.minimumEpoch;
  }
}

// ═══════════════════════════════════════════════════════════════
//  OP_VISME — phoneme→viseme timeline (VC-8)
// ═══════════════════════════════════════════════════════════════

/** Tập viseme trung gian mà core phát ra — trùng preset biểu cảm VRM chuẩn. */
export type VisemeCode = "aa" | "ee" | "ih" | "oh" | "ou" | "nil";
const VISEME_CODES: readonly string[] = ["aa", "ee", "ih", "oh", "ou", "nil"];

export interface VisemeCue {
  v: VisemeCode;
  tMs: number;
}

export interface VisemeTimelinePayload {
  turnEpoch: number;
  baseSeqId: number;
  cues: VisemeCue[];
}

/**
 * Parse an OP_VISME payload (UTF-8 JSON). Fail-closed: bất kỳ trường nào sai
 * kiểu, viseme ngoài whitelist hay `tMs` không tăng ngặt đều làm CẢ timeline
 * bị loại (`null`) thay vì bỏ từng cue — một timeline lệch nửa chừng còn tệ
 * hơn hẳn không có (client sẽ rơi về RMS).
 */
export function parseVisemePayload(payload: ArrayBuffer | Uint8Array): VisemeTimelinePayload | null {
  const bytes = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
  let raw: unknown;
  try {
    raw = JSON.parse(new TextDecoder().decode(bytes));
  } catch {
    return null;
  }
  if (typeof raw !== "object" || raw === null) return null;
  const obj = raw as Record<string, unknown>;

  const turnEpoch = obj.turnEpoch ?? obj.turn_epoch;
  const baseSeqId = obj.baseSeqId ?? obj.base_seq_id;
  if (typeof turnEpoch !== "number" || !Number.isInteger(turnEpoch) || turnEpoch < 0) return null;
  if (typeof baseSeqId !== "number" || !Number.isInteger(baseSeqId) || baseSeqId < 0) return null;
  // Core phát trường `visemes` (serde_json::json! trong pipeline.rs).
  const rawCues = obj.visemes ?? obj.cues;
  if (!Array.isArray(rawCues) || rawCues.length === 0) return null;

  const cues: VisemeCue[] = [];
  let prevTMs = -1;
  for (const item of rawCues) {
    if (typeof item !== "object" || item === null) return null;
    const cue = item as Record<string, unknown>;
    const v = cue.v;
    const tMs = cue.tMs ?? cue.t_ms;
    if (typeof v !== "string" || !VISEME_CODES.includes(v)) return null;
    if (typeof tMs !== "number" || !Number.isInteger(tMs) || tMs < 0 || tMs <= prevTMs) return null;
    prevTMs = tMs;
    cues.push({ v: v as VisemeCode, tMs });
  }

  return { turnEpoch, baseSeqId, cues };
}
