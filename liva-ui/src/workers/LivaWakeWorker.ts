/**
 * LivaWakeWorker.ts — cổng sơ tuyển cho wake word "Hey Liva"
 * =================================================================================
 *
 * Worker này **không** quyết định có đánh thức hay không. Nó chỉ trả lời một câu
 * rẻ tiền: *"vừa có ai nói một cụm ngắn không?"* — rồi cắt đúng cụm đó gửi ra để
 * core xác minh bằng STT thật (`OP_WAKE_PROBE` → `wake.rs::matches_phrase`).
 *
 * ## Vì sao phải đổi (đọc trước khi định "tối ưu" lại)
 *
 * Bản trước chạy một MLP 16→32→16→1 trên **16 giá trị RMS energy** của 380 ms
 * audio, trọng số sinh từ `scripts/generate_hey_liva_model.py`. Hai vấn đề, cái
 * nào cũng đủ chí mạng:
 *
 * 1. **16 con số RMS là đường cong to/nhỏ.** Không tần số, không formant, không
 *    âm vị. Về mặt thông tin thì "hey Liva" và "xin chào" là cùng một vector.
 *    Không ngưỡng nào tách được chúng, vì không có gì để tách.
 * 2. **Dữ liệu huấn luyện là `np.random.uniform`.** Lớp positive được định nghĩa
 *    là "năng lượng lên rồi giữ" — tức hình dạng của gần như mọi câu nói.
 *
 * Hệ quả: nó nhảy với mọi tiếng nói. Đó là hành vi đúng của thứ nó thực sự là —
 * một bộ dò *có người đang nói*, không phải bộ dò *người đó nói gì*.
 *
 * Nên bản này bỏ hẳn suy diễn giả và làm đúng việc mà năng lượng làm được: cắt
 * câu. Việc nhận dạng nội dung giao cho thứ có khả năng làm việc đó.
 *
 * ## Máy trạng thái
 *
 *   SILENCE ──rms ≥ floor, ≥2 khung──▶ SPEECH ──rms < floor, ≥8 khung──▶ SILENCE
 *                                                        └─▶ phát `candidate`
 *
 * Câu ứng viên gửi ra gồm pre-roll (~256 ms trước khi bắt đầu nói, để không cụt
 * phụ âm đầu "h" trong "hey") + đoạn nói + đuôi.
 */

/**
 * `tsconfig.app.json` không nạp lib `webworker`, nên TS kiểu hoá `self` thành
 * `Window` — mà `Window.postMessage` bắt buộc có `targetOrigin`, không nhận
 * transfer list. Bọc lại đúng chữ ký của `DedicatedWorkerGlobalScope`.
 */
const scope = self as unknown as {
  postMessage(message: unknown, transfer?: Transferable[]): void;
};

function post(message: unknown, transfer?: Transferable[]) {
  // Truyền `undefined` tường minh vẫn là một đối số thứ hai — với `postMessage`
  // thật thì vô hại, nhưng nó rò vào mọi assertion `toHaveBeenCalledWith`.
  if (transfer) scope.postMessage(message, transfer);
  else scope.postMessage(message);
}

function log(level: "info" | "warn" | "error", ...args: unknown[]) {
  post({ type: '__log', level, args });
}

// ============================================================================
// Configuration
// ============================================================================

interface WakeWordConfig {
  /**
   * Sàn RMS để coi một khung là "đang nói". Tiếng nói ở khoảng cách thường là
   * 0.03–0.2; tiếng ồn nền phòng là 0.001–0.005. Đây là **núm duy nhất** người
   * dùng cần chỉnh, và nó là RMS thật — không phải "confidence" của model nào.
   */
  speechFloor: number;
  /** Số khung liên tiếp ≥ sàn thì mới vào trạng thái SPEECH (chống click/gõ phím). */
  onsetFrames: number;
  /** Số khung liên tiếp < sàn thì mới coi là dứt câu. */
  hangoverFrames: number;
  /** Số khung đệm trước điểm bắt đầu nói, gửi kèm câu ứng viên. */
  prerollFrames: number;
  /** Câu ngắn hơn mức này là tiếng động, không phải cụm đánh thức. */
  minUtteranceMs: number;
  /**
   * Chỉ gửi tối đa chừng này mở đầu của câu. Core so cụm đánh thức trong 8 từ
   * đầu, nên phần đuôi không giúp gì mà chỉ tốn một lượt STT dài hơn.
   */
  maxUtteranceMs: number;
  /** Khoảng nghỉ tối thiểu giữa hai lần hỏi core. */
  cooldownMs: number;
  /**
   * Độ dài TỐI THIỂU của clip gửi đi, nới ngược về quá khứ trong vòng đệm cho
   * đủ. Đây không phải con số tuỳ ý — cả hai tầng xác minh của core đều có sàn:
   *
   * - Classifier (`wake_model.rs`) cần 196 mel frame ≈ **1,96 s**, dưới mức đó
   *   `predict_raw` trả rỗng, tức tầng 1 im lặng hoàn toàn.
   * - Nemotron RNN-T cần ≳ **1,3 s** mới ra chữ; đo 2026-07-27: cùng nội dung
   *   ở 0,8 s và 1,0 s ra rỗng, 1,3 s trở lên mới có transcript.
   *
   * Nới bằng audio thật (tiếng ồn nền phòng) chứ không đệm số 0 — ASR cần ngữ
   * cảnh âm thanh, khoảng lặng số hoá không phải là ngữ cảnh.
   */
  minProbeMs: number;
  sampleRate: number;
}

const DEFAULT_CONFIG: WakeWordConfig = {
  speechFloor: 0.015,
  onsetFrames: 2,
  hangoverFrames: 8,
  prerollFrames: 8,
  minUtteranceMs: 250,
  maxUtteranceMs: 3000,
  cooldownMs: 1200,
  minProbeMs: 2300,
  sampleRate: 16000,
};

// ============================================================================
// State
// ============================================================================

let config: WakeWordConfig = { ...DEFAULT_CONFIG };
let lastCandidateTime = 0;
let isReady = false;
let isPaused = false;

/**
 * Vòng đệm ~4 s. Phải chứa được pre-roll + câu dài nhất; 4 s cho biên rộng rãi
 * mà vẫn chỉ 256 KB.
 */
const RING_SAMPLES = 64000;
const ring = new Float32Array(RING_SAMPLES);
/** Tổng số mẫu đã ghi từ trước tới giờ — dùng làm mốc thời gian tuyệt đối. */
let ringWritten = 0;

/** Đang trong một đoạn nói? */
let inSpeech = false;
/** Mốc mẫu tuyệt đối nơi đoạn nói hiện tại bắt đầu. */
let speechStartedAt = 0;
/** Số khung liên tiếp ≥ / < sàn, tuỳ trạng thái. */
let onsetRun = 0;
let silenceRun = 0;

let peakRmsInSecond = 0;
let lastDebugLogTime = 0;

function resetSegmentation() {
  inSpeech = false;
  speechStartedAt = 0;
  onsetRun = 0;
  silenceRun = 0;
}

function writeToRing(audio: Float32Array) {
  const offset = ringWritten % RING_SAMPLES;
  const firstChunk = Math.min(audio.length, RING_SAMPLES - offset);
  ring.set(audio.subarray(0, firstChunk), offset);
  if (firstChunk < audio.length) {
    ring.set(audio.subarray(firstChunk), 0);
  }
  ringWritten += audio.length;
}

/**
 * Đọc lại `[from, to)` theo mốc mẫu tuyệt đối. Trả `null` nếu đoạn đó đã bị
 * vòng đệm ghi đè — thà không hỏi còn hơn hỏi bằng audio rác.
 */
function readRing(from: number, to: number): Float32Array | null {
  if (to > ringWritten || from < 0 || from >= to) return null;
  if (ringWritten - from > RING_SAMPLES) return null;

  const out = new Float32Array(to - from);
  const start = from % RING_SAMPLES;
  const firstChunk = Math.min(out.length, RING_SAMPLES - start);
  out.set(ring.subarray(start, start + firstChunk), 0);
  if (firstChunk < out.length) {
    out.set(ring.subarray(0, out.length - firstChunk), firstChunk);
  }
  return out;
}

function rmsOf(audio: Float32Array): number {
  let sumSquares = 0;
  for (let i = 0; i < audio.length; i++) {
    sumSquares += audio[i] * audio[i];
  }
  return audio.length > 0 ? Math.sqrt(sumSquares / audio.length) : 0;
}

// ============================================================================
// Detection Logic
// ============================================================================

interface WakeCandidate {
  audio: Float32Array;
  /** Độ dài đoạn nói thật (không tính pre-roll) — để log/chẩn đoán. */
  speechMs: number;
  peakRms: number;
}

function processAudioFrame(audioData: Float32Array): WakeCandidate | null {
  if (!isReady || isPaused) return null;

  writeToRing(audioData);

  const rms = rmsOf(audioData);
  if (rms > peakRmsInSecond) peakRmsInSecond = rms;

  const now = Date.now();
  if (now - lastDebugLogTime > 1000) {
    log('info', `[WakeWorker] RMS đỉnh 1 s qua: ${peakRmsInSecond.toFixed(4)} (sàn ${config.speechFloor})`);
    peakRmsInSecond = 0;
    lastDebugLogTime = now;
  }

  const speaking = rms >= config.speechFloor;

  if (!inSpeech) {
    onsetRun = speaking ? onsetRun + 1 : 0;
    if (onsetRun >= config.onsetFrames) {
      inSpeech = true;
      silenceRun = 0;
      // Lùi lại đúng số khung đã dùng để xác nhận onset, nếu không thì phụ âm
      // đầu ("h" trong "hey") nằm ngoài đoạn cắt.
      speechStartedAt = ringWritten - audioData.length * config.onsetFrames;
    }
    return null;
  }

  silenceRun = speaking ? 0 : silenceRun + 1;
  if (silenceRun < config.hangoverFrames) return null;

  // ── Dứt câu ── (chốt mốc TRƯỚC khi reset, reset xoá `speechStartedAt`)
  const segmentStart = speechStartedAt;
  const segmentEnd = ringWritten - audioData.length * config.hangoverFrames;
  const speechMs = ((segmentEnd - segmentStart) / config.sampleRate) * 1000;
  resetSegmentation();

  if (speechMs < config.minUtteranceMs) return null;
  if (now - lastCandidateTime < config.cooldownMs) return null;

  const preroll = audioData.length * config.prerollFrames;
  const maxSamples = Math.floor((config.maxUtteranceMs / 1000) * config.sampleRate);
  let from = Math.max(0, segmentStart - preroll);
  const to = Math.min(segmentEnd + preroll, from + preroll + maxSamples);

  // Nới ngược cho đủ minProbeMs. Giới hạn dưới là mẫu cũ nhất còn nguyên vẹn
  // trong vòng đệm; chừa 1 khung an toàn vì `ringWritten` vừa nhích lên.
  const minProbeSamples = Math.floor((config.minProbeMs / 1000) * config.sampleRate);
  if (to - from < minProbeSamples) {
    const oldestAvailable = Math.max(0, ringWritten - RING_SAMPLES + audioData.length);
    from = Math.max(oldestAvailable, to - minProbeSamples);
  }

  const audio = readRing(from, to);
  if (!audio) {
    log('warn', '[WakeWorker] Câu ứng viên đã bị vòng đệm ghi đè, bỏ qua.');
    return null;
  }

  lastCandidateTime = now;
  return { audio, speechMs, peakRms: rms };
}

// ============================================================================
// Message Handler
// ============================================================================

self.onmessage = async (event: MessageEvent) => {
  const { type, data } = event.data;

  switch (type) {
    case 'init': {
      if (data?.config) {
        config = { ...DEFAULT_CONFIG, ...data.config };
      }
      resetSegmentation();
      isReady = true;
      log('info', `[WakeWorker] Sẵn sàng — sàn RMS ${config.speechFloor}, cắt câu ${config.minUtteranceMs}–${config.maxUtteranceMs} ms`);
      post({ type: 'ready', success: true });
      break;
    }

    case 'audio': {
      // PCM f32 16 kHz mono từ AudioWorklet.
      const audioData = new Float32Array(data.audio);
      const candidate = processAudioFrame(audioData);

      if (candidate) {
        log('info', `[WakeWorker] Cụm ứng viên ${candidate.speechMs.toFixed(0)} ms → gửi core xác minh`);
        // Chuyển quyền sở hữu buffer thay vì sao chép: câu ứng viên tới 3 s là
        // 192 KB, cấu trúc sao chép mặc định của postMessage sẽ nhân đôi nó.
        post(
          { type: 'candidate', audio: candidate.audio.buffer, speechMs: candidate.speechMs, peakRms: candidate.peakRms },
          [candidate.audio.buffer],
        );
      }
      break;
    }

    case 'pause': {
      isPaused = true;
      resetSegmentation();
      log('info', '[WakeWorker] Paused');
      post({ type: 'paused' });
      break;
    }

    case 'resume': {
      isPaused = false;
      resetSegmentation();
      log('info', '[WakeWorker] Resumed');
      post({ type: 'resumed' });
      break;
    }

    case 'reset': {
      lastCandidateTime = 0;
      // Bỏ luôn audio đang tồn trong vòng đệm. Sau một quãng bị chặn (loa LIVA
      // đang phát), phần còn lại là audio cũ; ghép nó với audio mới tạo ra một
      // bậc năng lượng mà bộ cắt câu đọc thành một cụm giả.
      ringWritten = 0;
      ring.fill(0);
      resetSegmentation();
      peakRmsInSecond = 0;
      log('info', '[WakeWorker] Reset');
      post({ type: 'reset' });
      break;
    }

    case 'setThreshold': {
      // Giữ tên thông điệp cũ (UI/localStorage đã dùng), nhưng giá trị giờ là
      // **sàn RMS**, không còn là confidence của model.
      const newFloor = data?.threshold;
      if (typeof newFloor === 'number' && newFloor > 0 && newFloor <= 1) {
        config.speechFloor = newFloor;
        log('info', `[WakeWorker] Sàn RMS đặt thành: ${newFloor}`);
        post({ type: 'thresholdChanged', threshold: newFloor });
      }
      break;
    }

    case 'terminate': {
      isReady = false;
      log('info', '[WakeWorker] Terminated');
      post({ type: 'terminated' });
      self.close();
      break;
    }
  }
};

// Signal that worker is loaded
post({ type: 'loaded' });

export {};
