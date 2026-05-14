import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";

/**
 * Wake phrase variants — normalized to lowercase for matching.
 * Supports Vietnamese accented and unaccented variations.
 */
const WAKE_PHRASES = [
  "hey liva", "hê liva", "hey li va", "hê li va",
  "liva ơi", "li va ơi", "ê liva", "ê li va",
  "hey leva", "hey líva"  // Common Whisper mis-transcriptions
];

/**
 * WhisperNode — Hardware-Asymmetric STT (Zero-VRAM Router)
 * ==========================================================
 * [v22 Full-Duplex Pillar 3]
 *
 * Routing Strategy:
 *   - AI_PROVIDER=local → Local LLM is hogging GPU VRAM.
 *     Route STT to Cloud Whisper (Groq/OpenAI) OR Python Native Engine on port 8100.
 *     GPU is "Bất khả xâm phạm" (untouchable) territory for LLM.
 *
 *   - AI_PROVIDER=cloud → GPU is free (no local LLM).
 *     Use local Whisper server for maximum privacy + zero latency.
 *
 * Endpoint Priority:
 *   1. WHISPER_URL env var (explicit override)
 *   2. WHISPER_CLOUD_URL env var (cloud fallback for local LLM mode)
 *   3. Default: http://127.0.0.1:8100/v1/audio/transcriptions (Python Native Engine)
 *
 * [v25 Wake Word Mode]
 *   When wakeWordMode=true, audio is buffered separately and transcribed
 *   to check for wake phrases. On detection, emits 'wake_word_detected'.
 */
export class WhisperNode extends EventEmitter {
  private audioBuffer: Buffer[] = [];
  private isProcessing: boolean = false;
  private silenceTimer: NodeJS.Timeout | null = null;

  // ── Circuit Breaker (v25 Anti-DDoS) ──
  /** Prevents spamming requests to a failing Whisper server */
  #isCircuitOpen: boolean = false;
  #consecutiveFailures: number = 0;
  #circuitTimer: NodeJS.Timeout | null = null;
  private readonly CIRCUIT_THRESHOLD = 3;      // Open circuit after 3 failures
  private readonly CIRCUIT_RESET_MS = 15000;    // Auto-reset after 15 seconds

  // ── Wake Word State ──
  private wakeWordMode: boolean = false;
  private wakeAudioBuffer: Buffer[] = [];
  private wakeProcessing: boolean = false;
  private wakeSilenceTimer: NodeJS.Timeout | null = null;
  private readonly WAKE_SILENCE_MS = 600;  // Shorter for faster wake detection
  
  // Ngưỡng 800ms để quyết định người dùng đã kết thúc câu (Endpointing)
  // [v22] This timer will be REPLACED by VADWorkerBridge when SmartTurnVAD is wired.
  // For now, it serves as a fallback for environments without Silero ONNX model.
  private readonly VAD_SILENCE_MS = 800; 

  constructor() {
    super();
    logger.info(`👂 [WhisperNode] Khởi tạo Hệ thống Thính giác (VAD + Whisper). Chờ tín hiệu Float32 PCM.`);
  }

  // ═══════════════════════════════════════════════════════
  //  Circuit Breaker (v25 Anti-DDoS)
  // ═══════════════════════════════════════════════════════

  /**
   * [v25] Record a failure and potentially open the circuit breaker.
   * After 3 consecutive failures, blocks all further requests for 15 seconds.
   */
  #recordFailure(): void {
    this.#consecutiveFailures++;
    if (this.#consecutiveFailures >= this.CIRCUIT_THRESHOLD) {
      this.#isCircuitOpen = true;
      logger.error(`[WhisperNode] 🔥 CIRCUIT OPEN — Too many failures (${this.#consecutiveFailures}). Blocking requests for ${this.CIRCUIT_RESET_MS / 1000}s.`);

      // Schedule circuit reset
      if (this.#circuitTimer) clearTimeout(this.#circuitTimer);
      this.#circuitTimer = setTimeout(() => {
        this.#isCircuitOpen = false;
        this.#consecutiveFailures = 0;
        this.#circuitTimer = null;
        logger.info(`[WhisperNode] ✅ Circuit reset — Whisper requests resumed.`);
      }, this.CIRCUIT_RESET_MS);
    }
  }

  /**
   * [v25] Record a successful request — reset failure counter.
   */
  #recordSuccess(): void {
    this.#consecutiveFailures = 0;
  }

  /**
   * Check if circuit breaker is open (service unavailable).
   */
  public isCircuitOpen(): boolean {
    return this.#isCircuitOpen;
  }

  // ═══════════════════════════════════════════════════════
  //  Wake Word Mode Control
  // ═══════════════════════════════════════════════════════

  /**
   * Enable/disable wake word listening mode.
   * When enabled, incoming audio is routed to wake detection pipeline
   * instead of the normal STT pipeline.
   */
  public setWakeWordMode(enabled: boolean): void {
    this.wakeWordMode = enabled;
    if (enabled) {
      logger.info(`👂 [WhisperNode] Wake Word Mode: BẬT — đang lắng nghe "Hey Liva"...`);
      // Clear normal STT buffers to prevent cross-contamination
      this.flush();
    } else {
      logger.info(`👂 [WhisperNode] Wake Word Mode: TẮT`);
      this.flushWakeBuffer();
    }
  }

  public isWakeWordEnabled(): boolean {
    return this.wakeWordMode;
  }

  private flushWakeBuffer(): void {
    this.wakeAudioBuffer = [];
    if (this.wakeSilenceTimer) {
      clearTimeout(this.wakeSilenceTimer);
      this.wakeSilenceTimer = null;
    }
    this.wakeProcessing = false;
  }

  /**
   * Push audio chunk for wake word detection.
   * Separate from pushAudioChunk to keep pipelines independent.
   */
  public pushWakeAudioChunk(chunk: Buffer): void {
    if (!this.wakeWordMode) return;

    this.wakeAudioBuffer.push(chunk);

    if (this.wakeSilenceTimer) {
      clearTimeout(this.wakeSilenceTimer);
    }
    this.wakeSilenceTimer = setTimeout(() => this.processWakeWordAudio(), this.WAKE_SILENCE_MS);
  }

  /**
   * Process accumulated wake word audio through Whisper STT
   * then check transcription for wake phrases.
   */
  private async processWakeWordAudio(): Promise<void> {
    if (this.wakeAudioBuffer.length === 0 || this.wakeProcessing) return;

    this.wakeProcessing = true;

    const fullBuffer = Buffer.concat(this.wakeAudioBuffer);
    this.wakeAudioBuffer = [];

    try {
      // Minimum 4096 bytes (~256ms at 16kHz) to avoid noise false positives
      if (fullBuffer.length > 4096) {
        const transcription = await this.realWhisperInference(fullBuffer);
        if (transcription) {
          const normalized = transcription.toLowerCase().trim();
          logger.debug(`[WhisperNode] 🎧 Wake Check: "${normalized}"`);

          const matchedPhrase = WAKE_PHRASES.find(phrase => normalized.includes(phrase));
          if (matchedPhrase) {
            logger.info(`[WhisperNode] 🔔 WAKE WORD PHÁT HIỆN: "${transcription}"`);
            
            // Extract text after the wake phrase (e.g., "Hey Liva mấy giờ rồi" → "mấy giờ rồi")
            const phraseIdx = normalized.indexOf(matchedPhrase);
            const trailingText = transcription.substring(phraseIdx + matchedPhrase.length).trim();
            
            this.emit("wake_word_detected", trailingText || "");
          }
        }
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[WhisperNode] ❌ Wake word STT error: ${errMsg}`);
    } finally {
      this.wakeProcessing = false;
    }
  }

  /**
   * Hứng luồng âm thanh Mic cực nhỏ bắn từ UIController
   * @param chunk Khối nhị phân PCM
   */
  public pushAudioChunk(chunk: Buffer) {
    this.audioBuffer.push(chunk);

    // Kích hoạt lại cơ chế đo đếm Khoảng lặng
    if (this.silenceTimer) {
      clearTimeout(this.silenceTimer);
    }
    this.silenceTimer = setTimeout(() => this.processAudioIntents(), this.VAD_SILENCE_MS);
  }

  /**
   * [v22] Called by VADWorkerBridge on "speech_end" event.
   * Immediately triggers transcription without waiting for silence timer.
   */
  public triggerTranscription(): void {
    if (this.silenceTimer) {
      clearTimeout(this.silenceTimer);
      this.silenceTimer = null;
    }
    this.processAudioIntents();
  }

  /**
   * Trích xuất STT khi nhận diện khoảng lặng, theo kiến trúc VAD.
   */
  private async processAudioIntents() {
    if (this.audioBuffer.length === 0 || this.isProcessing) return;

    this.isProcessing = true;
    logger.debug(`[WhisperNode] 🎙️ Đoạt ngưỡng Silence Threshold. Đang nén VRAM để trích xuất Text...`);

    const fullBuffer = Buffer.concat(this.audioBuffer);
    this.audioBuffer = []; // Wipe the buffer for the next sentence

    try {
      // Thay vì giả lập, gởi thẳng luồng Audio vào mô hình Whisper
      // Chúng ta sẽ ép buffer Float32 PCM thành 16kHz 16-bit WAV (chuẩn bắt buộc của whisper.cpp)
      if (fullBuffer.length > 4096) {
         logger.info(`[WhisperNode] ⚡ Đang đúc khuôn WAV và gọi xuống Whisper.cpp Core (GGUF-IQ)...`);
         
         const textResult = await this.realWhisperInference(fullBuffer);
         if (textResult) {
            logger.info(`[WhisperNode] 🎯 Nhận dạng Giọng Nói: "${textResult}"`);
            this.emit("transcription_ready", textResult);
         }
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[WhisperNode] ❌ Sụp đổ luồng ASR: ${errMsg}`);
    } finally {
      this.isProcessing = false;
    }
  }

  /**
   * [v22 Pillar 3] Resolve Whisper endpoint based on hardware profile.
   * [v25] Updated: Default port changed to 8100 (Python Native Engine).
   *
   * Decision Tree:
   *   WHISPER_URL set?          → Use it (explicit override)
   *   AI_PROVIDER = "local"?    → Use WHISPER_CLOUD_URL (keep GPU for LLM)
   *                                Fallback to local if no cloud URL configured.
   *   AI_PROVIDER = "cloud"?    → Use local Whisper (GPU is free)
   */
  #resolveWhisperEndpoint(): string {
    // 1. Explicit override
    if (process.env.WHISPER_URL) {
      return process.env.WHISPER_URL;
    }

    // 2. Hardware-asymmetric routing
    const isLocalLLM = (process.env.AI_PROVIDER?.toLowerCase() || "local") === "local";

    if (isLocalLLM && process.env.WHISPER_CLOUD_URL) {
      // LLM is hogging local GPU → route STT to cloud
      logger.debug("[WhisperNode] 🌐 Routing STT to Cloud (GPU reserved for local LLM)");
      return process.env.WHISPER_CLOUD_URL;
    }

    // 3. Default: Python Native Engine on port 8100
    return "http://127.0.0.1:8100/v1/audio/transcriptions";
  }

  /**
   * [v25] Check if endpoint is a cloud service (needs Authorization header).
   * Cloud endpoints don't contain localhost/127.0.0.1.
   */
  #isCloudEndpoint(endpoint: string): boolean {
    return !endpoint.includes("localhost") && !endpoint.includes("127.0.0.1");
  }

  /**
   * [v25] Build fetch headers based on endpoint type.
   * Cloud endpoints require Bearer token authentication.
   */
  #buildRequestHeaders(endpoint: string): Record<string, string> {
    const headers: Record<string, string> = {};

    if (this.#isCloudEndpoint(endpoint)) {
      // Cloud endpoint: require Authorization header
      const apiKey = process.env.AI_API_KEY;
      if (apiKey) {
        headers["Authorization"] = `Bearer ${apiKey}`;
        logger.debug("[WhisperNode] 🔐 Using Bearer auth for cloud STT endpoint");
      } else {
        logger.warn("[WhisperNode] ⚠️ Cloud STT endpoint configured but AI_API_KEY is missing!");
      }
    }

    return headers;
  }

  /**
   * Đúc File WAV trên RAM và đút vào mồm Whisper.cpp qua IPC Http
   */
  private async realWhisperInference(pcmFloat32Buffer: Buffer): Promise<string> {
    // [v25] Circuit Breaker: If service is failing, reject immediately
    if (this.#isCircuitOpen) {
      logger.warn("[WhisperNode] 🔒 Circuit is OPEN — skipping Whisper request (service unavailable)");
      return "";
    }

    // 1. Chuyển Buffer JS thành Float32Array
    const float32Arr = new Float32Array(
      pcmFloat32Buffer.buffer, 
      pcmFloat32Buffer.byteOffset, 
      pcmFloat32Buffer.byteLength / 4
    );

    // 2. Ép kiểu chuẩn WAV Mono 16-bit 16000Hz PCM Encoding
    const sampleRate = 16000;
    const wavBuffer = Buffer.alloc(44 + float32Arr.length * 2);
    
    // Header WAV 44 byte chấn phái
    wavBuffer.write("RIFF", 0);
    wavBuffer.writeUInt32LE(36 + float32Arr.length * 2, 4);
    wavBuffer.write("WAVE", 8);
    wavBuffer.write("fmt ", 12);
    wavBuffer.writeUInt32LE(16, 16); 
    wavBuffer.writeUInt16LE(1, 20); 
    wavBuffer.writeUInt16LE(1, 22); 
    wavBuffer.writeUInt32LE(sampleRate, 24); 
    wavBuffer.writeUInt32LE(sampleRate * 2, 28); 
    wavBuffer.writeUInt16LE(2, 32); 
    wavBuffer.writeUInt16LE(16, 34); 
    wavBuffer.write("data", 36);
    wavBuffer.writeUInt32LE(float32Arr.length * 2, 40);

    // Bơm float32 nén thành int16 (tăng âm khuếch đại nhẹ)
    let offset = 44;
    for (let i = 0; i < float32Arr.length; i++) {
        const s = Math.max(-1, Math.min(1, float32Arr[i]));
        wavBuffer.writeInt16LE(s < 0 ? s * 0x8000 : s * 0x7FFF, offset);
        offset += 2;
    }

    // 3. Giao tiếp Zero-Copy Form Data -> Liva Whisper Inference
    const blob = new Blob([wavBuffer], { type: "audio/wav" });
    const fd = new FormData();
    fd.append('file', blob, 'liva_realtime_transcribe.wav');
    fd.append('response_format', 'text');

    // [v22] Asymmetric endpoint routing
    const whisperEndpoint = this.#resolveWhisperEndpoint();
    const headers = this.#buildRequestHeaders(whisperEndpoint);

    try {
        // 🔒 [Audit Fix C-5] safeFetch with 30s timeout (Whisper inference can be slow)
        // [v25] Add Authorization header for cloud endpoints
        const { safeFetch } = await import("../utils/HttpClient");
        const response = await safeFetch(whisperEndpoint, {
            method: 'POST',
            body: fd,
            headers
        }, 30000);

        if (!response.ok) {
            throw new Error(`Whisper API trả về mã lỗi: ${response.status} ${response.statusText}`);
        }

        const textResponse = await response.text();
        this.#recordSuccess();  // [v25] Reset failure counter on success
        return textResponse.trim();
    } catch (err: unknown) {
      // [v25] Extract ECONNREFUSED correctly per AI_CONTEXT.md Rule 4.1
      // Native fetch buries the real error in e.cause.message
      const errMsg = err instanceof Error ? ((err as any).cause?.message || err.message) : String(err);
        logger.error(`[WhisperNode] 🔌 Mất kết nối tới Whisper Engine: ${errMsg}`);
        this.#recordFailure();  // [v25] Increment failure counter, may open circuit
        return "";
    }
  }

  /**
   * Trạng thái Barge-in khẩn cấp để đánh giá ngắt luồng
   */
  public flush() {
    this.audioBuffer = [];
    if (this.silenceTimer) clearTimeout(this.silenceTimer);
    this.isProcessing = false;
    logger.debug(`[WhisperNode] 🧹 Buffer flushed due to Preemption.`);
  }

  /**
   * 🔒 [Audit Fix M-7] Full cleanup — matches WhisperJSNode interface
   */
  public destroy() {
    logger.info(`[WhisperNode] 🧹 Disposing STT engine...`);
    this.flush();
    this.flushWakeBuffer();
    // [v25] Clear circuit breaker timer
    if (this.#circuitTimer) {
      clearTimeout(this.#circuitTimer);
      this.#circuitTimer = null;
    }
    this.#isCircuitOpen = false;
    this.#consecutiveFailures = 0;
    this.removeAllListeners();
  }
}
