import { describe, it, expect, vi, beforeEach } from "vitest";

const { MockWorker } = vi.hoisted(() => {
  const { EventEmitter } = require("node:events");
  class MockWorker extends EventEmitter {
    postMessage = vi.fn().mockImplementation(function (this: any, msg: any) {
      if (msg.type === "init") {
        setTimeout(() => this.emit("message", { type: "ready" }), 5);
      } else if (msg.type === "generate") {
        setTimeout(() => this.emit("message", { type: "audio_result", base64: "UkkmRg==" }), 5);
      }
    });
  }
  return { MockWorker };
});

vi.mock("node:worker_threads", () => ({
  Worker: MockWorker,
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

  it("should expose IVoiceEngine API surface", () => {
    engine = new KokoroVoiceEngine();

    expect(typeof engine.pushTokens).toBe("function");
    expect(typeof engine.flushTTS).toBe("function");
    expect(typeof engine.preempt).toBe("function");
    expect(typeof engine.speak).toBe("function");
    expect(typeof engine.destroy).toBe("function");
    expect(typeof engine.on).toBe("function");
    expect(typeof engine.emit).toBe("function");

    engine.destroy();
  });

  it("should not throw when pushTokens called with emotion tags", () => {
    engine = new KokoroVoiceEngine();

    // Push tokens with emotion tags — they should be stripped internally
    expect(() => {
      engine.pushTokens("[happy]Hello world");
    }).not.toThrow();

    engine.destroy();
  });

  it("should not throw when pushing tokens that form a sentence", () => {
    engine = new KokoroVoiceEngine();

    expect(() => {
      engine.pushTokens("Hello ");
      engine.pushTokens("world.");
    }).not.toThrow();

    engine.destroy();
  });

  it("should clear everything on preempt without throwing", () => {
    engine = new KokoroVoiceEngine();

    engine.pushTokens("Some pending text ");
    expect(() => {
      engine.preempt();
    }).not.toThrow();

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
  });

  it("should handle flushTTS without throwing", () => {
    engine = new KokoroVoiceEngine();

    engine.pushTokens("Incomplete sentence without ending");
    expect(() => {
      engine.flushTTS();
    }).not.toThrow();

    engine.destroy();
  });
});
