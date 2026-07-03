import { describe, it, expect } from "vitest";
import {
  parseSpeakerPayload,
  OP_SPEAKER_OUT,
  OP_FLUSH,
  VOICE_FRAME_HEADER_SIZE,
} from "../../src/utils/speakerFrame";

/** Build an OP_SPEAKER_OUT payload: [u32 LE sample_rate][f32 LE samples…]. */
function encodeSpeakerPayload(sampleRate: number, samples: number[]): ArrayBuffer {
  const buffer = new ArrayBuffer(4 + samples.length * 4);
  const view = new DataView(buffer);
  view.setUint32(0, sampleRate, true);
  samples.forEach((value, i) => view.setFloat32(4 + i * 4, value, true));
  return buffer;
}

/** Wrap a payload in a full VoiceFrame: [opcode u8][seqId u32 LE][payloadSize u32 LE][payload]. */
function encodeVoiceFrame(opcode: number, seqId: number, payload: ArrayBuffer): Uint8Array {
  const payloadBytes = new Uint8Array(payload);
  const frame = new Uint8Array(VOICE_FRAME_HEADER_SIZE + payloadBytes.byteLength);
  const view = new DataView(frame.buffer);
  view.setUint8(0, opcode);
  view.setUint32(1, seqId, true);
  view.setUint32(5, payloadBytes.byteLength, true);
  frame.set(payloadBytes, VOICE_FRAME_HEADER_SIZE);
  return frame;
}

// Values chosen to be exactly representable as f32 so roundtrips compare exactly.
const SAMPLES = [0.5, -0.25, 1, -1, 0.125];

describe("speakerFrame", () => {
  describe("constants", () => {
    it("should match the Rust VoiceBinaryProtocol contract", () => {
      expect(OP_SPEAKER_OUT).toBe(0x02);
      expect(OP_FLUSH).toBe(0x03);
      expect(VOICE_FRAME_HEADER_SIZE).toBe(9);
    });
  });

  describe("parseSpeakerPayload", () => {
    it("should roundtrip a valid Piper payload (22050 Hz) from an ArrayBuffer", () => {
      const result = parseSpeakerPayload(encodeSpeakerPayload(22050, SAMPLES));
      expect(result).not.toBeNull();
      expect(result!.sampleRate).toBe(22050);
      expect(result!.samples).toBeInstanceOf(Float32Array);
      expect(Array.from(result!.samples)).toEqual(SAMPLES);
    });

    it("should roundtrip a valid Kokoro payload (24000 Hz) from a zero-offset Uint8Array", () => {
      const result = parseSpeakerPayload(new Uint8Array(encodeSpeakerPayload(24000, SAMPLES)));
      expect(result).not.toBeNull();
      expect(result!.sampleRate).toBe(24000);
      expect(Array.from(result!.samples)).toEqual(SAMPLES);
    });

    it("should honour a non-zero (unaligned) byteOffset view, as sliced from a VoiceFrame", () => {
      // Real WS handling slices the payload at byteOffset 9, which is not 4-byte aligned.
      const payload = encodeSpeakerPayload(22050, SAMPLES);
      const frame = encodeVoiceFrame(OP_SPEAKER_OUT, 7, payload);
      const payloadView = new Uint8Array(frame.buffer, VOICE_FRAME_HEADER_SIZE, payload.byteLength);

      const result = parseSpeakerPayload(payloadView);
      expect(result).not.toBeNull();
      expect(result!.sampleRate).toBe(22050);
      expect(Array.from(result!.samples)).toEqual(SAMPLES);
    });

    it("should honour a non-zero 4-byte-aligned byteOffset view", () => {
      const payload = new Uint8Array(encodeSpeakerPayload(24000, SAMPLES));
      const backing = new Uint8Array(8 + payload.byteLength);
      backing.fill(0xff, 0, 8); // junk before the view must be ignored
      backing.set(payload, 8);
      const payloadView = new Uint8Array(backing.buffer, 8, payload.byteLength);

      const result = parseSpeakerPayload(payloadView);
      expect(result).not.toBeNull();
      expect(result!.sampleRate).toBe(24000);
      expect(Array.from(result!.samples)).toEqual(SAMPLES);
    });

    it("should return null for payloads shorter than 8 bytes", () => {
      expect(parseSpeakerPayload(new ArrayBuffer(0))).toBeNull();
      expect(parseSpeakerPayload(new ArrayBuffer(4))).toBeNull(); // header only, zero samples
      expect(parseSpeakerPayload(new ArrayBuffer(7))).toBeNull();
    });

    it("should return null when the sample section is not a multiple of 4 bytes", () => {
      const buffer = new ArrayBuffer(10); // 4 header + 6 trailing bytes
      new DataView(buffer).setUint32(0, 22050, true);
      expect(parseSpeakerPayload(buffer)).toBeNull();

      const buffer2 = new ArrayBuffer(13); // 4 header + 9 trailing bytes
      new DataView(buffer2).setUint32(0, 24000, true);
      expect(parseSpeakerPayload(buffer2)).toBeNull();
    });

    it("should return null for sample rates outside [8000, 96000]", () => {
      expect(parseSpeakerPayload(encodeSpeakerPayload(4000, SAMPLES))).toBeNull();
      expect(parseSpeakerPayload(encodeSpeakerPayload(7999, SAMPLES))).toBeNull();
      expect(parseSpeakerPayload(encodeSpeakerPayload(96001, SAMPLES))).toBeNull();
      expect(parseSpeakerPayload(encodeSpeakerPayload(192000, SAMPLES))).toBeNull();
      // MP3 chunks start with 0xFF sync bits → absurd "sample rate" → null → MP3 fallback.
      const mp3Like = new Uint8Array(16).fill(0xff);
      expect(parseSpeakerPayload(mp3Like)).toBeNull();
    });

    it("should accept the boundary sample rates 8000 and 96000", () => {
      expect(parseSpeakerPayload(encodeSpeakerPayload(8000, SAMPLES))!.sampleRate).toBe(8000);
      expect(parseSpeakerPayload(encodeSpeakerPayload(96000, SAMPLES))!.sampleRate).toBe(96000);
    });

    it("should copy samples out of the source buffer (no aliasing)", () => {
      const payload = encodeSpeakerPayload(22050, SAMPLES);
      const result = parseSpeakerPayload(payload);
      expect(result).not.toBeNull();
      new Uint8Array(payload).fill(0); // mutate the source afterwards
      expect(Array.from(result!.samples)).toEqual(SAMPLES);
    });
  });
});
