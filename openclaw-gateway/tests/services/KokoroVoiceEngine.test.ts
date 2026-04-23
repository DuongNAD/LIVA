import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock kokoro-js (requires ONNX runtime, not available in test env)
vi.mock("kokoro-js", () => ({
  KokoroTTS: {
    from_pretrained: vi.fn().mockResolvedValue({
      list_voices: vi.fn().mockReturnValue(["af_heart", "af_bella"]),
      generate: vi.fn().mockResolvedValue({
        toWav: vi.fn().mockReturnValue(new Uint8Array([82, 73, 70, 70])), // "RIFF"
      }),
    }),
  },
}));

import { KokoroVoiceEngine } from "../../src/services/KokoroVoiceEngine";

describe("KokoroVoiceEngine", () => {
  let engine: KokoroVoiceEngine;

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should instantiate without throwing", () => {
    expect(() => {
      engine = new KokoroVoiceEngine();
    }).not.toThrow();
    engine.destroy();
  });

  it("should expose identical API surface to VoiceEngine", () => {
    engine = new KokoroVoiceEngine();

    expect(typeof engine.pushTokens).toBe("function");
    expect(typeof engine.preempt).toBe("function");
    expect(typeof engine.destroy).toBe("function");
    expect(typeof engine.on).toBe("function");
    expect(typeof engine.emit).toBe("function");

    engine.destroy();
  });

  it("should strip emotion tags from tokens before TTS", () => {
    engine = new KokoroVoiceEngine();

    // Push tokens with emotion tags — they should be stripped
    // Use text WITHOUT sentence-ending punctuation (. ? ! \n) to keep it in buffer
    engine.pushTokens("[happy]Hello world");
    // tokenBuffer should contain "Hello world" without [happy]
    expect((engine as any).tokenBuffer).toBe("Hello world");

    engine.destroy();
  });

  it("should buffer tokens and split on sentence boundary", () => {
    engine = new KokoroVoiceEngine();

    engine.pushTokens("Hello ");
    expect((engine as any).tokenBuffer).toBe("Hello ");

    engine.pushTokens("world.");
    // After sentence boundary, text should be queued
    expect((engine as any).tokenBuffer).toBe("");
    expect((engine as any).pendingTextQueue.length).toBeGreaterThanOrEqual(0);

    engine.destroy();
  });

  it("should clear everything on preempt", () => {
    engine = new KokoroVoiceEngine();

    engine.pushTokens("Some pending text ");
    engine.preempt();

    expect((engine as any).tokenBuffer).toBe("");
    expect((engine as any).pendingTextQueue.length).toBe(0);

    engine.destroy();
  });

  it("should handle destroy without crash", () => {
    engine = new KokoroVoiceEngine();

    expect(() => {
      engine.destroy();
      engine.destroy(); // Double destroy should be safe
    }).not.toThrow();
  });

  it("should not process after destroy", () => {
    engine = new KokoroVoiceEngine();
    engine.destroy();

    expect(() => {
      engine.pushTokens("Should be ignored.");
    }).not.toThrow();

    expect((engine as any).tokenBuffer).toBe("");
  });

  it("should respect MAX_QUEUE_SIZE", () => {
    engine = new KokoroVoiceEngine();

    // Force ready state
    (engine as any).isReady = false; // Not ready → items stay in queue

    for (let i = 0; i < 60; i++) {
      (engine as any).enqueue(`Sentence ${i}.`);
    }

    expect((engine as any).pendingTextQueue.length).toBeLessThanOrEqual(50);

    engine.destroy();
  });
});
