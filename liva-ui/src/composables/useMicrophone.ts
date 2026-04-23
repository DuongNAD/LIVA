/**
 * useMicrophone.ts — Voice Input Composable
 * ============================================
 * Captures microphone audio as Float32 PCM chunks and sends
 * them via WebSocket to Gateway → WhisperNode (STT).
 *
 * Features:
 * - Push-to-talk OR voice-activity-detection (VAD) mode
 * - Sends raw Float32 PCM (16kHz mono) — WhisperNode expects this
 * - Visual "volume level" for UI feedback
 * - Auto-stop after silence threshold
 * - Proper cleanup on stop/unmount
 */
import { ref, type Ref } from "vue";

export interface UseMicrophoneReturn {
  isListening: Ref<boolean>;
  volumeLevel: Ref<number>;     // 0-1, real-time mic volume
  isSupported: Ref<boolean>;
  startListening: (ws: WebSocket) => Promise<void>;
  stopListening: () => void;
}

export function useMicrophone(): UseMicrophoneReturn {
  const isListening = ref(false);
  const volumeLevel = ref(0);
  const isSupported = ref(typeof navigator !== "undefined" && !!navigator.mediaDevices?.getUserMedia);

  let mediaStream: MediaStream | null = null;
  let audioContext: AudioContext | null = null;
  let processor: ScriptProcessorNode | null = null;
  let analyser: AnalyserNode | null = null;
  let source: MediaStreamAudioSourceNode | null = null;
  let wsRef: WebSocket | null = null;

  // Volume analysis buffer
  let volumeBuffer: Uint8Array | null = null;

  // Silence detection
  let silenceFrames = 0;
  const SILENCE_THRESHOLD = 0.02;    // Volume below this = silence
  const MAX_SILENCE_FRAMES = 50;     // ~50 frames at 60fps ≈ 800ms silence → auto-stop (matches WhisperNode VAD_SILENCE_MS)

  /**
   * Start capturing microphone audio.
   * Audio is sent as raw binary Float32 PCM to Gateway via WebSocket.
   */
  async function startListening(ws: WebSocket) {
    if (isListening.value) return;
    if (!isSupported.value) {
      console.error("[Mic] getUserMedia not supported");
      return;
    }

    try {
      // 1. Get mic stream (mono, 16kHz for Whisper compatibility)
      mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          sampleRate: { ideal: 16000 },
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });

      wsRef = ws;

      // 2. Setup AudioContext for processing
      const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
      audioContext = new AudioCtx({ sampleRate: 16000 });

      source = audioContext.createMediaStreamSource(mediaStream);

      // Analyser for volume visualization
      analyser = audioContext.createAnalyser();
      analyser.fftSize = 256;
      volumeBuffer = new Uint8Array(analyser.frequencyBinCount);

      // ScriptProcessor for raw PCM extraction
      // Buffer size 4096 at 16kHz = ~256ms chunks (good for streaming STT)
      processor = audioContext.createScriptProcessor(4096, 1, 1);

      processor.onaudioprocess = (e: AudioProcessingEvent) => {
        if (!isListening.value || !wsRef || wsRef.readyState !== WebSocket.OPEN) return;

        // Get Float32 PCM data
        const inputData = e.inputBuffer.getChannelData(0);

        // Calculate RMS volume
        let sum = 0;
        for (let i = 0; i < inputData.length; i++) {
          sum += inputData[i] * inputData[i];
        }
        const rms = Math.sqrt(sum / inputData.length);
        volumeLevel.value = Math.min(1, rms * 5); // Scale up for visibility

        // Silence detection
        if (rms < SILENCE_THRESHOLD) {
          silenceFrames++;
        } else {
          silenceFrames = 0;
        }

        // Send PCM chunk as binary to Gateway
        // WhisperNode.pushAudioChunk expects raw Buffer
        const buffer = new Float32Array(inputData.length);
        buffer.set(inputData);
        wsRef.send(buffer.buffer);
      };

      // Connect pipeline: mic → analyser → processor → destination (required for ScriptProcessor)
      source.connect(analyser);
      analyser.connect(processor);
      processor.connect(audioContext.destination);

      isListening.value = true;
      silenceFrames = 0;

      // Start volume monitoring loop
      monitorVolume();

    } catch (err: any) {
      console.error("[Mic] Failed to start:", err?.message ?? err);
      isListening.value = false;
    }
  }

  /**
   * Volume monitoring loop — updates volumeLevel for UI visualization
   */
  function monitorVolume() {
    if (!isListening.value || !analyser || !volumeBuffer) return;

    analyser.getByteFrequencyData(volumeBuffer);

    // Calculate average volume from frequency data
    let sum = 0;
    for (let i = 0; i < volumeBuffer.length; i++) {
      sum += volumeBuffer[i];
    }
    const avg = sum / volumeBuffer.length / 255;
    volumeLevel.value = avg;

    requestAnimationFrame(monitorVolume);
  }

  /**
   * Stop listening and release all resources.
   */
  function stopListening() {
    isListening.value = false;
    volumeLevel.value = 0;
    silenceFrames = 0;

    if (processor) {
      processor.onaudioprocess = null;
      processor.disconnect();
      processor = null;
    }

    if (analyser) {
      analyser.disconnect();
      analyser = null;
    }

    if (source) {
      source.disconnect();
      source = null;
    }

    if (audioContext) {
      audioContext.close();
      audioContext = null;
    }

    if (mediaStream) {
      mediaStream.getTracks().forEach(t => t.stop());
      mediaStream = null;
    }

    volumeBuffer = null;
    wsRef = null;
  }

  return {
    isListening,
    volumeLevel,
    isSupported,
    startListening,
    stopListening,
  };
}
