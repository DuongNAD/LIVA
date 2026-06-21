import { describe, it, expect, vi, beforeEach } from "vitest";

const { MockWorker, instances } = vi.hoisted(() => {
  const { EventEmitter } = require("node:events");
  const instances: any[] = [];
  class MockWorker extends EventEmitter {
    postMessage = vi.fn().mockImplementation(function (this: any, msg: any) {
      if (msg.type === "init") {
        // We will manually trigger events in tests
      } else if (msg.type === "generate") {
        setTimeout(() => this.emit("message", { type: "audio_result", base64: `audio_for_${msg.text}` }), 10);
      }
    });
    terminate = vi.fn().mockResolvedValue(0);
    constructor() {
      super();
      instances.push(this);
    }
  }
  return { MockWorker, instances };
});

vi.mock("node:worker_threads", () => ({
  Worker: MockWorker,
}));

import { KokoroVoiceEngine } from "../../src/services/KokoroVoiceEngine";
import { logger } from "../../src/utils/logger";

describe("KokoroVoiceEngine Robustness and Hangs", () => {
  let engine: KokoroVoiceEngine;

  beforeEach(() => {
    vi.clearAllMocks();
    instances.length = 0;
  });

  it("should handle concurrent speak calls during initialization without mixing audio or hanging (expect failure currently)", async () => {
    engine = new KokoroVoiceEngine();
    await Promise.resolve(); // Let constructor deferred microtask run

    const results: string[] = [];
    engine.on("audio_base64", (base64) => {
      results.push(base64);
    });

    // Call speak twice concurrently during initialization
    engine.speak("first");
    engine.speak("second");

    // Manually emit ready on the mock worker to trigger resolution
    const w = instances[0];
    expect(w).toBeDefined();
    w.emit("message", { type: "ready" });

    // Wait for worker to finish processing
    await new Promise((resolve) => setTimeout(resolve, 200));

    // We expect both to be processed (in a correct implementation)
    // Currently this will fail
    expect(results).toContain("audio_for_first");
    expect(results).toContain("audio_for_second");

    engine.destroy();
  });

  it("should reject init promise if worker exits during initialization (expect failure currently)", async () => {
    engine = new KokoroVoiceEngine();
    await Promise.resolve(); // Let constructor deferred microtask run
    const initPromise = engine._initPromise;

    // Get mock worker
    const w = instances[0];
    expect(w).toBeDefined();

    // Simulate worker exiting during initialization
    w.emit("exit", 1);

    // Wrap in a timeout to catch hanging
    const timeoutPromise = new Promise((_, reject) => setTimeout(() => reject(new Error("TIMED_OUT")), 100));
    
    // We expect it to reject due to the worker exit, NOT due to TIMED_OUT.
    const result = await Promise.race([
      initPromise.then(() => "resolved").catch((err) => err),
      timeoutPromise.catch((err) => err)
    ]);

    expect(result).toBeInstanceOf(Error);
    expect((result as Error).message).not.toBe("TIMED_OUT");
    
    engine.destroy();
  });

  it("should reject init promise if destroy is called during initialization (expect failure currently)", async () => {
    engine = new KokoroVoiceEngine();
    await Promise.resolve(); // Let constructor deferred microtask run
    const initPromise = engine._initPromise;

    // Call destroy while initialization is pending
    engine.destroy();

    const timeoutPromise = new Promise((_, reject) => setTimeout(() => reject(new Error("TIMED_OUT")), 100));

    // We expect it to reject due to destroy/exit, NOT due to TIMED_OUT.
    const result = await Promise.race([
      initPromise.then(() => "resolved").catch((err) => err),
      timeoutPromise.catch((err) => err)
    ]);

    expect(result).toBeInstanceOf(Error);
    expect((result as Error).message).not.toBe("TIMED_OUT");
  });

  it("should not hang and should allow recovery if worker exits during generation", async () => {
    engine = new KokoroVoiceEngine();
    await Promise.resolve();

    const w = instances[0];
    expect(w).toBeDefined();
    w.emit("message", { type: "ready" });
    await engine._initPromise;

    engine.speak("first");
    engine.speak("second");

    await new Promise((resolve) => setTimeout(resolve, 5));
    const initialInstances = instances.length;

    // Simulate worker crashing during generation of "first"
    w.emit("exit", 1);
    await new Promise((resolve) => setTimeout(resolve, 50));

    // If the queue processing recovered, a new speak call should trigger reload
    engine.speak("third");
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(instances.length).toBeGreaterThan(initialInstances);
    engine.destroy();
  });

  it("should not enter an infinite loop of processQueue if loading fails with pending queue", async () => {
    engine = new KokoroVoiceEngine();
    await Promise.resolve();

    const w = instances[0];
    expect(w).toBeDefined();
    w.emit("message", { type: "ready" });
    await engine._initPromise;

    // Stub ensureLoaded to fail
    (engine as any).ensureLoaded = async () => false;

    // Queue some messages
    engine.speak("one");
    engine.speak("two");

    await new Promise((resolve) => setTimeout(resolve, 100));
    
    // Ensure instances count doesn't explode (no infinite reload loop)
    expect(instances.length).toBeLessThan(5);
    engine.destroy();
  });

  it("should terminate the worker and clear ready state on worker initialization error", async () => {
    engine = new KokoroVoiceEngine();
    await Promise.resolve(); // Let constructor deferred microtask run

    const w = instances[0];
    expect(w).toBeDefined();

    // Emit initialization error message
    w.emit("message", { type: "error", message: "Failed to load model" });

    // Expect the init promise to be rejected
    await expect(engine._initPromise).rejects.toThrow("Failed to load model");

    // Expect w.terminate to have been called
    expect(w.terminate).toHaveBeenCalled();

    engine.destroy();
  });

  it("should handle delayed exit/error event race on old/defunct worker without overwriting active worker state", async () => {
    engine = new KokoroVoiceEngine();
    await Promise.resolve();

    const w1 = instances[0];
    expect(w1).toBeDefined();

    // Now trigger a reload or spawn a new worker (e.g. by simulating w1 exiting)
    w1.emit("exit", 1);
    await expect(engine._initPromise).rejects.toThrow("Kokoro worker exited during initialization");

    // Now reload model (calling speak triggers reload internal ensureLoaded)
    engine.speak("hello");
    await Promise.resolve(); // Let speak run ensureLoaded and spawn a new worker w2
    
    const w2 = instances[1];
    expect(w2).toBeDefined();
    expect(w2).not.toBe(w1);
    
    // Now trigger ready on w2 to make the active worker ready
    w2.emit("message", { type: "ready" });
    
    // Now simulate a delayed exit/error event on the old w1
    w1.emit("exit", 0);
    w1.emit("error", new Error("late error"));
    
    // The active worker should still be w2 and should still be ready!
    // If w1 event was not guarded, it would set this.#worker to null and #isReady to false.
    const results: string[] = [];
    engine.on("audio_base64", (base64) => {
      results.push(base64);
    });
    
    // Wait for generate result
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(results).toContain("audio_for_hello");

    engine.destroy();
  });

  it("should reschedule idle timer when the queue becomes empty and terminate the worker after inactivity", async () => {
    vi.useFakeTimers();
    engine = new KokoroVoiceEngine();
    await Promise.resolve();

    const w = instances[0];
    expect(w).toBeDefined();
    w.emit("message", { type: "ready" });
    await engine._initPromise;

    engine.speak("hello");
    // Wait for the mock generate's setTimeout to run
    await vi.advanceTimersByTimeAsync(20);

    // Now the queue is empty. Since it touched the idle timer in finally,
    // the idle timer is scheduled for 5 minutes from now.
    // If we advance by 4 minutes, the worker should not be terminated yet.
    await vi.advanceTimersByTimeAsync(4 * 60 * 1000);
    expect(w.terminate).not.toHaveBeenCalled();

    // If we advance by another 1 minute (total 5 minutes since empty queue), the worker should be terminated.
    await vi.advanceTimersByTimeAsync(1 * 60 * 1000 + 100);
    expect(w.terminate).toHaveBeenCalled();

    engine.destroy();
    vi.useRealTimers();
  });

  it("should reject in-flight generation promise with Preempted error when preempt is called", async () => {
    const errorSpy = vi.spyOn(logger, "error");
    engine = new KokoroVoiceEngine();
    await Promise.resolve();

    const w = instances[0];
    expect(w).toBeDefined();
    w.emit("message", { type: "ready" });
    await engine._initPromise;

    // Start a generation
    engine.speak("long sentence");
    await new Promise((resolve) => setTimeout(resolve, 2)); // let processQueue start the promise

    // Preempt before worker completes (which takes 10ms in mock)
    engine.preempt();

    // Wait for event loop to propagate rejection
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(errorSpy).toHaveBeenCalledWith(
      expect.stringContaining("Generation failed for \"long sentence...\": Preempted")
    );

    engine.destroy();
  });
});
