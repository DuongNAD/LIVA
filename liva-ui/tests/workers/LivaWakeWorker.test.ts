import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";

describe("LivaWakeWorker", () => {
  let originalSelf: any;
  let postMessageMock: any;
  let closeMock: any;

  beforeEach(() => {
    vi.resetModules();
    originalSelf = globalThis.self;

    postMessageMock = vi.fn();
    closeMock = vi.fn();

    globalThis.self = {
      postMessage: postMessageMock,
      close: closeMock,
    } as any;
  });

  afterEach(() => {
    globalThis.self = originalSelf;
  });

  it("should initialize on load and handle init message", async () => {
    await import("../../src/workers/LivaWakeWorker");

    expect(postMessageMock).toHaveBeenCalledWith({ type: 'loaded' });

    const onmessage = (globalThis.self as any).onmessage;
    expect(onmessage).toBeDefined();

    await onmessage({
      data: {
        type: "init",
        data: {
          config: { threshold: 0.2 },
        },
      },
    });

    expect(postMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'ready', success: true })
    );
  });

  it("should process audio frames and detect wake word", async () => {
    await import("../../src/workers/LivaWakeWorker");
    const onmessage = (globalThis.self as any).onmessage;

    await onmessage({ data: { type: "init" } });

    const silentAudio = new Float32Array(8000).fill(0);
    await onmessage({
      data: {
        type: "audio",
        data: { audio: silentAudio.buffer },
      },
    });

    const loudFeatures = new Float32Array(16).fill(1.0);
    await onmessage({
      data: {
        type: "features",
        data: { features: loudFeatures.buffer },
      },
    });

    expect(postMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'detection', detected: true })
    );
  });

  it("should handle pause, resume, reset, setThreshold, terminate", async () => {
    await import("../../src/workers/LivaWakeWorker");
    const onmessage = (globalThis.self as any).onmessage;

    await onmessage({ data: { type: "pause" } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'paused' });

    await onmessage({ data: { type: "resume" } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'resumed' });

    await onmessage({ data: { type: "reset" } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'reset' });

    await onmessage({
      data: {
        type: "setThreshold",
        data: { threshold: 0.5 },
      },
    });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'thresholdChanged', threshold: 0.5 });

    await onmessage({ data: { type: "terminate" } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'terminated' });
    expect(closeMock).toHaveBeenCalled();
  });
});
