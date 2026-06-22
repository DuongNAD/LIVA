/**
 * NemotronWorker — Streaming ASR via Nemotron 3.5 ONNX (CPU-only, Zero-VRAM)
 * ===========================================================================
 * [v31 Pillar 4: Native Streaming STT]
 *
 * Runs NVIDIA Nemotron 3.5 ASR (600M, INT4 quantized) inside a dedicated
 * Node.js worker_thread. Main thread only sends raw Float32Array via
 * MessagePort (zero-copy transfer). Worker responds with partial/final
 * transcription events.
 *
 * WHY WORKER?
 * - ONNX inference takes ~20-40ms per 160ms audio chunk
 * - Running this on Main Thread blocks Event Loop
 * - This violates AI_CONTEXT CRITICAL_DIRECTIVE 4.0: >10ms CPU = Worker
 *
 * ARCHITECTURE:
 * - Cache-Aware FastConformer encoder: processes each audio frame only ONCE
 *   by caching self-attention and convolution activations
 * - RNNT decoder: emits text tokens incrementally (streaming)
 * - INT4 quantized: ~700MB RAM, 0% VRAM, 4.5x faster than real-time on CPU
 *
 * MODEL SPEC (from genai_config.json):
 * - vocab_size: 13088
 * - num_mels: 128
 * - fft_size: 512, hop_length: 160, win_length: 400
 * - preemphasis: 0.97
 * - subsampling_factor: 8
 * - blank_id: 13087
 * - encoder: 24 layers, hidden_size 1024
 * - decoder: LSTM 2 layers, hidden_size 640
 * - chunk_samples: 8960 (560ms @ 16kHz)
 *
 * PROTOCOL (parentPort messages):
 * Parent → Worker:  { type: "init", modelDir: string, language?: string }
 * Parent → Worker:  { type: "audio_chunk", buffer: Float32Array, isLast: boolean }
 * Parent → Worker:  { type: "reset" }
 * Parent → Worker:  { type: "ping" }
 * Parent → Worker:  { type: "dispose" }
 * Worker → Parent:  { type: "ready" }
 * Worker → Parent:  { type: "partial", text: string }
 * Worker → Parent:  { type: "final", text: string }
 * Worker → Parent:  { type: "pong" }
 * Worker → Parent:  { type: "error", message: string }
 */

import { parentPort } from "node:worker_threads";
import * as ort from "onnxruntime-node";
import * as fs from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";

// ─── Type Definitions ───────────────────────────────────────────────────────

interface GenaiConfig {
    readonly vocabSize: number;
    readonly numMels: number;
    readonly fftSize: number;
    readonly hopLength: number;
    readonly winLength: number;
    readonly preemph: number;
    readonly logEps: number;
    readonly subsamplingFactor: number;
    readonly leftContext: number;
    readonly convContext: number;
    readonly preEncodeCacheSize: number;
    readonly sampleRate: number;
    readonly chunkSamples: number;
    readonly blankId: number;
    readonly maxSymbolsPerStep: number;
    readonly encoder: {
        readonly filename: string;
        readonly hiddenSize: number;
        readonly numHiddenLayers: number;
    };
    readonly decoder: {
        readonly filename: string;
        readonly hiddenSize: number;
        readonly numHiddenLayers: number;
    };
    readonly joiner: {
        readonly filename: string;
    };
}

interface TokenizerVocab {
    readonly id2token: Map<number, string>;
    readonly blankId: number;
}

interface AudioChunkMessage {
    readonly buffer: Float32Array;
    readonly isLast: boolean;
}

interface InboundMessage {
    readonly type: string;
    readonly modelDir?: string;
    readonly language?: string;
    readonly buffer?: Float32Array;
    readonly isLast?: boolean;
}

// ─── Language ID Map (Nemotron 40-language support) ─────────────────────────

const LANGUAGE_IDS: Record<string, number> = {
    en: 0, es: 1, fr: 2, de: 3, it: 4, pt: 5, nl: 6, pl: 7, ru: 8, uk: 9,
    cs: 10, sk: 11, hu: 12, ro: 13, bg: 14, hr: 15, sl: 16, sr: 17, bs: 18, mk: 19,
    tr: 20, ar: 21, he: 22, fa: 23, hi: 24, bn: 25, ta: 26, te: 27, mr: 28, gu: 29,
    kn: 30, ml: 31, th: 32, vi: 33, id: 34, ms: 35, zh: 36, ja: 37, ko: 38, sv: 39,
};

// ─── Constants (defaults, overridden by genai_config.json) ──────────────────

const DEFAULT_CONFIG: GenaiConfig = {
    vocabSize: 13088,
    numMels: 128,
    fftSize: 512,
    hopLength: 160,
    winLength: 400,
    preemph: 0.97,
    logEps: 5.96046448e-08,
    subsamplingFactor: 8,
    leftContext: 56,
    convContext: 8,
    preEncodeCacheSize: 9,
    sampleRate: 16000,
    chunkSamples: 8960,
    blankId: 13087,
    maxSymbolsPerStep: 10,
    encoder: { filename: "encoder.onnx", hiddenSize: 1024, numHiddenLayers: 24 },
    decoder: { filename: "decoder.onnx", hiddenSize: 640, numHiddenLayers: 2 },
    joiner: { filename: "joint.onnx" },
};

const SPECIAL_TOKENS = new Set(["<blank>", "<unk>", "<s>", "</s>", "<pad>"]);

// ─── Mutable State ──────────────────────────────────────────────────────────

let config: GenaiConfig = DEFAULT_CONFIG;
let languageId = 33; // Vietnamese default

// ONNX Sessions
let encoderSession: ort.InferenceSession | null = null;
let decoderSession: ort.InferenceSession | null = null;
let joinerSession: ort.InferenceSession | null = null;

// Encoder Cache State (cache-aware streaming)
let cacheLastChannel: Float32Array | null = null;
let cacheLastTime: Float32Array | null = null;
let cacheLastChannelLen: BigInt64Array | null = null;

// Decoder LSTM State
let decoderHiddenState: Float32Array | null = null; // h_in: [num_layers, 1, hidden_size]
let decoderCellState: Float32Array | null = null;   // c_in: [num_layers, 1, hidden_size]
let lastDecoderToken = 0; // blank/SOS token

// Tokenizer
let vocab: TokenizerVocab | null = null;

// Accumulated transcription for current utterance
let accumulatedTokenIds: number[] = [];

// Mel Spectrogram Preprocessing
let hannWindow: Float32Array | null = null;
let melFilterbank: Float32Array[] | null = null;
let residualSamples: Float32Array = new Float32Array(0);
let prevSample = 0; // For preemphasis filter
let hasRunEncoder = false;

// FIFO Queue
const audioQueue: AudioChunkMessage[] = [];
let isProcessing = false;
let lastInferenceStart = 0;

// ═══════════════════════════════════════════════════════════════════════════
// Config Loading
// ═══════════════════════════════════════════════════════════════════════════

function loadGenaiConfig(modelDir: string): GenaiConfig {
    const configPath = path.join(modelDir, "genai_config.json");
    try {
        if (fs.existsSync(configPath)) {
            const raw = fs.readFileSync(configPath, "utf-8");
            const parsed: unknown = JSON.parse(raw);
            if (parsed && typeof parsed === "object") {
                const root = parsed as Record<string, unknown>;
                const m = root["model"] as Record<string, unknown> | undefined;
                if (m) {
                    const enc = m["encoder"] as Record<string, unknown> | undefined;
                    const dec = m["decoder"] as Record<string, unknown> | undefined;
                    const joi = m["joiner"] as Record<string, unknown> | undefined;
                    return {
                        vocabSize: (m["vocab_size"] as number) ?? DEFAULT_CONFIG.vocabSize,
                        numMels: (m["num_mels"] as number) ?? DEFAULT_CONFIG.numMels,
                        fftSize: (m["fft_size"] as number) ?? DEFAULT_CONFIG.fftSize,
                        hopLength: (m["hop_length"] as number) ?? DEFAULT_CONFIG.hopLength,
                        winLength: (m["win_length"] as number) ?? DEFAULT_CONFIG.winLength,
                        preemph: (m["preemph"] as number) ?? DEFAULT_CONFIG.preemph,
                        logEps: (m["log_eps"] as number) ?? DEFAULT_CONFIG.logEps,
                        subsamplingFactor: (m["subsampling_factor"] as number) ?? DEFAULT_CONFIG.subsamplingFactor,
                        leftContext: (m["left_context"] as number) ?? DEFAULT_CONFIG.leftContext,
                        convContext: (m["conv_context"] as number) ?? DEFAULT_CONFIG.convContext,
                        preEncodeCacheSize: (m["pre_encode_cache_size"] as number) ?? DEFAULT_CONFIG.preEncodeCacheSize,
                        sampleRate: (m["sample_rate"] as number) ?? DEFAULT_CONFIG.sampleRate,
                        chunkSamples: (m["chunk_samples"] as number) ?? DEFAULT_CONFIG.chunkSamples,
                        blankId: (m["blank_id"] as number) ?? DEFAULT_CONFIG.blankId,
                        maxSymbolsPerStep: (m["max_symbols_per_step"] as number) ?? DEFAULT_CONFIG.maxSymbolsPerStep,
                        encoder: {
                            filename: (enc?.["filename"] as string) ?? DEFAULT_CONFIG.encoder.filename,
                            hiddenSize: (enc?.["hidden_size"] as number) ?? DEFAULT_CONFIG.encoder.hiddenSize,
                            numHiddenLayers: (enc?.["num_hidden_layers"] as number) ?? DEFAULT_CONFIG.encoder.numHiddenLayers,
                        },
                        decoder: {
                            filename: (dec?.["filename"] as string) ?? DEFAULT_CONFIG.decoder.filename,
                            hiddenSize: (dec?.["hidden_size"] as number) ?? DEFAULT_CONFIG.decoder.hiddenSize,
                            numHiddenLayers: (dec?.["num_hidden_layers"] as number) ?? DEFAULT_CONFIG.decoder.numHiddenLayers,
                        },
                        joiner: {
                            filename: (joi?.["filename"] as string) ?? DEFAULT_CONFIG.joiner.filename,
                        },
                    };
                }
            }
        }
    } catch {
        // Fall through to defaults
    }
    return DEFAULT_CONFIG;
}

// ═══════════════════════════════════════════════════════════════════════════
// Mel Spectrogram Preprocessing (128-dim, preemphasis, power spectrum)
// ═══════════════════════════════════════════════════════════════════════════

function hzToMel(hz: number): number {
    const f_min = 0.0;
    const f_sp = 200.0 / 3;
    if (hz < 1000.0) {
        return (hz - f_min) / f_sp;
    } else {
        const min_log_hz = 1000.0;
        const min_log_mel = (min_log_hz - f_min) / f_sp;
        const logstep = Math.log(6.4) / 27.0;
        return min_log_mel + Math.log(hz / min_log_hz) / logstep;
    }
}

function melToHz(mel: number): number {
    const f_min = 0.0;
    const f_sp = 200.0 / 3;
    const min_log_hz = 1000.0;
    const min_log_mel = (min_log_hz - f_min) / f_sp;
    if (mel < min_log_mel) {
        return f_min + f_sp * mel;
    } else {
        const logstep = Math.log(6.4) / 27.0;
        return min_log_hz * Math.exp(logstep * (mel - min_log_mel));
    }
}

/** Pre-compute the mel filterbank matrix (numMels x (fftSize/2 + 1)). */
function computeMelFilterbank(): Float32Array[] {
    const numBins = config.fftSize / 2 + 1;
    const minMel = hzToMel(0);
    const maxMel = hzToMel(8000);

    const melF = new Float32Array(config.numMels + 2);
    for (let i = 0; i < config.numMels + 2; i++) {
        melF[i] = melToHz(minMel + (i * (maxMel - minMel)) / (config.numMels + 1));
    }

    const fftFreqs = new Float32Array(numBins);
    for (let k = 0; k < numBins; k++) {
        fftFreqs[k] = (k * config.sampleRate) / config.fftSize;
    }

    const filters: Float32Array[] = [];
    for (let i = 0; i < config.numMels; i++) {
        const filter = new Float32Array(numBins);
        const fdiffLower = melF[i + 1] - melF[i];
        const fdiffUpper = melF[i + 2] - melF[i + 1];
        const enorm = 2.0 / (melF[i + 2] - melF[i]); // Slaney normalization

        for (let k = 0; k < numBins; k++) {
            const lower = (fftFreqs[k] - melF[i]) / fdiffLower;
            const upper = (melF[i + 2] - fftFreqs[k]) / fdiffUpper;
            const weight = Math.max(0, Math.min(lower, upper));
            filter[k] = weight * enorm;
        }
        filters.push(filter);
    }
    return filters;
}

function computeHannWindow(): Float32Array {
    const window = new Float32Array(config.winLength);
    for (let i = 0; i < config.winLength; i++) {
        window[i] = 0.5 * (1.0 - Math.cos((2.0 * Math.PI * i) / config.winLength));
    }
    return window;
}

/**
 * Radix-2 Cooley-Tukey FFT → power spectrum (magnitude squared).
 * Returns power spectrum of length fftSize/2 + 1.
 */
function computeFFTMagnitudeSq(frame: Float32Array): Float32Array {
    const n = config.fftSize;
    const real = new Float32Array(n);
    const imag = new Float32Array(n);
    const offset = Math.floor((n - frame.length) / 2); // 56
    for (let i = 0; i < frame.length && (offset + i) < n; i++) {
        real[offset + i] = frame[i];
    }

    // Bit-reversal permutation
    for (let i = 1, j = 0; i < n; i++) {
        let bit = n >> 1;
        while (j & bit) {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if (i < j) {
            const tmpR = real[i]; real[i] = real[j]; real[j] = tmpR;
            const tmpI = imag[i]; imag[i] = imag[j]; imag[j] = tmpI;
        }
    }

    // FFT butterfly
    for (let len = 2; len <= n; len <<= 1) {
        const halfLen = len >> 1;
        const angle = (-2.0 * Math.PI) / len;
        const wR = Math.cos(angle);
        const wI = Math.sin(angle);

        for (let i = 0; i < n; i += len) {
            let curR = 1.0;
            let curI = 0.0;
            for (let j = 0; j < halfLen; j++) {
                const evenIdx = i + j;
                const oddIdx = i + j + halfLen;
                const tR = curR * real[oddIdx] - curI * imag[oddIdx];
                const tI = curR * imag[oddIdx] + curI * real[oddIdx];
                real[oddIdx] = real[evenIdx] - tR;
                imag[oddIdx] = imag[evenIdx] - tI;
                real[evenIdx] += tR;
                imag[evenIdx] += tI;
                const nextR = curR * wR - curI * wI;
                curI = curR * wI + curI * wR;
                curR = nextR;
            }
        }
    }

    const numBins = n / 2 + 1;
    const powerSpec = new Float32Array(numBins);
    for (let i = 0; i < numBins; i++) {
        powerSpec[i] = real[i] * real[i] + imag[i] * imag[i];
    }
    return powerSpec;
}

/**
 * Convert raw Float32 PCM → 128-dim log-mel spectrogram.
 * Applies preemphasis filter (0.97) per genai_config.
 * Returns [numFrames, numMels] features as flat Float32Array.
 */
function computeLogMelSpectrogram(
    samples: Float32Array,
): Float32Array {
    if (!hannWindow || !melFilterbank) {
        throw new Error("Mel spectrogram not initialized");
    }
    if (samples.length !== 10640) {
        throw new Error(`computeLogMelSpectrogram expects exactly 10640 samples, got ${samples.length}`);
    }

    const numFrames = 65;
    const features = new Float32Array(numFrames * config.numMels);
    const windowedFrame = new Float32Array(config.winLength);

    for (let f = 0; f < numFrames; f++) {
        const frameCenter = f * config.hopLength;
        const startIdx = frameCenter - Math.floor(config.winLength / 2);

        // Apply Hann window with reflect padding
        for (let i = 0; i < config.winLength; i++) {
            let idx = startIdx + i;
            if (idx < 0) {
                idx = -idx; // reflect padding on left
            } else if (idx >= samples.length) {
                idx = 2 * samples.length - 2 - idx; // reflect padding on right
            }
            windowedFrame[i] = samples[idx] * hannWindow[i];
        }

        // FFT → power spectrum
        const powerSpec = computeFFTMagnitudeSq(windowedFrame);

        // Apply mel filterbank + log (with log_eps guard)
        for (let m = 0; m < config.numMels; m++) {
            let melEnergy = 0.0;
            const filter = melFilterbank[m];
            for (let k = 0; k < powerSpec.length; k++) {
                melEnergy += filter[k] * powerSpec[k];
            }
            features[f * config.numMels + m] = Math.log(melEnergy + config.logEps);
        }
    }

    return features;
}

// ═══════════════════════════════════════════════════════════════════════════
// Tokenizer / Vocabulary
// ═══════════════════════════════════════════════════════════════════════════

function loadTokenizer(modelDir: string): TokenizerVocab {
    const tokenizerPath = path.join(modelDir, "tokenizer.json");
    const raw = fs.readFileSync(tokenizerPath, "utf-8");
    const parsed: unknown = JSON.parse(raw);

    if (!parsed || typeof parsed !== "object") {
        throw new Error("Invalid tokenizer.json format");
    }

    const id2token = new Map<number, string>();

    const tokObj = parsed as Record<string, unknown>;
    const model = tokObj["model"] as Record<string, unknown> | undefined;

    if (model && Array.isArray(model["vocab"])) {
        // SentencePiece BPE format: vocab is array of [token, score] pairs
        const vocabArr = model["vocab"] as [string, number][];
        for (let i = 0; i < vocabArr.length; i++) {
            id2token.set(i, vocabArr[i][0]);
        }
    } else if (model && typeof model["vocab"] === "object" && model["vocab"] !== null) {
        // Alternative: vocab is object { token: id }
        const vocabMap = model["vocab"] as Record<string, number>;
        for (const [token, id] of Object.entries(vocabMap)) {
            if (typeof id === "number") {
                id2token.set(id, token);
            }
        }
    }

    // Also check added_tokens
    const addedTokens = tokObj["added_tokens"] as Array<Record<string, unknown>> | undefined;
    if (Array.isArray(addedTokens)) {
        for (const at of addedTokens) {
            if (typeof at["id"] === "number" && typeof at["content"] === "string") {
                id2token.set(at["id"], at["content"]);
            }
        }
    }

    // Fallback: try vocab.txt
    if (id2token.size === 0) {
        const vocabTxtPath = path.join(modelDir, "vocab.txt");
        if (fs.existsSync(vocabTxtPath)) {
            const lines = fs.readFileSync(vocabTxtPath, "utf-8").split("\n");
            for (let i = 0; i < lines.length; i++) {
                const token = lines[i].trim();
                if (token.length > 0) {
                    id2token.set(i, token);
                }
            }
        }
    }

    if (id2token.size === 0) {
        throw new Error("Failed to parse vocabulary from tokenizer.json or vocab.txt");
    }

    return { id2token, blankId: config.blankId };
}

/** Convert token IDs to text, handling SentencePiece ▁ word boundaries. */
function tokensToText(tokenIds: number[]): string {
    if (!vocab) return "";
    const parts: string[] = [];

    for (const id of tokenIds) {
        if (id === vocab.blankId) continue;
        const token = vocab.id2token.get(id);
        if (!token || SPECIAL_TOKENS.has(token)) continue;
        parts.push(token);
    }

    return parts.join("").replace(/▁/g, " ").replace(/\s+/g, " ").trim();
}

// ═══════════════════════════════════════════════════════════════════════════
// Cache Management (Cache-Aware Encoder + LSTM Decoder)
// ═══════════════════════════════════════════════════════════════════════════

function initializeEncoderCaches(): void {
    // cache_last_channel: [num_layers, 1, hidden_size, left_context]
    const channelElements = config.encoder.numHiddenLayers * 1 * config.encoder.hiddenSize * config.leftContext;
    cacheLastChannel = new Float32Array(channelElements).fill(0);

    // cache_last_time: [num_layers, 1, hidden_size, conv_context]
    const timeElements = config.encoder.numHiddenLayers * 1 * config.encoder.hiddenSize * config.convContext;
    cacheLastTime = new Float32Array(timeElements).fill(0);

    // cache_last_channel_len: [1] — tracks how many valid cache frames exist
    cacheLastChannelLen = new BigInt64Array([0n]);
}

function initializeDecoderState(): void {
    // LSTM hidden state: [num_layers, 1, hidden_size]
    const stateElements = config.decoder.numHiddenLayers * 1 * config.decoder.hiddenSize;
    decoderHiddenState = new Float32Array(stateElements).fill(0);
    decoderCellState = new Float32Array(stateElements).fill(0);
    lastDecoderToken = config.blankId;
}

// ═══════════════════════════════════════════════════════════════════════════
// ONNX Inference (using exact tensor names from genai_config.json)
// ═══════════════════════════════════════════════════════════════════════════

async function runEncoder(
    features: Float32Array,
    numFrames: number,
): Promise<{ encoderOut: ort.Tensor; encoderOutLen: number }> {
    if (!encoderSession || !cacheLastChannel || !cacheLastTime || !cacheLastChannelLen) {
        throw new Error("Encoder session or caches not initialized");
    }

    // audio_signal: [batch=1, time_frames, num_mels]
    const featureTensor = new ort.Tensor("float32", features, [1, numFrames, config.numMels]);
    // length: [batch=1]
    const lengthTensor = new ort.Tensor("int64", BigInt64Array.from([BigInt(numFrames)]), [1]);

    // cache_last_channel: [batch=1, num_layers, left_context, hidden_size]
    const channelDims = [1, config.encoder.numHiddenLayers, config.leftContext, config.encoder.hiddenSize];
    const cacheChannelTensor = new ort.Tensor("float32", cacheLastChannel, channelDims);

    // cache_last_time: [batch=1, num_layers, hidden_size, conv_context]
    const timeDims = [1, config.encoder.numHiddenLayers, config.encoder.hiddenSize, config.convContext];
    const cacheTimeTensor = new ort.Tensor("float32", cacheLastTime, timeDims);

    // cache_last_channel_len: [1]
    const cacheChannelLenTensor = new ort.Tensor("int64", cacheLastChannelLen, [1]);

    // lang_id: [1]
    const langIdTensor = new ort.Tensor("int64", BigInt64Array.from([BigInt(languageId)]), [1]);

    const feeds: Record<string, ort.Tensor> = {
        audio_signal: featureTensor,
        length: lengthTensor,
        cache_last_channel: cacheChannelTensor,
        cache_last_time: cacheTimeTensor,
        cache_last_channel_len: cacheChannelLenTensor,
        lang_id: langIdTensor,
    };

    const results = await encoderSession.run(feeds);

    // Capture output caches for next chunk (names from genai_config)
    const outCacheChannel = results["cache_last_channel_next"];
    const outCacheTime = results["cache_last_time_next"];
    const outCacheChannelLen = results["cache_last_channel_len_next"];

    if (outCacheChannel) {
        cacheLastChannel = new Float32Array(outCacheChannel.data as Float32Array);
    }
    if (outCacheTime) {
        cacheLastTime = new Float32Array(outCacheTime.data as Float32Array);
    }
    if (outCacheChannelLen) {
        cacheLastChannelLen = new BigInt64Array(outCacheChannelLen.data as BigInt64Array);
    }

    // Encoder output: "outputs" tensor
    const encoderOutput = results["outputs"];
    if (!encoderOutput) {
        throw new Error("Encoder output tensor 'outputs' not found in session results");
    }

    // Output length after subsampling
    const outLenTensor = results["encoded_lengths"];
    const encoderOutLen = outLenTensor
        ? Number((outLenTensor.data as BigInt64Array)[0])
        : encoderOutput.dims[1];

    // Log features and output ranges
    let fMin = Infinity, fMax = -Infinity, fSum = 0;
    for (let i = 0; i < features.length; i++) {
        fMin = Math.min(fMin, features[i]);
        fMax = Math.max(fMax, features[i]);
        fSum += features[i];
    }
    const outData = encoderOutput.data as Float32Array;
    let oMin = Infinity, oMax = -Infinity, oSum = 0;
    for (let i = 0; i < outData.length; i++) {
        oMin = Math.min(oMin, outData[i]);
        oMax = Math.max(oMax, outData[i]);
        oSum += outData[i];
    }
    parentPort?.postMessage({
        type: "log",
        message: `[ENCODER DEBUG] features range: [${fMin.toFixed(4)}, ${fMax.toFixed(4)}], mean=${(fSum/features.length).toFixed(4)} | outputs range: [${oMin.toFixed(4)}, ${oMax.toFixed(4)}], mean=${(oSum/outData.length).toFixed(4)}`
    });

    return { encoderOut: encoderOutput as ort.Tensor, encoderOutLen };
}

async function runDecoder(tokenId: number): Promise<{ decoderOut: ort.Tensor }> {
    if (!decoderSession || !decoderHiddenState || !decoderCellState) {
        throw new Error("Decoder session or LSTM state not initialized");
    }

    // targets: [batch=1, 1]
    const inputTensor = new ort.Tensor("int64", BigInt64Array.from([BigInt(tokenId)]), [1, 1]);

    // LSTM states: [num_layers, batch=1, hidden_size]
    const lstmDims = [config.decoder.numHiddenLayers, 1, config.decoder.hiddenSize];
    const hInTensor = new ort.Tensor("float32", decoderHiddenState, lstmDims);
    const cInTensor = new ort.Tensor("float32", decoderCellState, lstmDims);

    const feeds: Record<string, ort.Tensor> = {
        targets: inputTensor,
        h_in: hInTensor,
        c_in: cInTensor,
    };

    const results = await decoderSession.run(feeds);

    // Capture LSTM output state
    const hOut = results["h_out"];
    const cOut = results["c_out"];
    if (hOut) {
        decoderHiddenState = new Float32Array(hOut.data as Float32Array);
    }
    if (cOut) {
        decoderCellState = new Float32Array(cOut.data as Float32Array);
    }

    const decoderOut = results["decoder_output"];
    if (!decoderOut) {
        throw new Error("Decoder output tensor 'decoder_output' not found");
    }
    return { decoderOut: decoderOut as ort.Tensor };
}

async function runJoiner(
    encoderFrame: ort.Tensor,
    decoderOut: ort.Tensor,
): Promise<number> {
    if (!joinerSession) {
        throw new Error("Joiner session not initialized");
    }

    // Decoder output is [1, 640, 1] — reshape to [1, 1, 640] for joiner
    // Must TRANSPOSE the data, not just reshape, since dims are [batch, hidden, seq]
    const decData = decoderOut.data as Float32Array;
    const reshapedDecoderOut = new ort.Tensor(
        "float32",
        decData,
        [1, 1, config.decoder.hiddenSize]
    );

    const feeds: Record<string, ort.Tensor> = {
        encoder_output: encoderFrame,
        decoder_output: reshapedDecoderOut,
    };

    const results = await joinerSession.run(feeds);
    const logits = results["joint_output"];
    if (!logits) {
        throw new Error("Joiner output tensor 'joint_output' not found");
    }

    // Argmax over vocabulary dimension
    const data = logits.data as Float32Array;
    let maxIdx = 0;
    let maxVal = data[0];
    for (let i = 1; i < data.length; i++) {
        if (data[i] > maxVal) {
            maxVal = data[i];
            maxIdx = i;
        }
    }
    return maxIdx;
}

// ═══════════════════════════════════════════════════════════════════════════
// RNNT Greedy Decoding
// ═══════════════════════════════════════════════════════════════════════════

async function greedyRnntDecode(
    encoderOut: ort.Tensor,
    encoderOutLen: number,
): Promise<number[]> {
    if (!vocab) throw new Error("Vocabulary not initialized");

    const emittedTokens: number[] = [];
    const encoderData = encoderOut.data as Float32Array;
    const encoderDims = encoderOut.dims; // [batch, time, features]
    const featureDim = encoderDims[2];

    let { decoderOut } = await runDecoder(lastDecoderToken);

    for (let t = 0; t < encoderOutLen; t++) {
        // Extract single encoder frame: [1, 1, featureDim]
        const frameStart = t * featureDim;
        const frameData = new Float32Array(featureDim);
        for (let d = 0; d < featureDim; d++) {
            frameData[d] = encoderData[frameStart + d];
        }
        const encoderFrame = new ort.Tensor("float32", frameData, [1, 1, featureDim]);

        let stepsThisFrame = 0;

        // Inner loop: emit tokens until blank or max steps
        while (stepsThisFrame < config.maxSymbolsPerStep) {
            const tokenId = await runJoiner(encoderFrame, decoderOut);
            stepsThisFrame++;

            if (tokenId === config.blankId) {
                break;
            }

            // Non-blank token emitted
            emittedTokens.push(tokenId);
            lastDecoderToken = tokenId;
            const result = await runDecoder(lastDecoderToken);
            decoderOut = result.decoderOut;
        }
    }

    return emittedTokens;
}

// ═══════════════════════════════════════════════════════════════════════════
// Audio Chunk Processing
// ═══════════════════════════════════════════════════════════════════════════

async function processAudioChunk(buffer: Float32Array, isLast: boolean): Promise<void> {
    lastInferenceStart = Date.now();
    try {
        // Apply preemphasis (config.preemph = 0.97) on raw float audio [-1, 1]
        // NeMo expects waveform in natural float range — do NOT scale to Int16
        const preemphed = new Float32Array(buffer.length);
        for (let i = 0; i < buffer.length; i++) {
            preemphed[i] = buffer[i] - config.preemph * prevSample;
            prevSample = buffer[i];
        }

        const combined = new Float32Array(residualSamples.length + preemphed.length);
        combined.set(residualSamples, 0);
        combined.set(preemphed, residualSamples.length);
        residualSamples = combined;

        // [DEBUG] Log buffer accumulation
        parentPort?.postMessage({ type: "log", message: `[ASR] chunk=${buffer.length} residual=${residualSamples.length} isLast=${isLast}` });

        while (residualSamples.length >= 10640) {
            const slice = residualSamples.subarray(0, 10640);
            const features = computeLogMelSpectrogram(slice);
            const { encoderOut, encoderOutLen } = await runEncoder(features, 65);
            hasRunEncoder = true;
            const newTokens = await greedyRnntDecode(encoderOut, encoderOutLen);
            accumulatedTokenIds.push(...newTokens);

            // [DEBUG] Log encoder output
            parentPort?.postMessage({ type: "log", message: `[ASR] encoder outLen=${encoderOutLen} newTokens=${newTokens.length} (${newTokens.slice(0, 10).join(",")}) total=${accumulatedTokenIds.length}` });

            if (newTokens.length > 0) {
                const partialText = tokensToText(accumulatedTokenIds);
                parentPort?.postMessage({ type: "partial", text: partialText });
            }

            residualSamples = residualSamples.slice(8960);
        }

        if (isLast) {
            parentPort?.postMessage({ type: "log", message: `[ASR] FINAL: residual=${residualSamples.length} hasRunEncoder=${hasRunEncoder} accumulated=${accumulatedTokenIds.length}` });

            if (residualSamples.length > 1680 || (!hasRunEncoder && residualSamples.length > 0)) {
                const padded = new Float32Array(10640);
                padded.set(residualSamples, 0);
                const features = computeLogMelSpectrogram(padded);
                const { encoderOut, encoderOutLen } = await runEncoder(features, 65);
                hasRunEncoder = true;
                const lastTokens = await greedyRnntDecode(encoderOut, encoderOutLen);
                accumulatedTokenIds.push(...lastTokens);
                parentPort?.postMessage({ type: "log", message: `[ASR] FINAL encoder: outLen=${encoderOutLen} lastTokens=${lastTokens.length} total=${accumulatedTokenIds.length}` });
            }
            residualSamples = new Float32Array(0);
            const finalText = tokensToText(accumulatedTokenIds);
            parentPort?.postMessage({ type: "final", text: finalText });
        }
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", message: `ASR inference error: ${msg}` });
    } finally {
        lastInferenceStart = 0;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Sequential FIFO Queue (VADWorker pattern)
// ═══════════════════════════════════════════════════════════════════════════

async function processQueue(): Promise<void> {
    if (isProcessing) return;
    isProcessing = true;
    try {
        while (audioQueue.length > 0) {
            const next = audioQueue.shift();
            if (next) await processAudioChunk(next.buffer, next.isLast);
        }
    } finally {
        isProcessing = false;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Initialization
// ═══════════════════════════════════════════════════════════════════════════

async function initialize(modelDir: string, language?: string): Promise<void> {
    try {
        // Load config from genai_config.json
        config = loadGenaiConfig(modelDir);

        // Set language ID
        if (language && language in LANGUAGE_IDS) {
            languageId = LANGUAGE_IDS[language];
        }

        // Load tokenizer vocabulary
        vocab = loadTokenizer(modelDir);

        // Pre-compute mel spectrogram helpers
        hannWindow = computeHannWindow();
        melFilterbank = computeMelFilterbank();
        residualSamples = new Float32Array(0);
        prevSample = 0;
        hasRunEncoder = false;

        // Initialize encoder caches
        initializeEncoderCaches();

        // Initialize decoder LSTM state
        initializeDecoderState();

        // Clear accumulated tokens
        accumulatedTokenIds = [];

        // Load ONNX sessions strictly on CPU to protect VRAM and avoid yield conflicts with the local LLM.
        const cpuOptions: ort.InferenceSession.SessionOptions = {
            executionProviders: ["cpu"],
            intraOpNumThreads: 2,
            interOpNumThreads: 1,
            enableCpuMemArena: false,
        };

        const encoderPath = path.join(modelDir, config.encoder.filename);
        const decoderPath = path.join(modelDir, config.decoder.filename);
        const joinerPath = path.join(modelDir, config.joiner.filename);

        // Validate model files
        for (const filePath of [encoderPath, decoderPath, joinerPath]) {
            if (!fs.existsSync(filePath)) {
                throw new Error(`Model file not found: ${filePath}`);
            }
        }

        // Load all 3 sessions in parallel
        const [enc, dec, joi] = await Promise.all([
            ort.InferenceSession.create(encoderPath, cpuOptions),
            ort.InferenceSession.create(decoderPath, cpuOptions),
            ort.InferenceSession.create(joinerPath, cpuOptions),
        ]);
        encoderSession = enc;
        decoderSession = dec;
        joinerSession = joi;

        parentPort?.postMessage({ type: "ready" });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", message: `Nemotron init failed: ${msg}` });
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Reset (clear state for new utterance)
// ═══════════════════════════════════════════════════════════════════════════

function resetState(): void {
    initializeEncoderCaches();
    initializeDecoderState();
    accumulatedTokenIds = [];
    residualSamples = new Float32Array(0);
    prevSample = 0;
    hasRunEncoder = false;
}

// ═══════════════════════════════════════════════════════════════════════════
// Dispose (cleanup and exit)
// ═══════════════════════════════════════════════════════════════════════════

async function dispose(): Promise<void> {
    try { if (encoderSession) await encoderSession.release(); } catch { /* ignore */ }
    try { if (decoderSession) await decoderSession.release(); } catch { /* ignore */ }
    try { if (joinerSession) await joinerSession.release(); } catch { /* ignore */ }

    encoderSession = null;
    decoderSession = null;
    joinerSession = null;
    cacheLastChannel = null;
    cacheLastTime = null;
    cacheLastChannelLen = null;
    decoderHiddenState = null;
    decoderCellState = null;
    vocab = null;
    hannWindow = null;
    melFilterbank = null;
    accumulatedTokenIds = [];
    audioQueue.length = 0;
    residualSamples = new Float32Array(0);
    prevSample = 0;
    hasRunEncoder = false;

    process.exit(0);
}

// ═══════════════════════════════════════════════════════════════════════════
// Message Handler (parentPort)
// ═══════════════════════════════════════════════════════════════════════════

parentPort?.on("message", async (msg: InboundMessage) => {
    switch (msg.type) {
        case "init":
            if (msg.modelDir) {
                await initialize(msg.modelDir, msg.language);
            } else {
                parentPort?.postMessage({ type: "error", message: "init requires modelDir" });
            }
            break;

        case "audio_chunk":
            if (msg.buffer) {
                audioQueue.push({ buffer: msg.buffer, isLast: msg.isLast === true });
                await processQueue();
            }
            break;

        case "reset":
            resetState();
            break;

        case "ping":
            if (lastInferenceStart > 0 && Date.now() - lastInferenceStart > 5000) {
                logger.error(`[NemotronWorker] Native hang detected! Inference running for ${Date.now() - lastInferenceStart}ms.`);
                // Withhold pong to trigger watchdog recovery
            } else {
                parentPort?.postMessage({ type: "pong" });
            }
            break;

        case "dispose":
            await dispose();
            break;
    }
});
