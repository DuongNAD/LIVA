import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";

describe("audio-worker", () => {
  let originalSelf: any;
  let originalAtob: any;
  let originalOfflineAudioContext: any;
  let postMessageMock: any;
  let decodeAudioDataMock: any;

  beforeEach(() => {
    vi.resetModules();
    originalSelf = globalThis.self;
    originalAtob = globalThis.atob;
    originalOfflineAudioContext = globalThis.OfflineAudioContext;

    postMessageMock = vi.fn();
    decodeAudioDataMock = vi.fn();

    class MockOfflineAudioContext {
      decodeAudioData = decodeAudioDataMock;
    }

    globalThis.self = {
      postMessage: postMessageMock,
    } as any;
    
    globalThis.atob = (str: string) => Buffer.from(str, 'base64').toString('binary');
    globalThis.OfflineAudioContext = MockOfflineAudioContext as any;
  });

  afterEach(() => {
    globalThis.self = originalSelf;
    globalThis.atob = originalAtob;
    globalThis.OfflineAudioContext = originalOfflineAudioContext;
  });

  it("should decode base64 audio and post message back", async () => {
    decodeAudioDataMock.mockResolvedValue({
      numberOfChannels: 1,
      length: 1000,
      sampleRate: 44100,
      getChannelData: () => new Float32Array(1000),
    });

    await import("../../src/workers/audio-worker");

    const onmessage = (globalThis.self as any).onmessage;
    expect(onmessage).toBeDefined();

    await onmessage({
      data: {
        type: "DECODE_AUDIO",
        id: "test-id",
        base64: Buffer.from("dummy audio data").toString("base64"),
      },
    });

    expect(postMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "AUDIO_READY",
        id: "test-id",
        channels: 1,
        length: 1000,
        sampleRate: 44100,
      }),
      expect.any(Object)
    );
  });

  it("should handle decode errors gracefully", async () => {
    decodeAudioDataMock.mockRejectedValue(new Error("Decode failed"));

    await import("../../src/workers/audio-worker");

    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({
      data: {
        type: "DECODE_AUDIO",
        id: "test-id-error",
        base64: Buffer.from("dummy").toString("base64"),
      },
    });

    expect(postMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "AUDIO_ERROR",
        id: "test-id-error",
        error: "Decode failed",
      })
    );
  });
});
