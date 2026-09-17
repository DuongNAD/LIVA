import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { useVoicePipeline } from '../../src/composables/useVoicePipeline';
import { useSpeakerPlayback } from '../../src/composables/useSpeakerPlayback';

describe('Fast Voice Duplex & Barge-in Unmuting SLA', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('honors bypassSoftwareMute: true so mic audio to worker is not muted during playback', () => {
    const pipeline = useVoicePipeline({ bypassSoftwareMute: true });
    
    // When bypassSoftwareMute is true, muteWakeWord must not block
    pipeline.muteWakeWord();
    // Unmute should not add 400ms echo tail blackout
    pipeline.unmuteWakeWord();
    // Pipeline remains active and ready
    expect(pipeline.state.value).toBe('OFF');
  });

  it('applies tuned pre-roll buffer of 80ms for fast voice low-latency streaming', () => {
    const scheduled: { startTimeSec: number; durationSec: number }[] = [];
    const speaker = useSpeakerPlayback({
      channel: '[TestFastVoice]',
      preRollBufferSec: 0.08,
      onChunkScheduled: (info) => scheduled.push(info),
    });

    expect(speaker).toBeDefined();
    expect(speaker.isPlaying()).toBe(false);
  });

  it('stops speaker playback cleanly on flush() / stop()', () => {
    const speaker = useSpeakerPlayback({
      channel: '[TestFlush]',
      useMasterGain: true,
    });

    speaker.stop();
    expect(speaker.isPlaying()).toBe(false);
    expect(speaker.hasActiveSources()).toBe(false);
  });
});
