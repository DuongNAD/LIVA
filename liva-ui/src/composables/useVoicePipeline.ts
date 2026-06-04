import { ref, shallowRef, triggerRef, watch, type Ref } from "vue";
import { logger } from "../utils/logger";
import { pack } from "msgpackr";

export interface UseVoicePipelineReturn {
  state: Ref<'OFF' | 'PASSIVE' | 'ACTIVE' | 'PROCESSING'>;
  volumeLevel: Ref<number>;
  isReady: Ref<boolean>;
  startPipeline: (ws: WebSocket) => Promise<void>;
  stopPipeline: () => Promise<void>;
  toggleVoice: () => void;
  onWakeWordDetected: (cb: () => void) => void;
  /** Transition ACTIVE → PROCESSING (AI is thinking). Resets timeout. */
  setProcessing: () => void;
  /** Transition ACTIVE/PROCESSING → PASSIVE (conversation turn done). Clears timeout. */
  setPassive: () => void;
  /** Reset the 15s inactivity timeout without changing state. Call on AI stream chunks. */
  keepAlive: () => void;
  wakeWordThreshold: Ref<number>;
  diagnosticsPanelRef: Ref<HTMLElement | null>;
  setWakeWordThreshold: (threshold: number) => void;
  pipelineError: Ref<string>;
  activateWebSpeechFallback: () => void;
  deactivateWebSpeechFallback: () => void;
  webSpeechFallbackActive: Ref<boolean>;
}

// Global worker to avoid reloading
let wakeWordWorker: Worker | null = null;
let isWorkerReady = false;
let detectedCallback: (() => void) | null = null;

const savedThresholdVal = typeof localStorage !== 'undefined' ? localStorage.getItem('liva_wake_threshold') : null;
const wakeWordThreshold = ref(savedThresholdVal ? parseFloat(savedThresholdVal) : 0.15);
const diagnosticsPanelRef = ref<HTMLElement | null>(null);
const pipelineError = ref("");

function initWorker(): Promise<boolean> {
  return new Promise((resolve) => {
    if (wakeWordWorker && isWorkerReady) {
      resolve(true);
      return;
    }

    wakeWordWorker = new Worker(
      new URL('../workers/LivaWakeWorker.ts', import.meta.url),
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
        const saved = typeof localStorage !== 'undefined' ? localStorage.getItem('liva_wake_threshold') : null;
        const initConfig = saved ? { threshold: parseFloat(saved) } : undefined;
        wakeWordWorker?.postMessage({ type: 'init', data: { config: initConfig } });
      } else if (type === 'ready') {
        isWorkerReady = success;
        resolve(success);
      } else if (type === 'detection') {
        const confidence = event.data.confidence ?? 0;
        if (diagnosticsPanelRef.value) {
          diagnosticsPanelRef.value.style.setProperty('--confidence-level', `${confidence * 100}%`);
        }
        if (event.data.detected && detectedCallback) detectedCallback();
      } else if (type === 'thresholdChanged') {
        if (event.data.threshold !== undefined) {
          wakeWordThreshold.value = event.data.threshold;
        }
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
  const webSpeechFallbackActive = ref(false);

  let mediaStream: MediaStream | null = null;
  let audioContext: AudioContext | null = null;
  let processor: ScriptProcessorNode | null = null;
  let source: MediaStreamAudioSourceNode | null = null;
  let wsRef: WebSocket | null = null;

  let recognition: any = null;
  let recognitionShouldRun = false;

  const SpeechRecognitionAPI = (globalThis as any).SpeechRecognition || (globalThis as any).webkitSpeechRecognition;

  function activateWebSpeechFallback() {
    if (webSpeechFallbackActive.value) return;
    logger.warn('[VoicePipeline] Activating Web Speech API fallback.');
    webSpeechFallbackActive.value = true;
    recognitionShouldRun = true;

    if (!SpeechRecognitionAPI) {
      logger.warn('[VoicePipeline] Web Speech API is not supported in this browser.');
      return;
    }

    initSpeechRecognition();
  }

  function deactivateWebSpeechFallback() {
    if (!webSpeechFallbackActive.value) return;
    logger.info('[VoicePipeline] Deactivating Web Speech API fallback.');
    webSpeechFallbackActive.value = false;
    recognitionShouldRun = false;
    if (recognition) {
      try {
        recognition.stop();
      } catch (err) {
        // ignore
      }
      recognition = null;
    }
  }

  function initSpeechRecognition() {
    if (recognition) {
      try {
        recognition.stop();
      } catch (e) {}
    }

    try {
      recognition = new SpeechRecognitionAPI();
      recognition.lang = 'vi-VN';
      recognition.continuous = true;
      recognition.interimResults = false;

      recognition.onresult = (event: any) => {
        if (state.value !== 'ACTIVE' && state.value !== 'PROCESSING') {
          return;
        }

        const lastResultIndex = event.resultIndex;
        const result = event.results[lastResultIndex];
        if (result && result.isFinal) {
          const text = result[0].transcript.trim();
          if (text) {
            logger.info('[VoicePipeline] Web Speech transcript received:', text);
            if (wsRef && wsRef.readyState === WebSocket.OPEN) {
              const packed = pack({ event: 'web_speech_transcription', payload: { text } });
              const msg = new Uint8Array(1 + packed.byteLength);
              msg[0] = 0x02; // MessagePack event
              msg.set(new Uint8Array(packed), 1);
              wsRef.send(msg);
            }
          }
        }
      };

      recognition.onerror = (event: any) => {
        logger.error('[VoicePipeline] Speech recognition error:', event.error);
      };

      recognition.onend = () => {
        logger.info('[VoicePipeline] Speech recognition ended.');
        if (recognitionShouldRun && (state.value === 'ACTIVE' || state.value === 'PROCESSING')) {
          logger.info('[VoicePipeline] Restarting Speech Recognition...');
          try {
            recognition.start();
          } catch (err) {
            logger.error('[VoicePipeline] Failed to restart Speech Recognition:', err);
          }
        }
      };

      if (state.value === 'ACTIVE' || state.value === 'PROCESSING') {
        recognition.start();
      }
    } catch (err) {
      logger.error('[VoicePipeline] Failed to initialize Speech Recognition:', err);
    }
  }

  watch(state, (newState) => {
    if (!webSpeechFallbackActive.value || !recognition) return;
    if (newState === 'ACTIVE' || newState === 'PROCESSING') {
      try {
        recognition.start();
      } catch (e) {
        // ignore already started
      }
    } else {
      try {
        recognition.stop();
      } catch (e) {
        // ignore
      }
    }
  });
  
  let analyser: AnalyserNode | null = null;
  let volumeBuffer: Uint8Array | null = null;
  let volumeRAF: number | null = null;

  let activeTimeoutId: any = null;
  const SILENCE_THRESHOLD = 0.02;

  function onWakeWordDetected(cb: () => void) {
    detectedCallback = () => {
      if (state.value === 'PASSIVE') {
        state.value = 'ACTIVE';
        resetActiveTimeout();
        cb();
        // Notify backend for analytics/logging
        if (wsRef && wsRef.readyState === WebSocket.OPEN) {
          const packed = pack({ event: 'wake_word_triggered', payload: {} });
          const msg = new Uint8Array(1 + packed.byteLength);
          msg[0] = 0x02; // MessagePack event
          msg.set(new Uint8Array(packed), 1);
          wsRef.send(msg);
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
    pipelineError.value = "";
    
    if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
      const errStr = 'getUserMedia not supported';
      logger.error('[VoicePipeline]', errStr);
      pipelineError.value = errStr;
      return;
    }

    try {
      const workerReady = await initWorker();
      if (!workerReady) {
        const errStr = 'Failed to initialize ONNX worker';
        logger.error('[VoicePipeline]', errStr);
        pipelineError.value = errStr;
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

        // 1. Send to WakeWordWorker ONLY in PASSIVE state to prevent self-wake feedback loop and save CPU
        if (state.value === 'PASSIVE' && rms > 0.002) {
          sendToWorker('audio', { audio: Array.from(inputData) });
        }

        // 2. VALVE: Send to WebSocket if ACTIVE or PROCESSING (Full-Duplex Barge-in)
        if ((state.value === 'ACTIVE' || state.value === 'PROCESSING') && wsRef && wsRef.readyState === WebSocket.OPEN) {
          const buffer = new Float32Array(inputData.length);
          buffer.set(inputData);
          
          // Prepend 0x01 header to raw PCM audio chunk (Audio Buffer Slicing optimization)
          const pcmBuffer = buffer.buffer;
          const msg = new Uint8Array(1 + pcmBuffer.byteLength);
          msg[0] = 0x01; // Audio header
          msg.set(new Uint8Array(pcmBuffer), 1);
          wsRef.send(msg);

          if (rms >= SILENCE_THRESHOLD) {
            resetActiveTimeout(); // Keeps session alive while speaking
          }
        }
      };

      source.connect(analyser);
      analyser.connect(processor);
      processor.connect(audioContext.destination);

      if (audioContext.state === 'suspended') {
        audioContext.resume().catch(() => {});
      }

      // Autoplay / Interaction Guard: Resume AudioContext on user click or keydown
      const resumeContext = () => {
        if (audioContext && audioContext.state === 'suspended') {
          audioContext.resume().then(() => {
            logger.info('[VoicePipeline]', 'AudioContext resumed successfully via user interaction.');
            cleanup();
          }).catch(e => logger.warn('[VoicePipeline]', 'Failed to resume AudioContext:', e));
        } else {
          cleanup();
        }
      };
      const cleanup = () => {
        globalThis.document?.removeEventListener('click', resumeContext);
        globalThis.document?.removeEventListener('keydown', resumeContext);
      };
      globalThis.document?.addEventListener('click', resumeContext);
      globalThis.document?.addEventListener('keydown', resumeContext);

      state.value = 'PASSIVE';
      isReady.value = true;
      monitorVolume();
      logger.info('[VoicePipeline]', 'Started 24/7 Omni-Duplex Pipeline');

    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
      logger.error('[VoicePipeline]', 'Failed to start:', errMsg);
      pipelineError.value = errMsg;
      state.value = 'OFF';
      isReady.value = false;
      throw err;
    }
  }

  function monitorVolume() {
    if (state.value === 'OFF' || !analyser || !volumeBuffer) {
      if (volumeRAF !== null) { cancelAnimationFrame(volumeRAF); volumeRAF = null; }
      return;
    }

    analyser.getByteFrequencyData(volumeBuffer as any);
    let sum = 0;
    for (let i = 0; i < volumeBuffer.length; i++) {
      sum += volumeBuffer[i];
    }
    const avg = sum / volumeBuffer.length / 255;
    
    // shallowRef: update value without deep reactive proxy overhead
    volumeLevel.value = avg;
    triggerRef(volumeLevel);

    if (diagnosticsPanelRef.value) {
      diagnosticsPanelRef.value.style.setProperty('--rms-level', `${avg * 100}%`);
    }

    volumeRAF = requestAnimationFrame(monitorVolume);
  }

  async function stopPipeline() {
    state.value = 'OFF';
    isReady.value = false;
    pipelineError.value = "";

    if (diagnosticsPanelRef.value) {
      diagnosticsPanelRef.value.style.setProperty('--rms-level', '0%');
      diagnosticsPanelRef.value.style.setProperty('--confidence-level', '0%');
    }

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

  /**
   * [v26] Transition ACTIVE → PROCESSING when AI starts thinking.
   * Resets the inactivity timeout to keep the pipeline alive during AI processing.
   */
  function setProcessing() {
    if (state.value === 'ACTIVE') {
      state.value = 'PROCESSING';
      resetActiveTimeout();
    }
  }

  /**
   * [v26] Transition ACTIVE/PROCESSING → PASSIVE when the conversation turn is done.
   * Clears the inactivity timeout.
   */
  function setPassive() {
    if (state.value === 'PROCESSING' || state.value === 'ACTIVE') {
      state.value = 'PASSIVE';
      if (activeTimeoutId) {
        clearTimeout(activeTimeoutId);
        activeTimeoutId = null;
      }
    }
  }

  /**
   * [v26] Reset the 15s inactivity timeout without changing state.
   * Call this on AI stream chunks to keep the pipeline alive during long responses.
   */
  function keepAlive() {
    if (state.value === 'ACTIVE' || state.value === 'PROCESSING') {
      resetActiveTimeout();
    }
  }

  function setWakeWordThreshold(newThreshold: number) {
    wakeWordThreshold.value = newThreshold;
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('liva_wake_threshold', newThreshold.toString());
    }
    sendToWorker('setThreshold', { threshold: newThreshold });
  }

  return {
    state,
    volumeLevel,
    isReady,
    startPipeline,
    stopPipeline,
    toggleVoice,
    onWakeWordDetected,
    setProcessing,
    setPassive,
    keepAlive,
    wakeWordThreshold,
    diagnosticsPanelRef,
    setWakeWordThreshold,
    pipelineError,
    activateWebSpeechFallback,
    deactivateWebSpeechFallback,
    webSpeechFallbackActive
  };
}

// [Optimization 2.3] Pre-Warm Wake Word Worker
// Khởi tạo Worker ngay khi module được load vào trình duyệt thay vì đợi user click mic
// Điều này giúp loại bỏ hoàn toàn độ trễ khởi động khi bật Voice Pipeline
if (typeof window !== 'undefined' && typeof Worker !== 'undefined') {
  initWorker().catch(err => {
    logger.warn('[VoicePipeline] Pre-warming failed:', err);
  });
}

