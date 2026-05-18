/**
 * SmartTurnVAD — Edge Voice Activity Detection via Silero ONNX.
 * 
 * ⚠️ Uses dynamic import() for onnxruntime-web to prevent Gateway crash
 * when the package is unavailable (it's a transitive dep of kokoro-js).
 * The BootstrapManager already guards initialization with fs.existsSync(modelPath),
 * but a top-level import would crash BEFORE that guard runs.
 */

/** Ring Buffer — chống OOM khi mic mở liên tục trong môi trường ồn */
const MAX_BUFFER_FRAMES = 16000 * 30;  // 30 giây @ 16kHz

/** Lazily loaded onnxruntime-web module */
let ort: typeof import("onnxruntime-web") | null = null;

export class SmartTurnVAD {
    #session: any | null = null;
    #disposed = false;
    #ringBuffer: Float32Array;          // Cửa sổ trượt cố định
    #writePos = 0;                      // Vị trí ghi hiện tại
    #bufferFilled = 0;                  // Số frame đã ghi
    #muted = false;
    
    constructor() {
        this.#ringBuffer = new Float32Array(MAX_BUFFER_FRAMES);
    }

    mute(): void { this.#muted = true; }
    unmute(): void { this.#muted = false; }
    
    /**
     * Load ONNX model (~8MB, INT8 quantized).
     * Runs on WASM CPU via onnxruntime-web — 0% GPU, 100% Tauri/SEA compatible.
     */
    async initialize(modelPath: string): Promise<void> {
        // [FIX] Dynamic import — prevents Gateway crash when package is missing
        if (!ort) {
            ort = await import("onnxruntime-web");
        }
        ort.env.wasm.numThreads = 1;    // Single-threaded để không tranh CPU với LLM
        this.#session = await ort.InferenceSession.create(modelPath, {
            executionProviders: ["wasm"],
        });
    }
    
    /**
     * Append audio chunk to ring buffer, then run VAD inference.
     * Ring buffer auto-evicts oldest frames when full (HOTFIX #4).
     */
    async processAudioChunk(chunk: Float32Array): Promise<{
        isTurnEnd: boolean;
        confidence: number;
    }> {
        if (this.#muted) return { isTurnEnd: false, confidence: 0 };
        if (!this.#session || this.#disposed || !ort) return { isTurnEnd: false, confidence: 0 };
        
        // Ring Buffer write — overwrite oldest frames when full
        for (let i = 0; i < chunk.length; i++) {
            this.#ringBuffer[this.#writePos] = chunk[i];
            this.#writePos = (this.#writePos + 1) % MAX_BUFFER_FRAMES;
        }
        this.#bufferFilled = Math.min(this.#bufferFilled + chunk.length, MAX_BUFFER_FRAMES);
        
        // Extract current window for inference
        const window = this.#getCurrentWindow();
        const inputTensor = new ort.Tensor("float32", window, [1, window.length]);
        const results = await this.#session.run({ input: inputTensor });
        const score = (results.output.data as Float32Array)[0];
        
        // Reset buffer on turn end detection
        if (score > 0.7) {
            this.#writePos = 0;
            this.#bufferFilled = 0;
        }
        
        return { isTurnEnd: score > 0.7, confidence: score };
    }
    
    /** Extract valid audio window from ring buffer */
    #getCurrentWindow(): Float32Array {
        if (this.#bufferFilled < MAX_BUFFER_FRAMES) {
            return this.#ringBuffer.slice(0, this.#bufferFilled);
        }
        // Full buffer — reconstruct linear order from ring
        const result = new Float32Array(MAX_BUFFER_FRAMES);
        const tail = MAX_BUFFER_FRAMES - this.#writePos;
        result.set(this.#ringBuffer.subarray(this.#writePos), 0);
        result.set(this.#ringBuffer.subarray(0, this.#writePos), tail);
        return result;
    }
    
    dispose(): void {
        this.#disposed = true;
        this.#session = null;
        this.#ringBuffer = new Float32Array(0);  // Free memory
    }
}
