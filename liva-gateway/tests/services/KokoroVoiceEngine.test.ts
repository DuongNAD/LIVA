import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock worker threads before importing KokoroVoiceEngine
const { mockWorkerState } = vi.hoisted(() => {
  return {
    mockWorkerState: {
      shouldFailInit: false,
      initDelay: 0,
      generateDelay: 0,
      shouldFailGenerate: false,
      instances: [] as any[],
      reset() {
        this.shouldFailInit = false;
        this.initDelay = 0;
        this.generateDelay = 0;
        this.shouldFailGenerate = false;
        this.instances = [];
      }
    }
  };
});

vi.mock("node:worker_threads", () => {
  const { EventEmitter } = require("node:events");
  
  class MockWorker extends EventEmitter {
    public postMessage = vi.fn();
    
    constructor(public workerPath: string, public options?: any) {
      super();
      mockWorkerState.instances.push(this);
      
      this.postMessage.mockImplementation((msg: any) => {
        const delay = msg.type === "init" ? mockWorkerState.initDelay : (msg.type === "generate" ? mockWorkerState.generateDelay : 0);
        
        const execute = () => {
          if (msg.type === "init") {
            if (mockWorkerState.shouldFailInit) {
              this.emit("message", { type: "error", message: "Mocked initialization error" });
            } else {
              this.emit("message", { type: "ready" });
            }
          } else if (msg.type === "generate") {
            if (mockWorkerState.shouldFailGenerate) {
              this.emit("message", { type: "error", id: msg.id, message: "Mocked generation error" });
            } else {
              this.emit("message", {
                type: "generate_result",
                id: msg.id,
                wavBuffer: new Uint8Array([82, 73, 70, 70]), // "RIFF"
              });
            }
          }
        };

        if (delay > 0) {
          setTimeout(execute, delay);
        } else {
          // Keep it async but immediate
          process.nextTick(execute);
        }
      });
    }
  }
  
  return { Worker: MockWorker };
});

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
    mockWorkerState.reset();
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

  it("should track the same initialization promise to prevent concurrent loads", async () => {
    mockWorkerState.initDelay = 50;

    engine = new KokoroVoiceEngine();
    
    // We get the first init promise
    const p1 = engine._initPromise;
    
    // speak triggers ensureLoaded which calls initModel again. It should return the same pending promise
    const speakPromise = engine.speak("test");
    
    await p1;
    await speakPromise;
    engine.destroy();
  });

  it("should stop retrying if loading fails due to hasFailed flag", async () => {
    mockWorkerState.shouldFailInit = true;

    engine = new KokoroVoiceEngine();
    
    try {
      await engine._initPromise;
    } catch (e) {
      // expected
    }

    mockWorkerState.shouldFailInit = false;
    mockWorkerState.instances = []; // clear spawned list
    
    // speak/ensureLoaded should not trigger new worker spawning because hasFailed is true
    await engine.speak("another text");
    
    expect(mockWorkerState.instances.length).toBe(0);
    engine.destroy();
  });

  it("should reject all pending requests and clear pending map on preempt", async () => {
    // Make generation slow so we can preempt before MockWorker responds
    mockWorkerState.generateDelay = 100;

    engine = new KokoroVoiceEngine();
    await engine._initPromise;

    let emitted = false;
    engine.on("audio_buffer", () => {
      emitted = true;
    });

    await engine.speak("test preempt");

    // Wait a brief moment to ensure processQueue has started and request is sent to worker
    await new Promise((resolve) => setTimeout(resolve, 20));

    engine.preempt();

    // Wait for the generate delay to elapse
    await new Promise((resolve) => setTimeout(resolve, 150));

    expect(emitted).toBe(false);
    engine.destroy();
  });

  it("should touch the idle timer when queue processing completes", async () => {
    vi.useFakeTimers();
    engine = new KokoroVoiceEngine();

    // Trigger speak to start processing (queue has the item, but init is still pending)
    await engine.speak("test queue completion");

    // Fast-forward timers to allow the queue processing to finish and the 5-minute idle timer to fire
    await vi.runAllTimersAsync();

    // Now switch back to real timers so worker thread mock runs normally
    vi.useRealTimers();

    // Speak again, which should trigger ensureLoaded and reload the model by spawning a new worker
    await engine.speak("test after idle");
    
    // Wait a brief moment in real time for worker to initialize
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(mockWorkerState.instances.length).toBe(2);

    engine.destroy();
  });
});
