import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("vue", () => ({
  onUnmounted: vi.fn(),
}));

vi.mock("../../src/utils/logger", () => ({
  logger: {
    warn: vi.fn(),
  },
}));

import { useSpeakerPlayback } from "../../src/composables/useSpeakerPlayback";

describe("useSpeakerPlayback", () => {
  const decodeAudioData = vi.fn().mockRejectedValue(new Error("not encoded audio"));
  const sources: AudioBufferSourceNodeMock[] = [];

  class AudioBufferSourceNodeMock {
    buffer: AudioBuffer | null = null;
    onended: (() => void) | null = null;
    connect = vi.fn();
    start = vi.fn();
    stop = vi.fn();
  }

  class AudioContextMock {
    state: AudioContextState = "running";
    currentTime = 0;
    destination = {} as AudioNode;

    decodeAudioData = decodeAudioData;
    resume = vi.fn().mockResolvedValue(undefined);
    close = vi.fn().mockResolvedValue(undefined);
    createGain = vi.fn(() => ({
      connect: vi.fn(),
      context: this,
      gain: { value: 1, setTargetAtTime: vi.fn() },
    }));
    createBuffer = vi.fn((_channels: number, length: number, sampleRate: number) => ({
      duration: length / sampleRate,
      copyToChannel: vi.fn(),
    }));
    createBufferSource = vi.fn(() => {
      const source = new AudioBufferSourceNodeMock();
      sources.push(source);
      return source;
    });
  }

  beforeEach(() => {
    decodeAudioData.mockClear();
    sources.length = 0;
    vi.stubGlobal("AudioContext", AudioContextMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("drops malformed OP_SPEAKER_OUT instead of decoding it as MP3", async () => {
    const speaker = useSpeakerPlayback();

    await speaker.enqueueSpeakerPayload(new Uint8Array([0xff, 0xfb, 0x90, 0x64]));

    expect(decodeAudioData).not.toHaveBeenCalled();
    expect(speaker.getContext()).toBeNull();
  });

  it("schedules PCM, drains callbacks, flushes, unblocks, and closes", async () => {
    const onPlaybackStarted = vi.fn();
    const onPlaybackFinished = vi.fn();
    const onQueueDrained = vi.fn();
    const onSourceStarted = vi.fn();
    const speaker = useSpeakerPlayback({
      useMasterGain: true,
      onPlaybackStarted,
      onPlaybackFinished,
      onQueueDrained,
      onSourceStarted,
    });
    const payload = new Uint8Array(12);
    const view = new DataView(payload.buffer);
    view.setUint32(0, 7, true);
    view.setUint32(4, 16000, true);
    view.setFloat32(8, 0.5, true);

    await speaker.enqueueSpeakerPayload(payload);
    expect(speaker.isPlaying()).toBe(true);
    expect(speaker.hasActiveSources()).toBe(true);
    expect(onPlaybackStarted).toHaveBeenCalledOnce();
    expect(onSourceStarted).toHaveBeenCalledOnce();

    speaker.setMasterVolume(0.2);
    sources[0].onended?.();
    expect(speaker.isPlaying()).toBe(false);
    expect(onPlaybackFinished).toHaveBeenCalledOnce();
    expect(onQueueDrained).toHaveBeenCalledOnce();

    await speaker.enqueueSpeakerPayload(payload);
    speaker.flush();
    expect(speaker.isBlocked()).toBe(false);
    expect(sources[1].stop).toHaveBeenCalledOnce();
    speaker.stop();
    expect(speaker.isBlocked()).toBe(true);
    speaker.unblock();
    expect(speaker.isBlocked()).toBe(false);
    speaker.close();
    expect(speaker.getContext()).toBeNull();
  });

  it("schedules decoded audio and drops a decode invalidated by flush", async () => {
    const decoded = { duration: 1 } as AudioBuffer;
    decodeAudioData.mockResolvedValueOnce(decoded);
    const speaker = useSpeakerPlayback();
    await speaker.enqueueEncodedAudio(new ArrayBuffer(4));
    expect(sources).toHaveLength(1);

    let resolveDecode!: (buffer: AudioBuffer) => void;
    decodeAudioData.mockReturnValueOnce(new Promise((resolve) => { resolveDecode = resolve; }));
    const pending = speaker.enqueueEncodedAudio(new ArrayBuffer(8));
    speaker.flush();
    resolveDecode(decoded);
    await pending;
    expect(sources).toHaveLength(1);
  });
});
