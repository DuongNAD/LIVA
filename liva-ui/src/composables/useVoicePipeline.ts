import { ref, shallowRef, triggerRef, watch, type Ref, onUnmounted } from "vue";
import { logger } from "../utils/logger";
import { pack } from "msgpackr";
import { serializeVoiceFrame, OP_MIC_IN } from "../utils/voiceFrame";

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
let workerInitPromise: Promise<boolean> | null = null;
let settleWorkerInit: ((ready: boolean) => void) | null = null;
let detectedCallback: (() => void) | null = null;

const savedThresholdVal = typeof localStorage !== 'undefined' ? localStorage.getItem('liva_wake_threshold') : null;
const wakeWordThreshold = ref(savedThresholdVal ? parseFloat(savedThresholdVal) : 0.15);
const diagnosticsPanelRef = ref<HTMLElement | null>(null);
const pipelineError = ref("");

function initWorker(): Promise<boolean> {
  if (wakeWordWorker && isWorkerReady) {
    return Promise.resolve(true);
  }
  if (workerInitPromise) {
    return workerInitPromise;
  }

  workerInitPromise = new Promise((resolve) => {
    let settled = false;
    const complete = (ready: boolean) => {
      if (settled) return;
      settled = true;
      isWorkerReady = ready;
      settleWorkerInit = null;
      workerInitPromise = null;
      if (!ready && wakeWordWorker) {
        wakeWordWorker.terminate();
        wakeWordWorker = null;
      }
      resolve(ready);
    };
    settleWorkerInit = complete;
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
        complete(Boolean(success));
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
      complete(false);
    };
  });
  return workerInitPromise;
}

function sendToWorker(type: string, data?: unknown) {
  if (wakeWordWorker) {
    wakeWordWorker.postMessage({ type, data });
  }
}

interface SpeechRecognitionResult {
  isFinal: boolean;
  [index: number]: { transcript: string };
}
interface SpeechRecognitionEvent {
  resultIndex: number;
  results: {
    [index: number]: SpeechRecognitionResult;
  };
}
interface SpeechRecognitionErrorEvent {
  error: string;
}
interface SpeechRecognitionInstance {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  onresult: (event: SpeechRecognitionEvent) => void;
  onerror: (event: SpeechRecognitionErrorEvent) => void;
  onend: () => void;
  start: () => void;
  stop: () => void;
}

export function useVoicePipeline(): UseVoicePipelineReturn {
  const state = ref<'OFF' | 'PASSIVE' | 'ACTIVE' | 'PROCESSING'>('OFF');
  const volumeLevel = shallowRef<number>(0);
  const isReady = ref(false);
  const webSpeechFallbackActive = ref(false);

  let mediaStream: MediaStream | null = null;
  let audioContext: AudioContext | null = null;
  let processor: AudioWorkletNode | null = null;
  let source: MediaStreamAudioSourceNode | null = null;
  let wsRef: WebSocket | null = null;

  let recognition: SpeechRecognitionInstance | null = null;
  let recognitionShouldRun = false;
  let cleanupInteractionGuard: (() => void) | null = null;
  let startInFlight: Promise<void> | null = null;
  let lifecycleGeneration = 0;

  const SpeechRecognitionAPI = ((globalThis as unknown as Record<string, unknown>).SpeechRecognition || 
    (globalThis as unknown as Record<string, unknown>).webkitSpeechRecognition) as { new(): SpeechRecognitionInstance } | undefined;

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
      } catch {
        // ignore
      }
      recognition = null;
    }
  }

  function initSpeechRecognition() {
    if (recognition) {
      try {
        recognition.stop();
      } catch {
        // ignore
      }
    }

    try {
      if (!SpeechRecognitionAPI) return;
      recognition = new SpeechRecognitionAPI();
      recognition.lang = 'vi-VN';
      recognition.continuous = true;
      recognition.interimResults = false;

      recognition.onresult = (event: SpeechRecognitionEvent) => {
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

      recognition.onerror = (event: SpeechRecognitionErrorEvent) => {
        logger.error('[VoicePipeline] Speech recognition error:', event.error);
      };

      recognition.onend = () => {
        logger.info('[VoicePipeline] Speech recognition ended.');
        if (recognitionShouldRun && (state.value === 'ACTIVE' || state.value === 'PROCESSING')) {
          logger.info('[VoicePipeline] Restarting Speech Recognition...');
          try {
            if (recognition) {
              recognition.start();
            }
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
      } catch {
        // ignore already started
      }
    } else {
      try {
        recognition.stop();
      } catch {
        // ignore
      }
    }
  });
  
  let analyser: AnalyserNode | null = null;
  let volumeBuffer: Uint8Array | null = null;
  let volumeRAF: number | null = null;
  /** seqId cho khung OP_MIC_IN; core dùng nó để xếp thứ tự gói. Quấn vòng u32. */
  let micSeqId = 0;

  let activeTimeoutId: ReturnType<typeof setTimeout> | null = null;
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

  function startPipeline(ws: WebSocket): Promise<void> {
    if (state.value !== 'OFF') return Promise.resolve();
    if (startInFlight) return startInFlight;

    const pending = startPipelineOnce(ws, lifecycleGeneration);
    startInFlight = pending;
    pending.then(
      () => {
        if (startInFlight === pending) startInFlight = null;
      },
      () => {
        if (startInFlight === pending) startInFlight = null;
      },
    );
    return pending;
  }

  async function startPipelineOnce(ws: WebSocket, generation: number) {
    
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
      if (generation !== lifecycleGeneration) return;
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

      const AudioCtx = globalThis.AudioContext || (globalThis as unknown as Record<string, unknown>).webkitAudioContext as typeof AudioContext;
      audioContext = new AudioCtx({ sampleRate: 16000 });
      source = audioContext.createMediaStreamSource(mediaStream);

      analyser = audioContext.createAnalyser();
      analyser.fftSize = 256;
      volumeBuffer = new Uint8Array(analyser.frequencyBinCount);

      await audioContext.audioWorklet.addModule(
        new URL('../worklets/mic-capture.worklet.js', import.meta.url),
      );
      if (generation !== lifecycleGeneration) {
        await releaseAudioResources();
        wsRef = null;
        return;
      }
      processor = new AudioWorkletNode(audioContext, 'liva-mic-capture', {
        numberOfInputs: 1,
        numberOfOutputs: 1,
        outputChannelCount: [1],
        processorOptions: {
          frameSize: 512, // 32 ms at 16 kHz
        },
      });
      if (generation !== lifecycleGeneration) {
        mediaStream.getTracks().forEach(track => track.stop());
        mediaStream = null;
        wsRef = null;
        return;
      }

      processor.port.onmessage = (event: MessageEvent<Float32Array>) => {
        if (state.value === 'OFF') return;

        const inputData = event.data;

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
          // Hợp đồng VoiceFrame: header 9 byte (op, seq LE, len LE) + payload
          // PCM f32 LE. Trước đây chỗ này chỉ ghi 1 byte header, nên core đọc
          // 4 byte PCM đầu làm seqId và 4 byte kế làm payloadSize — ra số rác
          // thường vượt 1 MiB ⇒ mọi khung mic bị từ chối và barge-in từ trình
          // duyệt không thể hoạt động.
          wsRef.send(serializeVoiceFrame(
            OP_MIC_IN,
            micSeqId,
            new Uint8Array(inputData.buffer, inputData.byteOffset, inputData.byteLength),
          ));
          micSeqId = (micSeqId + 1) >>> 0;

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

      // Clear any prior interaction guard
      if (cleanupInteractionGuard) {
        cleanupInteractionGuard();
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
        cleanupInteractionGuard = null;
      };
      cleanupInteractionGuard = cleanup;
      globalThis.document?.addEventListener('click', resumeContext);
      globalThis.document?.addEventListener('keydown', resumeContext);

      if (generation !== lifecycleGeneration) {
        await releaseAudioResources();
        wsRef = null;
        return;
      }
      state.value = 'PASSIVE';
      isReady.value = true;
      monitorVolume();
      logger.info('[VoicePipeline]', 'Started 24/7 Omni-Duplex Pipeline');

    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
      logger.error('[VoicePipeline]', 'Failed to start:', errMsg);
      await releaseAudioResources();
      wsRef = null;
      pipelineError.value = errMsg;
      state.value = 'OFF';
      isReady.value = false;
      throw err;
    }
  }

  async function releaseAudioResources() {
    if (processor) {
      processor.port.onmessage = null;
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
      const context = audioContext;
      audioContext = null;
      await context.close();
    }

    if (mediaStream) {
      mediaStream.getTracks().forEach(t => t.stop());
      mediaStream = null;
    }

    if (cleanupInteractionGuard) {
      cleanupInteractionGuard();
    }
    if (volumeRAF !== null) {
      cancelAnimationFrame(volumeRAF);
      volumeRAF = null;
    }
  }

  function monitorVolume() {
    if (state.value === 'OFF' || !analyser || !volumeBuffer) {
      if (volumeRAF !== null) { cancelAnimationFrame(volumeRAF); volumeRAF = null; }
      return;
    }

    analyser.getByteFrequencyData(volumeBuffer as unknown as Uint8Array<ArrayBuffer>);
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
    lifecycleGeneration += 1;
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

    await releaseAudioResources();

    if (wakeWordWorker) {
      settleWorkerInit?.(false);
      wakeWordWorker?.terminate();
      wakeWordWorker = null;
      isWorkerReady = false;
    }

    wsRef = null;
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

  onUnmounted(() => {
    stopPipeline().catch((err) => {
      logger.error('[VoicePipeline] Unmount cleanup failed:', err);
    });
  });

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

