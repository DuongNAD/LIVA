import { onUnmounted } from "vue";
import { logger } from "../utils/logger";
import { parseSpeakerPayload } from "../utils/speakerFrame";

/**
 * useSpeakerPlayback — gapless TTS speaker output queue
 * =====================================================
 * Plays OP_SPEAKER_OUT VoiceFrame payloads from the Rust core:
 * raw PCM ([turn_epoch u32 LE][sample_rate u32 LE][f32 LE mono samples…])
 * on the binary fast path. Malformed binary speaker payloads are dropped;
 * encoded audio is accepted only through the separate JSON audio path.
 *
 * Chunks arrive sequentially per sentence and are scheduled back-to-back on a
 * `nextStartTime` cursor so playback is gapless. All scheduled
 * AudioBufferSourceNodes are tracked so FLUSH/barge-in can stop() them all
 * and reset the cursor.
 */

export interface UseSpeakerPlaybackOptions {
  /** Logger channel, e.g. '[App]' or '[Widget]'. */
  channel?: string;
  /** Route playback through a master GainNode (required for audio ducking). */
  useMasterGain?: boolean;
  /**
   * Insert a persistent AnalyserNode at the head of the output chain so a
   * consumer can drive lip-sync from the audio that is actually audible.
   * Read it with `getAnalyser()`. It lives as long as the AudioContext and is
   * never rebuilt per chunk — see the note on `outputNode()`.
   */
  enableAnalyser?: boolean;
  /** First chunk of a run started playing — e.g. sendMsg("audio_play_started"). */
  onPlaybackStarted?: () => void;
  /** Run finished or was stopped — e.g. sendMsg("audio_play_finished"). */
  onPlaybackFinished?: () => void;
  /** A chunk source was scheduled — drive lip-sync / avatar motion. */
  onSourceStarted?: (ctx: AudioContext, source: AudioBufferSourceNode) => void;
  /** Queue emptied naturally or playback stopped — stop lip-sync, release voice state. */
  onQueueDrained?: () => void;
}

export interface UseSpeakerPlaybackReturn {
  /** Lazily create the shared AudioContext (does not resume it). */
  ensureContext: () => AudioContext;
  getContext: () => AudioContext | null;
  /** Smoothed master volume for audio ducking. No-op until the master gain exists. */
  setMasterVolume: (volume: number) => void;
  /** OP_SPEAKER_OUT payload → PCM only; malformed binary payloads are dropped. */
  enqueueSpeakerPayload: (payload: Uint8Array) => Promise<void>;
  /** JSON audio path: compressed bytes (MP3) → decodeAudioData → schedule. */
  enqueueEncodedAudio: (data: ArrayBuffer) => Promise<void>;
  /**
   * OP_FLUSH barge-in: stop all scheduled sources and reset the cursor, but
   * keep accepting chunks — frames after a FLUSH belong to the new session.
   */
  flush: () => void;
  /** Stop playback. blockIncomingChunks=true also discards chunks until unblock(). */
  stop: (blockIncomingChunks?: boolean) => void;
  /** Accept chunks again (ai_stream_start / ai_spoken_response). */
  unblock: () => void;
  isBlocked: () => boolean;
  isPlaying: () => boolean;
  hasActiveSources: () => boolean;
  /**
   * The analyser sitting in the output chain, or null when `enableAnalyser`
   * is off or nothing has been scheduled yet. Consumers read from it; they
   * must not connect or disconnect it.
   */
  getAnalyser: () => AnalyserNode | null;
  /** Stop playback and close the AudioContext (component unmount). */
  close: () => void;
}

/**
 * Legacy MP3 chunks carry ~100ms of encoder padding, so they are overlapped
 * slightly. Raw PCM chunks are sample-exact and must be scheduled gaplessly.
 */
const LEGACY_MP3_OVERLAP_S = 0.1;

export function useSpeakerPlayback(
  options: UseSpeakerPlaybackOptions = {}
): UseSpeakerPlaybackReturn {
  const channel = options.channel ?? "[SpeakerPlayback]";

  let audioCtx: AudioContext | null = null;
  let masterGain: GainNode | null = null;
  let analyser: AnalyserNode | null = null;
  /** Gapless scheduling cursor: earliest time the next chunk may start. */
  let nextStartTime = 0;
  let activeSources: AudioBufferSourceNode[] = [];
  /** Bumped on stop/flush so in-flight async decodes drop their stale chunk. */
  let queueEpoch = 0;
  let blocked = false;
  let playing = false;

  function ensureContext(): AudioContext {
    if (!audioCtx) {
      const AudioContextCls =
        globalThis.AudioContext ||
        (globalThis as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      audioCtx = new AudioContextCls();
    }
    return audioCtx;
  }

  /** Tail of the output chain: the ducking gain when enabled, else the speakers. */
  function sinkNode(ctx: AudioContext): AudioNode {
    if (options.useMasterGain) {
      if (!masterGain) {
        masterGain = ctx.createGain();
        masterGain.connect(ctx.destination);
      }
      return masterGain;
    }
    return ctx.destination;
  }

  /**
   * Head of the output chain. Every scheduled source connects HERE and nowhere
   * else, so the graph stays a single route to the speakers:
   *   `source → analyser → masterGain → destination`
   *
   * The analyser sits *inside* that chain rather than hanging off the source as
   * a second branch. A parallel tap looks harmless but is not: the same buffer
   * would reach the destination twice (summed, ~2x amplitude), and
   * `setMasterVolume()` would attenuate only one of the two routes, so audio
   * ducking could never fully duck. It is also built once per context — never
   * per chunk — because chunks are scheduled ahead of playback, so a
   * per-chunk analyser spends most of its life attached to a source that has
   * not started making sound yet.
   */
  function outputNode(ctx: AudioContext): AudioNode {
    if (!options.enableAnalyser) return sinkNode(ctx);
    if (!analyser) {
      analyser = ctx.createAnalyser();
      analyser.connect(sinkNode(ctx));
    }
    return analyser;
  }

  function handleSourceEnded(source: AudioBufferSourceNode): void {
    activeSources = activeSources.filter((item) => item !== source);
    if (activeSources.length > 0) return;
    if (options.onQueueDrained) options.onQueueDrained();
    if (playing) {
      playing = false;
      if (options.onPlaybackFinished) options.onPlaybackFinished();
    }
  }

  function scheduleBuffer(ctx: AudioContext, audioBuffer: AudioBuffer, overlap: number): void {
    const source = ctx.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(outputNode(ctx));
    source.onended = () => handleSourceEnded(source);

    if (nextStartTime < ctx.currentTime) {
      nextStartTime = ctx.currentTime;
    }
    activeSources.push(source);

    if (!playing && activeSources.length === 1) {
      playing = true;
      if (options.onPlaybackStarted) options.onPlaybackStarted();
    }

    source.start(nextStartTime);
    nextStartTime += audioBuffer.duration - overlap;

    if (options.onSourceStarted) options.onSourceStarted(ctx, source);
  }

  async function enqueueSpeakerPayload(payload: Uint8Array): Promise<void> {
    if (blocked) return;

    const chunk = parseSpeakerPayload(payload);
    if (!chunk) {
      logger.warn(channel, "Dropping malformed OP_SPEAKER_OUT payload");
      return;
    }

    try {
      const ctx = ensureContext();
      const epoch = queueEpoch;
      if (ctx.state === "suspended") {
        await ctx.resume();
      }
      if (epoch !== queueEpoch || blocked) return;

      const audioBuffer = ctx.createBuffer(1, chunk.samples.length, chunk.sampleRate);
      audioBuffer.copyToChannel(chunk.samples, 0);
      scheduleBuffer(ctx, audioBuffer, 0);
    } catch (err: unknown) {
      logger.warn(channel, "Speaker PCM playback error:", err instanceof Error ? err.message : String(err));
    }
  }

  async function enqueueEncodedAudio(data: ArrayBuffer): Promise<void> {
    if (blocked) return;

    try {
      const ctx = ensureContext();
      const epoch = queueEpoch;
      if (ctx.state === "suspended") {
        await ctx.resume();
      }
      const audioBuffer = await ctx.decodeAudioData(data);
      if (epoch !== queueEpoch || blocked) return;
      scheduleBuffer(ctx, audioBuffer, LEGACY_MP3_OVERLAP_S);
    } catch (err: unknown) {
      logger.warn(channel, "Audio decode/playback error:", err instanceof Error ? err.message : String(err));
    }
  }

  function stop(blockIncomingChunks = true): void {
    if (blockIncomingChunks) {
      blocked = true;
    }

    queueEpoch++;
    const sources = activeSources;
    activeSources = [];

    if (audioCtx && masterGain && sources.length > 0) {
      const now = audioCtx.currentTime;
      try {
        if (typeof masterGain.gain.setValueAtTime === "function") {
          masterGain.gain.setValueAtTime(masterGain.gain.value, now);
        }
        if (typeof masterGain.gain.linearRampToValueAtTime === "function") {
          masterGain.gain.linearRampToValueAtTime(0, now + 0.015);
        }
      } catch {
        // Fallback if audio param method fails
      }

      for (const source of sources) {
        try {
          source.stop(now + 0.015);
        } catch {
          try {
            source.stop();
          } catch {
            // Source may already have ended or may not have reached its scheduled start.
          }
        }
      }

      const currentGain = masterGain;
      const currentCtx = audioCtx;
      setTimeout(() => {
        if (currentGain) {
          try {
            if (typeof currentGain.gain.setValueAtTime === "function" && currentCtx) {
              currentGain.gain.setValueAtTime(1.0, currentCtx.currentTime);
            } else {
              currentGain.gain.value = 1.0;
            }
          } catch {
            currentGain.gain.value = 1.0;
          }
        }
      }, 16);
    } else {
      for (const source of sources) {
        try {
          source.stop();
        } catch {
          // Source may already have ended or may not have reached its scheduled start.
        }
      }
      if (masterGain) masterGain.gain.value = 1.0;
    }

    nextStartTime = audioCtx ? audioCtx.currentTime : 0;

    if (options.onQueueDrained) options.onQueueDrained();
    if (playing) {
      playing = false;
      if (options.onPlaybackFinished) options.onPlaybackFinished();
    }
  }

  function flush(): void {
    stop(false);
  }

  function unblock(): void {
    blocked = false;
  }

  function setMasterVolume(volume: number): void {
    if (masterGain) {
      masterGain.gain.setTargetAtTime(volume, masterGain.context.currentTime, 0.05);
    }
  }

  function close(): void {
    stop();
    if (audioCtx && audioCtx.state !== "closed") {
      audioCtx.close().catch(() => {
        // Context already closing/closed — nothing to release.
      });
    }
    audioCtx = null;
    masterGain = null;
    analyser = null;
  }

  onUnmounted(close);

  return {
    ensureContext,
    getContext: () => audioCtx,
    setMasterVolume,
    enqueueSpeakerPayload,
    enqueueEncodedAudio,
    flush,
    stop,
    unblock,
    isBlocked: () => blocked,
    isPlaying: () => playing,
    hasActiveSources: () => activeSources.length > 0,
    getAnalyser: () => analyser,
    close,
  };
}
