import { ref, shallowRef, triggerRef, watch, type Ref, onUnmounted } from "vue";
import { logger } from "../utils/logger";
import { pack } from "msgpackr";
import { serializeVoiceFrame, OP_MIC_IN, OP_WAKE_PROBE } from "../utils/voiceFrame";

export function wakeConfidencePercent(score: unknown): string {
  if (typeof score !== "number" || !Number.isFinite(score)) return "0.0%";
  return `${(Math.min(1, Math.max(0, score)) * 100).toFixed(1)}%`;
}

/**
 * Vì sao pipeline không lên được. Tách "môi trường không cho" (quyền mic bị chặn,
 * máy không có mic, webview thiếu getUserMedia) khỏi "hỏng thật": nhóm đầu là
 * trạng thái hợp lệ của một máy không mic hoặc một webview sandbox — không ném
 * lỗi, không log `error`, và UI báo bằng giọng khác.
 */
export type VoicePipelineErrorKind =
  | 'none'
  | 'permission'
  | 'no-device'
  | 'busy'
  | 'unsupported'
  | 'failure';

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
  /** Loa bắt đầu phát TTS — ngưng nạp mic cho bộ wake-word (chống tự nghe). */
  muteWakeWord: () => void;
  /** Loa dứt — mở lại sau đuôi vọng và xoá cửa sổ trượt đang lẫn tiếng loa. */
  unmuteWakeWord: () => void;
  /** Chặn bộ wake-word trong `ms` tới (âm hiệu ngắn tự phát, ví dụ chime). */
  muteWakeWordFor: (ms: number) => void;
  pipelineError: Ref<string>;
  /** Vì sao mic không dùng được — để UI phân biệt "chưa cấp quyền" với "hỏng". */
  pipelineErrorKind: Ref<VoicePipelineErrorKind>;
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

/**
 * Sàn RMS để LivaWakeWorker coi là "đang có người nói" — xem module docs của
 * worker. Khoá localStorage **mới**: giá trị của khoá cũ (`liva_wake_threshold`)
 * là confidence 0–1 của bộ MLP đã bỏ, mặc định 0.15. Đem 0.15 dùng làm sàn RMS
 * thì gần như không câu nói nào vượt nổi, tức bản vá này sẽ im lặng không bao
 * giờ đánh thức trên đúng những máy đã từng chỉnh núm đó.
 */
const WAKE_FLOOR_STORAGE_KEY = 'liva_wake_speech_floor';
const savedFloorVal = typeof localStorage !== 'undefined' ? localStorage.getItem(WAKE_FLOOR_STORAGE_KEY) : null;
const wakeWordThreshold = ref(savedFloorVal ? parseFloat(savedFloorVal) : 0.015);

// ═══════════════════════════════════════════════════════
//  Chống tự nghe (self-wake)
//  LivaWakeWorker chỉ nhìn 16 giá trị RMS energy (extractFeatures), nên nó không
//  phân biệt được "Hey Liva" với bất kỳ tiếng nói nào — giọng TTS của chính LIVA
//  vọng từ loa vào mic là đủ vượt ngưỡng. Mỗi lần vượt là một lần widget nhảy
//  sang ACTIVE, in "Dạ, Liva nghe đây..." và bắt đầu bắn khung mic lên core, tức
//  core có thể STT chính giọng LIVA thành một lượt người dùng giả.
//  `echoCancellation` của getUserMedia không phủ được đường ra của AudioContext
//  trong webview Tauri, nên phải tự chặn ở phía nạp dữ liệu.
// ═══════════════════════════════════════════════════════
/** Loa TTS đang phát; bật/tắt theo cặp onPlaybackStarted/onPlaybackFinished. */
let speakerActive = false;
/** Mốc chặn tuyệt đối: đuôi vọng sau khi loa dứt, và các âm hiệu ngắn tự phát. */
let wakeWordMutedUntil = 0;
/** Loa dứt rồi phòng vẫn còn tiếng vọng — giữ chặn thêm một nhịp. */
const WAKE_WORD_ECHO_TAIL_MS = 400;

/**
 * Dùng mốc thời gian thay vì refcount: một lần `unmute` bị bỏ sót sẽ tự hết hạn,
 * chứ không khoá chết bộ wake-word tới lần reload sau.
 */
function isWakeWordMuted(): boolean {
  return speakerActive || Date.now() < wakeWordMutedUntil;
}
const diagnosticsPanelRef = ref<HTMLElement | null>(null);
const pipelineError = ref("");
const pipelineErrorKind = ref<VoicePipelineErrorKind>('none');

interface MicFailure {
  kind: VoicePipelineErrorKind;
  message: string;
}

/**
 * `getUserMedia` phân biệt các ca hỏng qua `name` của DOMException, không qua
 * `message` — `message` khác nhau theo từng trình duyệt và có ca rỗng hẳn, nên
 * dội thẳng nó ra màn hình thì người dùng đọc được đúng một chuỗi tiếng Anh cụt.
 */
function classifyMicError(err: unknown): MicFailure {
  const name = typeof err === 'object' && err !== null && 'name' in err
    ? String((err as { name: unknown }).name)
    : '';

  switch (name) {
    case 'NotAllowedError':
    case 'PermissionDeniedError': // tên cũ, còn gặp ở webview nhân Chromium cũ
      return { kind: 'permission', message: 'Micro chưa được cấp quyền, hoặc bị trình duyệt/hệ điều hành chặn.' };
    case 'SecurityError':
      return { kind: 'permission', message: 'Trang không chạy trong ngữ cảnh bảo mật (https hoặc localhost) nên không xin được micro.' };
    case 'NotFoundError':
    case 'DevicesNotFoundError':
      return { kind: 'no-device', message: 'Không tìm thấy thiết bị micro nào trên máy.' };
    case 'OverconstrainedError':
    case 'ConstraintNotSatisfiedError':
      return { kind: 'no-device', message: 'Không micro nào đáp ứng được cấu hình thu (1 kênh, 16 kHz).' };
    case 'NotReadableError':
    case 'TrackStartError':
    case 'AbortError':
      return { kind: 'busy', message: 'Micro đang bị ứng dụng khác giữ nên không mở được.' };
    default:
      return { kind: 'failure', message: err instanceof Error ? err.message : String(err) };
  }
}

/**
 * Hỏi trạng thái quyền trước khi gọi `getUserMedia`. Khi quyền đã bị từ chối thì
 * `getUserMedia` vẫn phải chạy tới nơi rồi mới ném — và mỗi lần chạy là một lần
 * trình duyệt (hoặc webview nhúng, ví dụ Browser pane của Claude Code) dựng banner
 * "trang này xin quyền micro". Hỏi trước thì bỏ hẳn được lần dựng banner đó.
 * Không nền tảng nào cũng có Permissions API cho 'microphone' — thiếu thì coi như
 * chưa biết và cứ thử như cũ.
 */
async function isMicPermissionDenied(): Promise<boolean> {
  try {
    const status = await navigator.permissions?.query({ name: 'microphone' as PermissionName });
    return status?.state === 'denied';
  } catch {
    return false;
  }
}

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
        const saved = typeof localStorage !== 'undefined' ? localStorage.getItem(WAKE_FLOOR_STORAGE_KEY) : null;
        const initConfig = saved ? { speechFloor: parseFloat(saved) } : undefined;
        wakeWordWorker?.postMessage({ type: 'init', data: { config: initConfig } });
      } else if (type === 'ready') {
        complete(Boolean(success));
      } else if (type === 'candidate') {
        // Worker vừa cắt được một cụm ngắn. Nó KHÔNG biết cụm đó là gì — hỏi
        // core. UI chỉ chuyển ACTIVE khi core trả `wake_word_triggered`.
        sendWakeProbe(new Float32Array(event.data.audio));
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

function sendToWorker(type: string, data?: unknown, transfer?: Transferable[]) {
  if (wakeWordWorker) {
    wakeWordWorker.postMessage({ type, data }, transfer ?? []);
  }
}

/**
 * Cửa gửi `OP_WAKE_PROBE`. `initWorker`/`sendWakeProbe` ở phạm vi module (worker
 * là singleton, khớp thiết kế sẵn có) nhưng `wsRef` nằm trong composable, nên
 * composable đăng ký hàm gửi thật vào đây khi nó dựng xong.
 */
let wakeProbeSender: ((audio: Float32Array) => void) | null = null;

function sendWakeProbe(audio: Float32Array) {
  if (!wakeProbeSender) {
    logger.warn('[WakeWord]', 'Có cụm ứng viên nhưng chưa có kết nối core để xác minh — bỏ qua.');
    return;
  }
  wakeProbeSender(audio);
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

  /** seqId cho khung OP_WAKE_PROBE — đếm riêng với micSeqId. */
  let wakeProbeSeqId = 0;

  wakeProbeSender = (audio: Float32Array) => {
    if (!wsRef || wsRef.readyState !== WebSocket.OPEN) {
      logger.warn('[WakeWord]', 'Socket core chưa mở — không xác minh được cụm ứng viên.');
      return;
    }
    wsRef.send(serializeVoiceFrame(
      OP_WAKE_PROBE,
      wakeProbeSeqId,
      new Uint8Array(audio.buffer, audio.byteOffset, audio.byteLength),
    ));
    wakeProbeSeqId = (wakeProbeSeqId + 1) >>> 0;
  };

  /**
   * Phán quyết đánh thức đến từ core, không phải từ trình duyệt. Gắn bằng
   * `addEventListener` chứ không phải `onmessage` — WidgetApp đã chiếm
   * `socket.onmessage`, gán đè sẽ làm câm toàn bộ luồng sự kiện của nó.
   */
  const onCoreMessage = (event: MessageEvent) => {
    if (typeof event.data !== 'string') return;
    let parsed: { event?: string; payload?: { transcript?: string; score?: number } };
    try {
      parsed = JSON.parse(event.data);
    } catch {
      return;
    }

    if (parsed.event === 'wake_word_triggered') {
      logger.info('[WakeWord]', 'Core xác nhận cụm đánh thức:', parsed.payload?.transcript ?? '');
      if (diagnosticsPanelRef.value) {
        diagnosticsPanelRef.value.style.setProperty(
          '--confidence-level',
          wakeConfidencePercent(parsed.payload?.score),
        );
      }
      detectedCallback?.();
    } else if (parsed.event === 'wake_probe_rejected') {
      // Không phải lỗi — đây là cổng đang làm đúng việc của nó. Nhưng phải log
      // ra transcript, vì "sao nó không thức" là câu hỏi không thể trả lời nếu
      // không biết core nghe ra cái gì.
      logger.info('[WakeWord]', 'Bỏ qua, không phải cụm đánh thức. Nghe ra:', parsed.payload?.transcript || '(không ra chữ)');
      if (diagnosticsPanelRef.value) {
        diagnosticsPanelRef.value.style.setProperty(
          '--confidence-level',
          wakeConfidencePercent(parsed.payload?.score),
        );
      }
    }
  };

  /** Socket đang gắn `onCoreMessage`, để gỡ đúng cái đã gắn. */
  let coreMessageSocket: WebSocket | null = null;

  function attachCoreListener(ws: WebSocket) {
    if (coreMessageSocket === ws) return;
    detachCoreListener();
    // Socket giả trong test (và mọi adapter không đủ EventTarget) không có
    // addEventListener; thiếu nó chỉ mất phán quyết đánh thức, không được phép
    // làm hỏng cả việc dựng pipeline.
    if (typeof ws?.addEventListener !== 'function') {
      logger.warn('[WakeWord]', 'Socket không hỗ trợ addEventListener — bỏ qua kênh xác minh wake word.');
      return;
    }
    ws.addEventListener('message', onCoreMessage);
    coreMessageSocket = ws;
  }

  function detachCoreListener() {
    if (typeof coreMessageSocket?.removeEventListener === 'function') {
      coreMessageSocket.removeEventListener('message', onCoreMessage);
    }
    coreMessageSocket = null;
  }

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
    attachCoreListener(ws);
    pipelineError.value = "";
    pipelineErrorKind.value = 'none';

    if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
      const errStr = 'Trình duyệt/webview này không mở được micro (thiếu getUserMedia).';
      logger.warn('[VoicePipeline]', 'Mic unavailable: unsupported —', errStr);
      pipelineError.value = errStr;
      pipelineErrorKind.value = 'unsupported';
      return;
    }

    if (await isMicPermissionDenied()) {
      if (generation !== lifecycleGeneration) return;
      const denied = classifyMicError({ name: 'NotAllowedError' });
      logger.warn('[VoicePipeline]', 'Mic unavailable: permission —', denied.message);
      pipelineError.value = denied.message;
      pipelineErrorKind.value = denied.kind;
      wsRef = null;
      return;
    }

    /** Đặt khi chính `getUserMedia` ném, để catch dưới không đoán nhầm lỗi của worklet. */
    let micFailure: MicFailure | null = null;

    try {
      const workerReady = await initWorker();
      if (generation !== lifecycleGeneration) return;
      if (!workerReady) {
        const errStr = 'Failed to initialize ONNX worker';
        logger.error('[VoicePipeline]', errStr);
        pipelineError.value = errStr;
        pipelineErrorKind.value = 'failure';
        return;
      }

      try {
        mediaStream = await navigator.mediaDevices.getUserMedia({
          audio: {
            channelCount: 1,
            sampleRate: { ideal: 16000 },
            echoCancellation: true,
            noiseSuppression: true,
            autoGainControl: true,
          },
        });
      } catch (err: unknown) {
        micFailure = classifyMicError(err);
        throw err; // catch ngoài lo phần dọn tài nguyên, ở một chỗ duy nhất
      }

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

        // 1. Nạp bộ cắt câu, CHỈ khi PASSIVE — tránh vòng lặp tự nghe và đỡ CPU.
        //    Cổng PASSIVE một mình là không đủ: khi người dùng *gõ* chat thì state
        //    ở PASSIVE suốt lúc LIVA đọc câu trả lời, nên tiếng loa vọng vào mic
        //    đi thẳng vào bộ cắt. isWakeWordMuted() là cổng thứ hai cho ca đó.
        //
        //    KHÔNG lọc theo `rms` ở đây. Bộ cắt câu nhận ra "đã dứt câu" bằng
        //    cách đếm các khung LIÊN TIẾP dưới sàn — chặn khung im lặng lại thì
        //    bộ đếm đó không bao giờ chạy, câu không bao giờ đóng, và không một
        //    cụm ứng viên nào được phát ra. Bản cũ lọc được vì nó suy luận trên
        //    từng khung độc lập, không có khái niệm biên câu.
        if (state.value === 'PASSIVE' && !isWakeWordMuted()) {
          // Sao chép rồi chuyển quyền sở hữu bản sao: `inputData` là buffer của
          // worklet, transfer thẳng nó sẽ tháo mất buffer khỏi nhánh OP_MIC_IN
          // bên dưới.
          const frame = new Float32Array(inputData);
          sendToWorker('audio', { audio: frame.buffer }, [frame.buffer]);
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
      const failure: MicFailure = micFailure
        ?? { kind: 'failure', message: err instanceof Error ? err.message : String(err) };

      await releaseAudioResources();
      wsRef = null;
      pipelineError.value = failure.message;
      pipelineErrorKind.value = failure.kind;
      state.value = 'OFF';
      isReady.value = false;

      if (failure.kind === 'failure') {
        logger.error('[VoicePipeline]', 'Failed to start:', failure.message);
        throw err;
      }

      // Máy không có mic, hoặc webview không cho mở mic, là trạng thái hợp lệ —
      // không phải sự cố cần ai đó bắt. Ném ở đây chỉ tạo ra một rejection mà mọi
      // call site đều phải nuốt, và một dòng đỏ trong console cho chuyện bình thường.
      logger.warn('[VoicePipeline]', `Mic unavailable: ${failure.kind} —`, failure.message);
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
    pipelineErrorKind.value = 'none';

    if (diagnosticsPanelRef.value) {
      diagnosticsPanelRef.value.style.setProperty('--rms-level', '0%');
      diagnosticsPanelRef.value.style.setProperty('--confidence-level', '0%');
    }

    if (activeTimeoutId) {
      clearTimeout(activeTimeoutId);
      activeTimeoutId = null;
    }

    // Trạng thái chặn tự-nghe là module-scope: pipeline mới không nên thừa hưởng
    // một lần mute còn treo của lần chạy trước.
    speakerActive = false;
    wakeWordMutedUntil = 0;

    await releaseAudioResources();

    if (wakeWordWorker) {
      settleWorkerInit?.(false);
      wakeWordWorker?.terminate();
      wakeWordWorker = null;
      isWorkerReady = false;
    }

    detachCoreListener();
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

  /** Loa bắt đầu phát TTS — ngưng nạp mic cho bộ wake-word. */
  function muteWakeWord() {
    speakerActive = true;
  }

  /**
   * Loa dứt — mở lại sau đuôi vọng. `reset` xoá cửa sổ trượt trong worker: nếu
   * giữ lại, lúc mở chắn nó sẽ suy luận trên đoạn ghép rời rạc (trước loa + sau
   * loa) và cái bậc năng lượng đó lại rất giống một cụm wake-word.
   */
  function unmuteWakeWord() {
    speakerActive = false;
    wakeWordMutedUntil = Date.now() + WAKE_WORD_ECHO_TAIL_MS;
    sendToWorker('reset');
  }

  /** Chặn bộ wake-word trong `ms` tới; chỉ nới dài, không bao giờ rút ngắn. */
  function muteWakeWordFor(ms: number) {
    wakeWordMutedUntil = Math.max(wakeWordMutedUntil, Date.now() + ms);
    sendToWorker('reset');
  }

  /** Đặt sàn RMS của bộ cắt câu (xem `WAKE_FLOOR_STORAGE_KEY`). */
  function setWakeWordThreshold(newThreshold: number) {
    wakeWordThreshold.value = newThreshold;
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(WAKE_FLOOR_STORAGE_KEY, newThreshold.toString());
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
    muteWakeWord,
    unmuteWakeWord,
    muteWakeWordFor,
    pipelineError,
    pipelineErrorKind,
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

