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
 * Mọi dữ liệu nhị phân client gửi lên core đều phải dùng khung 9 byte này.
 * Các sự kiện điều khiển và transcript phải gửi dưới dạng JSON text, không được
 * tự thêm tiền tố MessagePack một byte vì core sẽ đọc nhầm thành VoiceFrame.
 */

/** VoiceFrame header size: opcode (u8) + seqId (u32 LE) + payloadSize (u32 LE). */
export const VOICE_FRAME_HEADER_SIZE = 9;

/** Opcode: handshake khởi tạo phiên. */
export const OP_AUTH_HANDSHAKE = 0x00;
/** Opcode: audio từ micro của client gửi lên core. */
export const OP_MIC_IN = 0x01;
/**
 * Opcode: gửi MỘT câu ứng viên để core xác minh có phải cụm đánh thức không.
 * Cùng định dạng payload với `OP_MIC_IN` (f32 LE 16 kHz mono) nhưng là cả câu đã
 * cắt sẵn. Core trả về sự kiện text `wake_word_triggered` (khớp) hoặc
 * `wake_probe_rejected` (không khớp, kèm transcript nó nghe ra).
 *
 * KHÔNG dùng `OP_MIC_IN` cho việc này: khung mic chạy thẳng vào pipeline → LLM.
 */
export const OP_WAKE_PROBE = 0x05;

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
  // Trả `Uint8Array<ArrayBuffer>` chứ không phải `Uint8Array` trần: từ TS 5.7
  // kiểu này generic theo `ArrayBufferLike`, và `WebSocket.send()` chỉ nhận
  // `ArrayBufferView<ArrayBuffer>` — buffer ở đây luôn là `ArrayBuffer` thật
  // (dựng ngay bên dưới), nên nói rõ ra để nơi gọi không phải ép kiểu.
): Uint8Array<ArrayBuffer> {
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
