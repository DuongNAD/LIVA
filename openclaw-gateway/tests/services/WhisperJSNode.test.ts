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

  it("should discard audio when worker is not ready", () => {
    stt = new WhisperJSNode();

    // Worker is not ready by default (no "ready" message received)
    const chunk = Buffer.alloc(2048);
    stt.pushAudioChunk(chunk);
    stt.pushAudioChunk(chunk);
    stt.pushAudioChunk(chunk);

    // Manually trigger processAudio (bypass silence timer)
    (stt as any).processAudio();

    // Buffer should be cleared (discarded because worker not ready)
    expect((stt as any).audioBuffer.length).toBe(0);

    stt.destroy();
  });

  it("should send audio to worker via postMessage when ready", () => {
    stt = new WhisperJSNode();

    // Simulate worker being ready
    (stt as any).isReady = true;
    const mockWorker = { postMessage: vi.fn(), terminate: vi.fn(), on: vi.fn() };
    (stt as any).worker = mockWorker;

    // Push enough data to exceed 4096 byte threshold
    const chunk = Buffer.alloc(2048);
    stt.pushAudioChunk(chunk);
    stt.pushAudioChunk(chunk);
    stt.pushAudioChunk(chunk); // 6144 bytes total > 4096

    // Manually trigger processAudio (bypass silence timer)
    (stt as any).processAudio();

    // Worker should have received a postMessage with "process" type
    expect(mockWorker.postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: "process" }),
      expect.any(Array) // transferList
    );

    stt.destroy();
  });

  it("should skip sending to worker if buffer too small", () => {
    stt = new WhisperJSNode();

    // Simulate worker being ready
    (stt as any).isReady = true;
    const mockWorker = { postMessage: vi.fn(), terminate: vi.fn(), on: vi.fn() };
    (stt as any).worker = mockWorker;

    // Push data below 4096 byte threshold
    const chunk = Buffer.alloc(512);
    stt.pushAudioChunk(chunk);

    // Manually trigger processAudio (bypass silence timer)
    (stt as any).processAudio();

    // Worker should NOT have received postMessage (buffer too small)
    expect(mockWorker.postMessage).not.toHaveBeenCalled();

    stt.destroy();
  });
});
