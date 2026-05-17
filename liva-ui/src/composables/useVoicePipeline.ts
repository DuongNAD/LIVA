import { ref, shallowRef, triggerRef, type Ref } from "vue";
import { logger } from "../utils/logger";

export interface UseVoicePipelineReturn {
  state: Ref<'OFF' | 'PASSIVE' | 'ACTIVE' | 'PROCESSING'>;
  volumeLevel: Ref<number>;
  isReady: Ref<boolean>;
  startPipeline: (ws: WebSocket) => Promise<void>;
  stopPipeline: () => Promise<void>;
  toggleVoice: () => void;
  onWakeWordDetected: (cb: () => void) => void;
}

// Global worker to avoid reloading
let wakeWordWorker: Worker | null = null;
let isWorkerReady = false;
let detectedCallback: (() => void) | null = null;

function initWorker(): Promise<boolean> {
  return new Promise((resolve) => {
    if (wakeWordWorker && isWorkerReady) {
      resolve(true);
      return;
    }

    wakeWordWorker = new Worker(
      new URL('../workers/WakeWordWorker.ts', import.meta.url),
      { type: 'module' }
    );

    wakeWordWorker.onmessage = (event) => {
      const { type, success } = event.data;

      if (type === '__log') {
        const { level, args } = event.data;
        const fn = logger[level as "debug" | "info" | "warn" | "error"] ?? logger.info;
        fn('[WakeWord]', ...args);
        return;
      }

      if (type === 'loaded') {
        wakeWordWorker?.postMessage({ type: 'init' });
      } else if (type === 'ready') {
        isWorkerReady = success;
        resolve(success);
      } else if (type === 'detection') {
        if (detectedCallback) detectedCallback();
      }
    };

    wakeWordWorker.onerror = (error) => {
      logger.error('[WakeWordWorker]', 'Worker error:', error);
      resolve(false);
    };
  });
}

function sendToWorker(type: string, data?: any) {
  if (wakeWordWorker) {
    wakeWordWorker.postMessage({ type, data });
  }
}

export function useVoicePipeline(): UseVoicePipelineReturn {
  const state = ref<'OFF' | 'PASSIVE' | 'ACTIVE' | 'PROCESSING'>('OFF');
  const volumeLevel = shallowRef<number>(0);
  const isReady = ref(false);

  let mediaStream: MediaStream | null = null;
  let audioContext: AudioContext | null = null;
  let processor: ScriptProcessorNode | null = null;
  let source: MediaStreamAudioSourceNode | null = null;
  let wsRef: WebSocket | null = null;
  
  let analyser: AnalyserNode | null = null;
  let volumeBuffer: Uint8Array | null = null;
  let volumeRAF: number | null = null;

  let activeTimeoutId: NodeJS.Timeout | null = null;
  const SILENCE_THRESHOLD = 0.02;

  function onWakeWordDetected(cb: () => void) {
    detectedCallback = () => {
      if (state.value === 'PASSIVE') {
        state.value = 'ACTIVE';
        resetActiveTimeout();
        cb();
        // Notify backend for analytics/logging
        if (wsRef && wsRef.readyState === WebSocket.OPEN) {
          wsRef.send(JSON.stringify({ event: 'wake_word_triggered', payload: {} }));
        }
      }
    };
  }

  function resetActiveTimeout() {
    if (activeTimeoutId) clearTimeout(activeTimeoutId);
    activeTimeoutId = setTimeout(() => {
      if (state.value === 'ACTIVE' || state.value === 'PROCESSING') {
        logger.warn('[VoicePipeline] 15s timeout reached. Returning to PASSIVE.');
        state.value = 'PASSIVE';
      }
    }, 15000);
  }

  async function startPipeline(ws: WebSocket) {
    if (state.value !== 'OFF') return;
    
    wsRef = ws;
    
    if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
      logger.error('[VoicePipeline]', 'getUserMedia not supported');
      return;
    }

    try {
      const workerReady = await initWorker();
      if (!workerReady) {
        logger.error('[VoicePipeline]', 'Failed to initialize ONNX worker');
        return;
      }

      mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          sampleRate: { ideal: 16000 },
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });

      const AudioCtx = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
      audioContext = new AudioCtx({ sampleRate: 16000 });
      source = audioContext.createMediaStreamSource(mediaStream);

      analyser = audioContext.createAnalyser();
      analyser.fftSize = 256;
      volumeBuffer = new Uint8Array(analyser.frequencyBinCount);

      processor = audioContext.createScriptProcessor(4096, 1, 1);

      processor.onaudioprocess = (e: AudioProcessingEvent) => {
        if (state.value === 'OFF') return;

        const inputData = e.inputBuffer.getChannelData(0);

        let sumSquares = 0;
        for (let i = 0; i < inputData.length; i++) {
          sumSquares += inputData[i] * inputData[i];
        }
        const rms = Math.sqrt(sumSquares / inputData.length);

        // 1. ALWAYS send to WakeWordWorker for 24/7 detection
        if (rms > 0.01) {
          sendToWorker('audio', { audio: Array.from(inputData) });
        }

        // 2. VALVE: Send to WebSocket if ACTIVE or PROCESSING (Full-Duplex Barge-in)
        if ((state.value === 'ACTIVE' || state.value === 'PROCESSING') && wsRef && wsRef.readyState === WebSocket.OPEN) {
          const buffer = new Float32Array(inputData.length);
          buffer.set(inputData);
          wsRef.send(buffer.buffer);

          if (rms >= SILENCE_THRESHOLD) {
            resetActiveTimeout(); // Keeps session alive while speaking
          }
        }
      };

      source.connect(analyser);
      analyser.connect(processor);
      processor.connect(audioContext.destination);

      state.value = 'PASSIVE';
      isReady.value = true;
      monitorVolume();
      logger.info('[VoicePipeline]', 'Started 24/7 Omni-Duplex Pipeline');

    } catch (err: unknown) {
      logger.error('[VoicePipeline]', 'Failed to start:', err instanceof Error ? err.message : String(err));
      state.value = 'OFF';
      isReady.value = false;
    }
  }

  function monitorVolume() {
    if (state.value === 'OFF' || !analyser || !volumeBuffer) {
      if (volumeRAF !== null) { cancelAnimationFrame(volumeRAF); volumeRAF = null; }
      return;
    }

    analyser.getByteFrequencyData(volumeBuffer);
    let sum = 0;
    for (let i = 0; i < volumeBuffer.length; i++) {
      sum += volumeBuffer[i];
    }
    const avg = sum / volumeBuffer.length / 255;
    
    // shallowRef: update value without deep reactive proxy overhead
    volumeLevel.value = avg;
    triggerRef(volumeLevel);

    volumeRAF = requestAnimationFrame(monitorVolume);
  }

  async function stopPipeline() {
    state.value = 'OFF';
    isReady.value = false;

    if (activeTimeoutId) {
      clearTimeout(activeTimeoutId);
      activeTimeoutId = null;
    }

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

    if (wakeWordWorker) {
      sendToWorker('terminate');
      wakeWordWorker = null;
      isWorkerReady = false;
    }

    wsRef = null;
    if (volumeRAF !== null) { cancelAnimationFrame(volumeRAF); volumeRAF = null; }
    logger.info('[VoicePipeline]', 'Stopped entirely');
  }

  function toggleVoice() {
    if (state.value === 'PASSIVE') {
      state.value = 'ACTIVE';
      resetActiveTimeout();
    } else if (state.value === 'ACTIVE' || state.value === 'PROCESSING') {
      state.value = 'PASSIVE';
      if (activeTimeoutId) clearTimeout(activeTimeoutId);
    }
  }

  return {
    state,
    volumeLevel,
    isReady,
    startPipeline,
    stopPipeline,
    toggleVoice,
    onWakeWordDetected
  };
}
