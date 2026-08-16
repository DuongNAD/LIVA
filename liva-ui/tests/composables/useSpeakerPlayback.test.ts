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
  const gains: GainNodeMock[] = [];
  const analysers: AnalyserNodeMock[] = [];

  class AudioBufferSourceNodeMock {
    buffer: AudioBuffer | null = null;
    onended: (() => void) | null = null;
    connect = vi.fn();
    start = vi.fn();
    stop = vi.fn();
  }

  interface GainNodeMock {
    connect: ReturnType<typeof vi.fn>;
    context: unknown;
    gain: {
      value: number;
      setTargetAtTime: ReturnType<typeof vi.fn>;
      setValueAtTime: ReturnType<typeof vi.fn>;
      linearRampToValueAtTime: ReturnType<typeof vi.fn>;
    };
  }

  interface AnalyserNodeMock {
    connect: ReturnType<typeof vi.fn>;
    disconnect: ReturnType<typeof vi.fn>;
    fftSize: number;
    smoothingTimeConstant: number;
    frequencyBinCount: number;
    getByteFrequencyData: ReturnType<typeof vi.fn>;
  }

  class AudioContextMock {
    state: AudioContextState = "running";
    currentTime = 0;
    destination = {} as AudioNode;

    decodeAudioData = decodeAudioData;
    resume = vi.fn().mockResolvedValue(undefined);
    close = vi.fn().mockResolvedValue(undefined);
    createAnalyser = vi.fn(() => {
      const analyser: AnalyserNodeMock = {
        connect: vi.fn(),
        disconnect: vi.fn(),
        fftSize: 2048,
        smoothingTimeConstant: 0.8,
        frequencyBinCount: 1024,
        getByteFrequencyData: vi.fn(),
      };
      analysers.push(analyser);
      return analyser;
    });
    createGain = vi.fn(() => {
      const gain: GainNodeMock = {
        connect: vi.fn(),
        context: this,
        gain: {
          value: 1,
          setTargetAtTime: vi.fn(),
          setValueAtTime: vi.fn(),
          linearRampToValueAtTime: vi.fn(),
        },
      };
      gains.push(gain);
      return gain;
    });
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
    gains.length = 0;
    analysers.length = 0;
    vi.stubGlobal("AudioContext", AudioContextMock);
  });

  /** Một chunk PCM hợp lệ: [turn_epoch u32][sample_rate u32][1 mẫu f32]. */
  function pcmChunk(): Uint8Array {
    const payload = new Uint8Array(12);
    const view = new DataView(payload.buffer);
    view.setUint32(0, 7, true);
    view.setUint32(4, 16000, true);
    view.setFloat32(8, 0.5, true);
    return payload;
  }

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

  it("applies 15ms linear gain rampdown before stopping audio during barge-in / stop", async () => {
    const speaker = useSpeakerPlayback({ useMasterGain: true });
    await speaker.enqueueSpeakerPayload(pcmChunk());

    expect(sources).toHaveLength(1);
    expect(gains).toHaveLength(1);

    const masterGain = gains[0];
    speaker.stop();

    expect(masterGain.gain.setValueAtTime).toHaveBeenCalledWith(1, 0);
    expect(masterGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(0, 0.015);
    expect(sources[0].stop).toHaveBeenCalledWith(0.015);
  });

  // ── U24 · lỗi 1: nguồn âm từng mắc song song ─────────────────────────────
  // Bản cũ để lip-sync tự nối `source → analyser → destination` BÊN CẠNH
  // `source → masterGain → destination`. Cùng một buffer về đích hai đường:
  // tiếng to gấp đôi, và `setMasterVolume()` chỉ hạ được một nhánh nên
  // audio_ducking không bao giờ hạ hết. Test này khoá đồ thị về MỘT đường.
  it("nối analyser TRONG chuỗi ra, không mắc song song với masterGain", async () => {
    const speaker = useSpeakerPlayback({ useMasterGain: true, enableAnalyser: true });

    await speaker.enqueueSpeakerPayload(pcmChunk());

    expect(analysers).toHaveLength(1);
    expect(gains).toHaveLength(1);

    // source → analyser (đúng một đích, và KHÔNG phải destination/gain)
    expect(sources[0].connect).toHaveBeenCalledTimes(1);
    expect(sources[0].connect).toHaveBeenCalledWith(analysers[0]);

    // analyser → masterGain → destination
    expect(analysers[0].connect).toHaveBeenCalledTimes(1);
    expect(analysers[0].connect).toHaveBeenCalledWith(gains[0]);
    expect(gains[0].connect).toHaveBeenCalledTimes(1);
    expect(gains[0].connect).toHaveBeenCalledWith(speaker.getContext()!.destination);

    // Hệ quả phải giữ được: mọi đường tới loa đều đi qua masterGain, nên
    // setMasterVolume() hạ được TOÀN BỘ tiếng chứ không còn nhánh lọt.
    const destination = speaker.getContext()!.destination;
    expect(sources[0].connect).not.toHaveBeenCalledWith(destination);
    expect(analysers[0].connect).not.toHaveBeenCalledWith(destination);
  });

  // ── U24 · lỗi 2: analyser từng bị dựng lại mỗi chunk ──────────────────────
  // Chunk được xếp lịch TRƯỚC khi kêu (nextStartTime nằm ở tương lai), nên một
  // analyser dựng lại theo từng chunk sẽ đọc nguồn còn im và làm miệng đóng
  // giữa lượt nói. Analyser phải sống theo AudioContext, không theo chunk.
  it("giữ đúng MỘT analyser qua nhiều chunk, không tháo giữa chừng", async () => {
    const speaker = useSpeakerPlayback({ useMasterGain: true, enableAnalyser: true });

    await speaker.enqueueSpeakerPayload(pcmChunk());
    await speaker.enqueueSpeakerPayload(pcmChunk());
    await speaker.enqueueSpeakerPayload(pcmChunk());

    expect(sources).toHaveLength(3);
    expect(analysers).toHaveLength(1);
    expect(analysers[0].disconnect).not.toHaveBeenCalled();
    expect(speaker.getAnalyser()).toBe(analysers[0]);

    // Cả ba chunk đổ vào cùng một analyser.
    for (const source of sources) {
      expect(source.connect).toHaveBeenCalledWith(analysers[0]);
    }

    // Barge-in dừng nguồn nhưng KHÔNG được phá analyser — phá là đứt tiếng cho
    // lượt sau, vì analyser nay nằm trong chuỗi chứ không còn là nhánh phụ.
    speaker.flush();
    expect(analysers[0].disconnect).not.toHaveBeenCalled();
    expect(speaker.getAnalyser()).toBe(analysers[0]);
  });

  it("không dựng analyser khi enableAnalyser tắt (đường App.vue)", async () => {
    const speaker = useSpeakerPlayback({ useMasterGain: true });

    await speaker.enqueueSpeakerPayload(pcmChunk());

    expect(analysers).toHaveLength(0);
    expect(speaker.getAnalyser()).toBeNull();
    expect(sources[0].connect).toHaveBeenCalledWith(gains[0]);
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
