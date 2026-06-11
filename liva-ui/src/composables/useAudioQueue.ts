import { ref, type Ref } from "vue";
import { logger } from "../utils/logger";

export function useAudioQueue(
  engineRef: Ref<any>,
  isThinking: Ref<boolean>,
  voice: any,
  sendMsg: (event: string, payload?: any) => void
) {
  let audioCtx: AudioContext | null = null;
  let masterGain: GainNode | null = null;
  let nextAudioTime = 0;
  let activeAudioSources: AudioBufferSourceNode[] = [];
  let audioQueueEpoch = 0;
  let isAudioPlaybackBlocked = false;
  const isPlayingAudio = ref(false);

  let wakeWordAudioCtx: AudioContext | null = null;

  const removeAudioSource = (source: AudioBufferSourceNode) => {
    activeAudioSources = activeAudioSources.filter((item) => item !== source);
  };

  const stopQueuedAudio = (blockIncomingChunks = true) => {
    if (blockIncomingChunks) {
      isAudioPlaybackBlocked = true;
    }

    audioQueueEpoch++;
    const sources = activeAudioSources;
    activeAudioSources = [];

    for (const source of sources) {
      try {
        source.stop();
      } catch {
        // Ignore errors if source already stopped or not started
      }
    }

    if (engineRef.value?.stopAudioLipSync) {
      engineRef.value.stopAudioLipSync();
    }

    nextAudioTime = audioCtx ? audioCtx.currentTime : 0;
    if (masterGain) masterGain.gain.value = 1.0;

    if (isPlayingAudio.value) {
      isPlayingAudio.value = false;
      sendMsg("audio_play_finished");
    }
    if (!isThinking.value && voice.state.value === "PROCESSING") {
      voice.setPassive();
    }
  };

  const allowIncomingChunks = () => {
    isAudioPlaybackBlocked = false;
  };

  const playWakeWordSound = () => {
    try {
      if (!wakeWordAudioCtx) {
        const AudioContextCls = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
        wakeWordAudioCtx = new AudioContextCls();
      }
      if (wakeWordAudioCtx.state === "suspended") {
        wakeWordAudioCtx.resume();
      }

      const playTone = (freq: number, startTime: number, duration: number) => {
        const oscillator = wakeWordAudioCtx!.createOscillator();
        const gainNode = wakeWordAudioCtx!.createGain();

        oscillator.connect(gainNode);
        gainNode.connect(wakeWordAudioCtx!.destination);

        oscillator.type = "sine";
        oscillator.frequency.value = freq;

        gainNode.gain.setValueAtTime(0, startTime);
        gainNode.gain.linearRampToValueAtTime(0.3, startTime + 0.02);
        gainNode.gain.exponentialRampToValueAtTime(0.001, startTime + duration);

        oscillator.start(startTime);
        oscillator.stop(startTime + duration);
      };

      const now = wakeWordAudioCtx.currentTime;
      playTone(415.30, now, 0.15);       // G#4
      playTone(554.37, now + 0.15, 0.2); // C#5
    } catch (err) {
      logger.warn("[AudioQueue]", "Could not play wake word sound:", err);
    }
  };

  const playAudioBuffer = async (audioBuffer: AudioBuffer) => {
    if (isAudioPlaybackBlocked || !audioCtx) return;

    const source = audioCtx.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(masterGain || audioCtx.destination);
    source.onended = () => {
      removeAudioSource(source);
      if (activeAudioSources.length === 0 && engineRef.value?.stopAudioLipSync) {
        engineRef.value.stopAudioLipSync();
      }
      if (activeAudioSources.length === 0 && !isThinking.value && voice.state.value === "PROCESSING") {
        voice.setPassive();
      }
      if (activeAudioSources.length === 0 && isPlayingAudio.value) {
        isPlayingAudio.value = false;
        sendMsg("audio_play_finished");
      }
    };

    const overlap = 0.1;
    const currentTime = audioCtx.currentTime;
    if (nextAudioTime < currentTime) nextAudioTime = currentTime;
    activeAudioSources.push(source);

    if (!isPlayingAudio.value && activeAudioSources.length === 1) {
      isPlayingAudio.value = true;
      sendMsg("audio_play_started");
    }

    source.start(nextAudioTime);
    nextAudioTime += (audioBuffer.duration - overlap);

    if (engineRef.value?.startAudioLipSync && audioCtx) {
      engineRef.value.startAudioLipSync(audioCtx, source);
    }
  };

  const handleBinaryAudioChunk = async (audioData: Uint8Array) => {
    try {
      if (!audioCtx) {
        const AudioContextCls = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
        audioCtx = new AudioContextCls();
      }
      if (!masterGain && audioCtx) {
        masterGain = audioCtx.createGain();
        masterGain.connect(audioCtx.destination);
      }
      if (audioCtx.state === "suspended") await audioCtx.resume();

      const queueEpoch = audioQueueEpoch;
      const buffer = audioData.slice().buffer as ArrayBuffer;
      const audioBuffer = await audioCtx.decodeAudioData(buffer);
      if (queueEpoch !== audioQueueEpoch || isAudioPlaybackBlocked) return;

      await playAudioBuffer(audioBuffer);
    } catch (audioErr: unknown) {
      logger.warn("[AudioQueue]", "Binary audio decode error:", audioErr instanceof Error ? audioErr.message : String(audioErr));
    }
  };

  const handleBase64AudioChunk = async (base64Data: string) => {
    try {
      if (!audioCtx) {
        const AudioContextCls = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
        audioCtx = new AudioContextCls();
      }
      if (!masterGain && audioCtx) {
        masterGain = audioCtx.createGain();
        masterGain.connect(audioCtx.destination);
      }
      if (audioCtx.state === "suspended") await audioCtx.resume();

      const queueEpoch = audioQueueEpoch;
      const binaryStr = atob(base64Data);
      const bytes = new Uint8Array(binaryStr.length);
      for (let i = 0; i < binaryStr.length; i++) {
        bytes[i] = binaryStr.charCodeAt(i);
      }

      const audioBuffer = await audioCtx.decodeAudioData(bytes.buffer);
      if (queueEpoch !== audioQueueEpoch || isAudioPlaybackBlocked) return;

      await playAudioBuffer(audioBuffer);
    } catch (audioErr: unknown) {
      logger.warn("[AudioQueue]", "Audio decode/playback error:", audioErr instanceof Error ? audioErr.message : String(audioErr));
    }
  };

  const duckAudio = (volume: number) => {
    if (masterGain) {
      masterGain.gain.setTargetAtTime(volume, masterGain.context.currentTime, 0.05);
    }
  };

  const cleanup = () => {
    stopQueuedAudio();
    if (audioCtx) {
      audioCtx.close();
      audioCtx = null;
    }
    if (wakeWordAudioCtx) {
      wakeWordAudioCtx.close();
      wakeWordAudioCtx = null;
    }
  };

  return {
    isPlayingAudio,
    activeAudioSources: ref(activeAudioSources), // Return as ref for template reactive bindings if needed
    stopQueuedAudio,
    allowIncomingChunks,
    playWakeWordSound,
    handleBinaryAudioChunk,
    handleBase64AudioChunk,
    duckAudio,
    cleanup,
  };
}
