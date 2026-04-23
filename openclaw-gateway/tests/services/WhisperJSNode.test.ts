import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock @huggingface/transformers (requires ONNX runtime)
vi.mock("@huggingface/transformers", () => ({
  pipeline: vi.fn().mockResolvedValue(
    vi.fn().mockResolvedValue({ text: "xin chào thế giới" })
  ),
}));

import { WhisperJSNode } from "../../src/services/WhisperJSNode";

describe("WhisperJSNode", () => {
  let stt: WhisperJSNode;

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should instantiate without throwing", () => {
    expect(() => {
      stt = new WhisperJSNode();
    }).not.toThrow();
    stt.destroy();
  });

  it("should expose identical API surface to WhisperNode", () => {
    stt = new WhisperJSNode();

    expect(typeof stt.pushAudioChunk).toBe("function");
    expect(typeof stt.flush).toBe("function");
    expect(typeof stt.on).toBe("function");
    expect(typeof stt.emit).toBe("function");

    stt.destroy();
  });

  it("should buffer audio chunks", () => {
    stt = new WhisperJSNode();

    const chunk = Buffer.alloc(1024);
    stt.pushAudioChunk(chunk);

    expect((stt as any).audioBuffer.length).toBe(1);

    stt.pushAudioChunk(chunk);
    expect((stt as any).audioBuffer.length).toBe(2);

    stt.destroy();
  });

  it("should flush buffer on preempt", () => {
    stt = new WhisperJSNode();

    const chunk = Buffer.alloc(1024);
    stt.pushAudioChunk(chunk);
    stt.pushAudioChunk(chunk);
    expect((stt as any).audioBuffer.length).toBe(2);

    stt.flush();

    expect((stt as any).audioBuffer.length).toBe(0);
    expect((stt as any).isProcessing).toBe(false);

    stt.destroy();
  });

  it("should handle destroy without crash", () => {
    stt = new WhisperJSNode();

    expect(() => {
      stt.destroy();
      stt.destroy(); // Double destroy should be safe
    }).not.toThrow();
  });

  it("should not accept audio after destroy", () => {
    stt = new WhisperJSNode();
    stt.destroy();

    expect(() => {
      stt.pushAudioChunk(Buffer.alloc(512));
    }).not.toThrow();

    expect((stt as any).audioBuffer.length).toBe(0);
  });

  it("should have correct VAD silence threshold", () => {
    stt = new WhisperJSNode();
    expect((stt as any).VAD_SILENCE_MS).toBe(800);
    stt.destroy();
  });

  it("should encode WAV correctly", () => {
    stt = new WhisperJSNode();

    const samples = new Float32Array([0, 0.5, -0.5, 1, -1]);
    const wav = (stt as any).encodeWAV(samples, 16000);

    // Check WAV header magic
    expect(wav.toString("ascii", 0, 4)).toBe("RIFF");
    expect(wav.toString("ascii", 8, 12)).toBe("WAVE");
    expect(wav.toString("ascii", 12, 16)).toBe("fmt ");
    expect(wav.toString("ascii", 36, 40)).toBe("data");

    // Check data size: 5 samples * 2 bytes each = 10
    expect(wav.readUInt32LE(40)).toBe(10);

    // Total buffer: 44 header + 10 data = 54
    expect(wav.length).toBe(54);

    stt.destroy();
  });
});
