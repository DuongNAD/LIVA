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

describe("useSpeakerPlayback — Adversarial Empirical Challenge Suite (M1_2)", () => {
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

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  /** Helper to generate a valid PCM chunk with specified sample length. */
  function createPcmChunk(sampleCount = 1600, sampleRate = 16000): Uint8Array {
    // 8 bytes header (turn_epoch u32, sample_rate u32) + 4 * sampleCount bytes
    const payload = new Uint8Array(8 + sampleCount * 4);
    const view = new DataView(payload.buffer);
    view.setUint32(0, 1, true); // epoch = 1
    view.setUint32(4, sampleRate, true);
    for (let i = 0; i < sampleCount; i++) {
      view.setFloat32(8 + i * 4, 0.25, true);
    }
    return payload;
  }

  // ──────────────────────────────────────────────────────────────────────────
  // 1. Inter-clause Gap Empirical Challenge (50ms, 150ms, 300ms vs 400ms)
  // ──────────────────────────────────────────────────────────────────────────

  const simulatedGaps = [
    { gapMs: 50, shouldHoldPlaying: true, description: "50ms tight gap" },
    { gapMs: 150, shouldHoldPlaying: true, description: "150ms typical clause gap" },
    { gapMs: 300, shouldHoldPlaying: true, description: "300ms long clause gap (< 350ms)" },
    { gapMs: 400, shouldHoldPlaying: false, description: "400ms gap (> 350ms grace timeout)" },
  ];

  function advanceTime(speaker: ReturnType<typeof useSpeakerPlayback>, ms: number): void {
    const ctx = speaker.getContext() as unknown as AudioContextMock;
    if (ctx) {
      ctx.currentTime += ms / 1000;
    }
    vi.advanceTimersByTime(ms);
  }

  for (const { gapMs, shouldHoldPlaying, description } of simulatedGaps) {
    it(`empirically verifies ${description}: holds playing=${shouldHoldPlaying} and pre-roll suppression`, async () => {
      vi.useFakeTimers();
      try {
        const onPlaybackStarted = vi.fn();
        const onPlaybackFinished = vi.fn();
        const onQueueDrained = vi.fn();
        const scheduledEvents: { startTimeSec: number; durationSec: number }[] = [];

        const speaker = useSpeakerPlayback({
          channel: `[Test-${gapMs}]`,
          preRollBufferSec: 0.04, // 40ms pre-roll
          graceTimeoutMs: 350,   // 350ms grace period
          onPlaybackStarted,
          onPlaybackFinished,
          onQueueDrained,
          onChunkScheduled: (info) => scheduledEvents.push(info),
        });

        // 1. Enqueue First Clause (100ms duration: 1600 samples @ 16kHz)
        const chunk1 = createPcmChunk(1600, 16000); // duration = 0.1s
        await speaker.enqueueSpeakerPayload(chunk1);

        expect(speaker.isPlaying()).toBe(true);
        expect(onPlaybackStarted).toHaveBeenCalledTimes(1);
        expect(sources).toHaveLength(1);
        // First chunk incurs 40ms pre-roll (currentTime 0 + 0.04)
        expect(sources[0].start).toHaveBeenCalledWith(0.04);
        expect(scheduledEvents[0].startTimeSec).toBeCloseTo(0.04);

        // Advance fake time to when Chunk 1 finishes playing (40ms pre-roll + 100ms audio = 140ms)
        advanceTime(speaker, 140);
        sources[0].onended?.();

        // Immediately after Chunk 1 ends: queue drained, but grace period is active
        expect(speaker.hasActiveSources()).toBe(false);

        if (shouldHoldPlaying) {
          // Advance by simulated gap (< 350ms)
          advanceTime(speaker, gapMs);

          // During grace period: playing state MUST NOT drop
          expect(speaker.isPlaying()).toBe(true);
          expect(onPlaybackFinished).not.toHaveBeenCalled();

          // Enqueue Second Clause (100ms duration)
          const chunk2 = createPcmChunk(1600, 16000);
          await speaker.enqueueSpeakerPayload(chunk2);

          // VERIFY: Playing state remains continuously true
          expect(speaker.isPlaying()).toBe(true);
          // onPlaybackStarted must NOT have been called again (continuous run)
          expect(onPlaybackStarted).toHaveBeenCalledTimes(1);
          expect(sources).toHaveLength(2);

          // VERIFY: Chunk 2 starts gaplessly without redundant 40ms pre-roll!
          // Current time is 0.14 + gapMs/1000.
          // Because playing is true, preRoll is 0.
          // Chunk 2 starts immediately at ctx.currentTime (0ms additional pre-roll delay!)
          const expectedStartTime = 0.14 + gapMs / 1000;
          expect(sources[1].start).toHaveBeenCalledWith(expectedStartTime);
          expect(scheduledEvents[1].startTimeSec).toBeCloseTo(expectedStartTime);
        } else {
          // For gapMs = 400ms (> 350ms):
          // Advance to 350ms: grace timeout expires
          advanceTime(speaker, 350);

          // VERIFY: Grace timeout triggers transition to playing=false
          expect(speaker.isPlaying()).toBe(false);
          expect(onPlaybackFinished).toHaveBeenCalledTimes(1);
          expect(onQueueDrained).toHaveBeenCalledTimes(1);

          // Advance remaining 50ms (total 400ms gap)
          advanceTime(speaker, gapMs - 350);

          // Enqueue Second Clause after expired grace period
          const chunk2 = createPcmChunk(1600, 16000);
          await speaker.enqueueSpeakerPayload(chunk2);

          // VERIFY: Treated as new run: playing state becomes true again
          expect(speaker.isPlaying()).toBe(true);
          expect(onPlaybackStarted).toHaveBeenCalledTimes(2);
          expect(sources).toHaveLength(2);

          // Chunk 2 MUST trigger a new 40ms pre-roll buffer
          // currentTime = 0.14 + 0.40 = 0.54s.
          // With pre-roll = 0.04s, nextStartTime = 0.54 + 0.04 = 0.58s.
          const expectedStartTime = 0.54 + 0.04;
          expect(sources[1].start).toHaveBeenCalledWith(expectedStartTime);
          expect(scheduledEvents[1].startTimeSec).toBeCloseTo(expectedStartTime);
        }
      } finally {
        vi.useRealTimers();
      }
    });
  }

  // ──────────────────────────────────────────────────────────────────────────
  // 2. Empirical Verification of stop() Execution Time & Barge-in Latency
  // ──────────────────────────────────────────────────────────────────────────

  it("empirically verifies that stop() executes in < 10ms across 1,000 iterations", async () => {
    const latenciesMs: number[] = [];

    for (let i = 0; i < 1000; i++) {
      const speaker = useSpeakerPlayback({
        useMasterGain: true,
        graceTimeoutMs: 350,
      });

      // Schedule 3 chunks to build active state
      await speaker.enqueueSpeakerPayload(createPcmChunk(800));
      await speaker.enqueueSpeakerPayload(createPcmChunk(800));
      await speaker.enqueueSpeakerPayload(createPcmChunk(800));

      expect(speaker.isPlaying()).toBe(true);
      expect(speaker.hasActiveSources()).toBe(true);

      // Measure synchronous execution time of stop()
      const tStart = performance.now();
      speaker.stop();
      const tElapsed = performance.now() - tStart;
      latenciesMs.push(tElapsed);

      // Immediate post-stop assertions:
      expect(speaker.isPlaying()).toBe(false);
      expect(speaker.hasActiveSources()).toBe(false);
      expect(speaker.isBlocked()).toBe(true);
    }

    const avgLatency = latenciesMs.reduce((a, b) => a + b, 0) / latenciesMs.length;
    const maxLatency = Math.max(...latenciesMs);
    const p99Latency = latenciesMs.slice().sort((a, b) => a - b)[Math.floor(latenciesMs.length * 0.99)];

    // Log empirical metrics for handoff report
    // Average should be < 0.2ms, max strictly < 10ms
    expect(avgLatency).toBeLessThan(1.0);
    expect(p99Latency).toBeLessThan(5.0);
    expect(maxLatency).toBeLessThan(10.0);
  });

  it("empirically verifies immediate audio cutoff upon stop() during active playback", async () => {
    const onPlaybackFinished = vi.fn();
    const onQueueDrained = vi.fn();
    const speaker = useSpeakerPlayback({
      useMasterGain: true,
      onPlaybackFinished,
      onQueueDrained,
    });

    await speaker.enqueueSpeakerPayload(createPcmChunk(1600));
    await speaker.enqueueSpeakerPayload(createPcmChunk(1600));

    expect(sources).toHaveLength(2);
    expect(speaker.isPlaying()).toBe(true);

    const masterGain = gains[0];
    speaker.stop();

    // Verify gain linear ramp to 0 within 15ms
    expect(masterGain.gain.setValueAtTime).toHaveBeenCalled();
    expect(masterGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(0, 0.015);

    // Verify all active AudioBufferSourceNodes have stop() scheduled
    expect(sources[0].stop).toHaveBeenCalledWith(0.015);
    expect(sources[1].stop).toHaveBeenCalledWith(0.015);

    // Verify state dropped immediately
    expect(speaker.isPlaying()).toBe(false);
    expect(speaker.hasActiveSources()).toBe(false);
    expect(onPlaybackFinished).toHaveBeenCalledOnce();
    expect(onQueueDrained).toHaveBeenCalledOnce();
  });

  it("empirically verifies stop() cancels graceTimer immediately when called during grace period", async () => {
    vi.useFakeTimers();
    try {
      const onPlaybackFinished = vi.fn();
      const speaker = useSpeakerPlayback({
        graceTimeoutMs: 350,
        onPlaybackFinished,
      });

      await speaker.enqueueSpeakerPayload(createPcmChunk(1600));
      expect(speaker.isPlaying()).toBe(true);

      // Chunk finishes
      sources[0].onended?.();

      // In grace window: playing is still true
      expect(speaker.isPlaying()).toBe(true);

      // Barge-in occurs at 100ms into grace window
      vi.advanceTimersByTime(100);
      speaker.stop();

      // State must drop immediately
      expect(speaker.isPlaying()).toBe(false);
      expect(onPlaybackFinished).toHaveBeenCalledOnce();

      // Advancing past 350ms must not trigger duplicate callbacks
      vi.advanceTimersByTime(300);
      expect(onPlaybackFinished).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  // ──────────────────────────────────────────────────────────────────────────
  // 3. Multi-Clause Streaming Stress Test (10 Sequential Clauses with Varied Gaps)
  // ──────────────────────────────────────────────────────────────────────────

  it("handles 10-clause conversational stream with varied inter-clause gaps seamlessly", async () => {
    vi.useFakeTimers();
    try {
      const scheduled: { startTimeSec: number; durationSec: number }[] = [];
      const onPlaybackFinished = vi.fn();
      const speaker = useSpeakerPlayback({
        preRollBufferSec: 0.04,
        graceTimeoutMs: 350,
        onPlaybackFinished,
        onChunkScheduled: (info) => scheduled.push(info),
      });

      const interClauseGapsMs = [50, 120, 200, 80, 150, 300, 60, 100, 250];

      // Send 10 chunks separated by realistic LLM generation pauses (<350ms)
      for (let i = 0; i < 10; i++) {
        await speaker.enqueueSpeakerPayload(createPcmChunk(1600, 16000)); // 0.1s duration
        expect(speaker.isPlaying()).toBe(true);

        if (i < 9) {
          // Source ends
          sources[i].onended?.();
          expect(speaker.isPlaying()).toBe(true);

          // Advance by simulated pause
          vi.advanceTimersByTime(interClauseGapsMs[i]);
          expect(speaker.isPlaying()).toBe(true);
        }
      }

      // Verify that all 10 chunks were scheduled continuously without pre-roll resets
      expect(scheduled).toHaveLength(10);
      expect(scheduled[0].startTimeSec).toBeCloseTo(0.04);
      for (let i = 1; i < 10; i++) {
        const expectedStart = 0.04 + i * 0.1;
        expect(scheduled[i].startTimeSec).toBeCloseTo(expectedStart);
      }

      // Finish last source and wait for grace timeout
      sources[9].onended?.();
      expect(speaker.isPlaying()).toBe(true);

      vi.advanceTimersByTime(350);
      expect(speaker.isPlaying()).toBe(false);
      expect(onPlaybackFinished).toHaveBeenCalledOnce();
    } finally {
      vi.useRealTimers();
    }
  });
});
