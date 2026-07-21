import { describe, it, expect } from "vitest";
import {
  serializeVoiceFrame,
  OP_MIC_IN,
  OP_AUTH_HANDSHAKE,
  VOICE_FRAME_HEADER_SIZE,
  MAX_PAYLOAD_BYTES,
} from "../../src/utils/voiceFrame";

/**
 * Bộ giải mã đối chiếu, viết bám sát `liva-native-core/src/webrtc/frame.rs`.
 * Nếu encoder lệch hợp đồng thì hàm này phát hiện ra — đây chính là lỗi F3:
 * client ghi header 1 byte trong khi core đọc 9 byte.
 */
function decodeLikeCore(
  bytes: Uint8Array,
): { opCode: number; seqId: number; payloadLen: number; payload: Uint8Array } | null {
  if (bytes.byteLength < VOICE_FRAME_HEADER_SIZE) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const opCode = view.getUint8(0);
  const seqId = view.getUint32(1, true);
  const payloadLen = view.getUint32(5, true);
  if (payloadLen > MAX_PAYLOAD_BYTES) return null; // core: "Payload exceeds 1MB limit"
  if (bytes.byteLength < VOICE_FRAME_HEADER_SIZE + payloadLen) return null;
  return {
    opCode,
    seqId,
    payloadLen,
    payload: bytes.subarray(VOICE_FRAME_HEADER_SIZE, VOICE_FRAME_HEADER_SIZE + payloadLen),
  };
}

describe("serializeVoiceFrame", () => {
  it("ghi header 9 byte đúng thứ tự little-endian", () => {
    const payload = new Uint8Array([1, 2, 3, 4]);
    const frame = serializeVoiceFrame(OP_MIC_IN, 0x01020304, payload);

    expect(frame.byteLength).toBe(VOICE_FRAME_HEADER_SIZE + 4);
    expect(frame[0]).toBe(OP_MIC_IN);
    // seqId 0x01020304 little-endian => 04 03 02 01
    expect(Array.from(frame.subarray(1, 5))).toEqual([0x04, 0x03, 0x02, 0x01]);
    // payloadSize 4 little-endian => 04 00 00 00
    expect(Array.from(frame.subarray(5, 9))).toEqual([4, 0, 0, 0]);
    expect(Array.from(frame.subarray(9))).toEqual([1, 2, 3, 4]);
  });

  it("core giải mã lại được đúng những gì đã gửi", () => {
    const payload = new Uint8Array([9, 8, 7]);
    const decoded = decodeLikeCore(serializeVoiceFrame(OP_MIC_IN, 42, payload));

    expect(decoded).not.toBeNull();
    expect(decoded!.opCode).toBe(OP_MIC_IN);
    expect(decoded!.seqId).toBe(42);
    expect(decoded!.payloadLen).toBe(3);
    expect(Array.from(decoded!.payload)).toEqual([9, 8, 7]);
  });

  it("giữ nguyên byte của PCM f32 little-endian", () => {
    const samples = new Float32Array([0, 1, -1, 0.5, -0.25]);
    const frame = serializeVoiceFrame(OP_MIC_IN, 0, new Uint8Array(samples.buffer));
    const decoded = decodeLikeCore(frame)!;

    const view = new DataView(
      decoded.payload.buffer,
      decoded.payload.byteOffset,
      decoded.payload.byteLength,
    );
    const roundTripped = Array.from({ length: samples.length }, (_, i) =>
      view.getFloat32(i * 4, true),
    );
    expect(roundTripped).toEqual(Array.from(samples));
  });

  /** Đây là hồi quy cho chính bug F3. */
  it("KHÔNG dùng header 1 byte như bản cũ", () => {
    const samples = new Float32Array([0.1, 0.2, 0.3, 0.4]);
    const pcm = new Uint8Array(samples.buffer);

    // Bản cũ: msg[0] = 0x01 rồi nối thẳng PCM.
    const legacy = new Uint8Array(1 + pcm.byteLength);
    legacy[0] = OP_MIC_IN;
    legacy.set(pcm, 1);

    // Core đọc 4 byte PCM đầu làm seqId và 4 byte kế làm payloadSize.
    const legacyDecoded = decodeLikeCore(legacy);
    const fixedDecoded = decodeLikeCore(serializeVoiceFrame(OP_MIC_IN, 0, pcm));

    expect(fixedDecoded).not.toBeNull();
    expect(fixedDecoded!.payloadLen).toBe(pcm.byteLength);
    // Bản cũ hoặc bị từ chối hẳn, hoặc ra payloadLen sai — không bao giờ đúng.
    expect(legacyDecoded === null || legacyDecoded.payloadLen !== pcm.byteLength).toBe(true);
  });

  it("seqId quấn vòng như u32 bên Rust", () => {
    const payload = new Uint8Array(0);
    expect(decodeLikeCore(serializeVoiceFrame(OP_MIC_IN, 0xffffffff, payload))!.seqId).toBe(
      0xffffffff,
    );
    // 2^32 quấn về 0
    expect(decodeLikeCore(serializeVoiceFrame(OP_MIC_IN, 0x1_0000_0000, payload))!.seqId).toBe(0);
    expect(decodeLikeCore(serializeVoiceFrame(OP_MIC_IN, -1, payload))!.seqId).toBe(0xffffffff);
  });

  it("payload rỗng vẫn là khung hợp lệ", () => {
    const frame = serializeVoiceFrame(OP_AUTH_HANDSHAKE, 0, new Uint8Array(0));
    expect(frame.byteLength).toBe(VOICE_FRAME_HEADER_SIZE);
    const decoded = decodeLikeCore(frame)!;
    expect(decoded.opCode).toBe(OP_AUTH_HANDSHAKE);
    expect(decoded.payloadLen).toBe(0);
  });

  it("ném lỗi thay vì để core lặng lẽ đóng kết nối khi payload quá lớn", () => {
    const tooBig = new Uint8Array(MAX_PAYLOAD_BYTES + 1);
    expect(() => serializeVoiceFrame(OP_MIC_IN, 0, tooBig)).toThrow(RangeError);
    // đúng bằng giới hạn thì vẫn cho qua
    expect(() => serializeVoiceFrame(OP_MIC_IN, 0, new Uint8Array(MAX_PAYLOAD_BYTES))).not.toThrow();
  });
});
