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

  class AudioContextMock {
    state: AudioContextState = "running";
    currentTime = 0;
    destination = {} as AudioNode;

    decodeAudioData = decodeAudioData;
    resume = vi.fn().mockResolvedValue(undefined);
    close = vi.fn().mockResolvedValue(undefined);
  }

  beforeEach(() => {
    decodeAudioData.mockClear();
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
});
