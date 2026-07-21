/**
 * voiceFrame.ts — VoiceBinaryProtocol encoder (client → core)
 * ===========================================================
 * Đối xứng với `speakerFrame.ts` (core → client). Hợp đồng nhị phân khớp
 * `liva-native-core/src/webrtc/frame.rs`:
 *
 *   [opcode u8][seqId u32 LE][payloadSize u32 LE][payload…]
 *
 * Core từ chối mọi khung ngắn hơn 9 byte và mọi `payloadSize` vượt 1 MiB.
 *
 * ⚠️ Đừng nhầm với tiền tố **1 byte** `0x02` mà `useVoicePipeline.ts` dùng cho
 * sự kiện MessagePack — đó là giao thức khác, đi qua nhánh khác. Chỉ đường
 * audio (`OP_MIC_IN`) mới dùng khung 9 byte này.
 */

/** VoiceFrame header size: opcode (u8) + seqId (u32 LE) + payloadSize (u32 LE). */
export const VOICE_FRAME_HEADER_SIZE = 9;

/** Opcode: handshake khởi tạo phiên. */
export const OP_AUTH_HANDSHAKE = 0x00;
/** Opcode: audio từ micro của client gửi lên core. */
export const OP_MIC_IN = 0x01;

/** Giới hạn payload của core (`frame.rs`); vượt là core đóng kết nối. */
export const MAX_PAYLOAD_BYTES = 1024 * 1024;

/**
 * Đóng gói một VoiceFrame.
 *
 * `payload` cho `OP_MIC_IN` là PCM **f32 little-endian, 16 kHz, mono** — đúng
 * bằng byte của một `Float32Array` trên x86/ARM, nên chỉ cần bọc header, tuyệt
 * đối không chuyển sang i16.
 *
 * @throws nếu payload vượt giới hạn của core — ném ở đây dễ chẩn đoán hơn
 *         nhiều so với việc core lặng lẽ đóng kết nối.
 */
export function serializeVoiceFrame(
  opcode: number,
  seqId: number,
  payload: Uint8Array,
): Uint8Array {
  if (payload.byteLength > MAX_PAYLOAD_BYTES) {
    throw new RangeError(
      `VoiceFrame payload ${payload.byteLength} byte vượt giới hạn ${MAX_PAYLOAD_BYTES} của core`,
    );
  }

  const buffer = new ArrayBuffer(VOICE_FRAME_HEADER_SIZE + payload.byteLength);
  const view = new DataView(buffer);
  view.setUint8(0, opcode & 0xff);
  // `>>> 0` để seqId âm hoặc vượt 2^32 vẫn quấn vòng đúng như u32 bên Rust.
  view.setUint32(1, seqId >>> 0, true);
  view.setUint32(5, payload.byteLength, true);

  const bytes = new Uint8Array(buffer);
  bytes.set(payload, VOICE_FRAME_HEADER_SIZE);
  return bytes;
}
