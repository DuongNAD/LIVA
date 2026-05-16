/**
 * useWakeWord.ts — Wake Word Detection via ONNX Runtime WebAssembly
 * ==================================================================
 * [v25 Pillar 4: Wake-Word Edge Offloading]
 *
 * Architecture:
 *   - Mic is always-on in passive mode (getUserMedia with echo cancellation)
 *   - Audio chunks are processed locally via WakeWordWorker (ONNX WASM)
 *   - When wake word is detected → send event to Gateway via WebSocket
 *   - Backend (Whisper) has ZERO CPU/GPU usage when idle
 *
 * Benefits:
 *   - 100% local processing (privacy-first)
 *   - Zero Backend CPU/GPU when idle
 *   - Zero network traffic when idle
 *   - Fast detection (~100ms latency)
 *
 * Dependencies:
 *   - onnxruntime-web (loaded in Web Worker)
 *   - WakeWordWorker.ts (Web Worker for ONNX inference)
 */
import { ref, type Ref } from "vue";
import { logger } from "../utils/logger";

export interface UseWakeWordReturn {
    isListening: Ref<boolean>;
    isReady: Ref<boolean>;
    startWakeWord: () => Promise<void>;
    stopWakeWord: () => Promise<void>;
    pauseWakeWord: () => void;
    resumeWakeWord: () => void;
    onWakeWordDetected: (callback: (keyword: string) => void) => void;
    setWebSocket: (ws: WebSocket) => void;
}

// ============================================================================
// Worker Manager
// ============================================================================

let wakeWordWorker: Worker | null = null;
let isWorkerReady = false;

function initWorker(): Promise<boolean> {
    return new Promise((resolve) => {
        if (wakeWordWorker && isWorkerReady) {
            resolve(true);
            return;
        }

        // Create worker
        wakeWordWorker = new Worker(
            new URL('../workers/WakeWordWorker.ts', import.meta.url),
            { type: 'module' }
        );

        wakeWordWorker.onmessage = (event) => {
            const { type, success } = event.data;

            // Handle log messages forwarded from worker
            if (type === '__log') {
                const { level, args } = event.data;
                const fn = logger[level as "debug" | "info" | "warn" | "error"] ?? logger.info;
                fn('[WakeWord]', ...args);
                return;
            }

            if (type === 'loaded') {
                // Worker loaded, now init the model
                wakeWordWorker?.postMessage({ type: 'init' });
            } else if (type === 'ready') {
                isWorkerReady = success;
                resolve(success);
            } else if (type === 'detection') {
                // Wake word detected! Trigger callback
                handleWakeWordDetection();
            }
        };

        wakeWordWorker.onerror = (error) => {
            logger.error('[WakeWord]', 'Worker error:', error);
            resolve(false);
        };
    });
}

function sendToWorker(type: string, data?: any) {
    if (wakeWordWorker) {
        wakeWordWorker.postMessage({ type, data });
    }
}

// ============================================================================
// Callbacks
// ============================================================================

let detectedCallback: ((keyword: string) => void) | null = null;
let wsRef: WebSocket | null = null;

function handleWakeWordDetection() {
    // Notify registered callback
    if (detectedCallback) {
        detectedCallback('');
    }

    // Send event to Gateway via WebSocket
    if (wsRef && wsRef.readyState === WebSocket.OPEN) {
        wsRef.send(JSON.stringify({
            event: 'wake_word_triggered',
            payload: {}
        }));
    }
}

// ============================================================================
// Main Composable
// ============================================================================

export function useWakeWord(): UseWakeWordReturn {
    const isListening = ref(false);
    const isReady = ref(false);

    // Internal state
    let mediaStream: MediaStream | null = null;
    let audioContext: AudioContext | null = null;
    let processor: ScriptProcessorNode | null = null;
    let source: MediaStreamAudioSourceNode | null = null;
    let isPaused = false;

    // ============================================================================
    // Public API
    // ============================================================================

    /**
     * Register callback for wake word detection.
     */
    function onWakeWordDetected(callback: (keyword: string) => void) {
        detectedCallback = callback;
    }

    /**
     * Set WebSocket reference for sending events to Gateway.
     */
    function setWebSocket(ws: WebSocket) {
        wsRef = ws;
    }

    /**
     * Start always-on mic for wake word detection.
     * Audio is processed locally via WakeWordWorker (ONNX WASM).
     */
    async function startWakeWord(): Promise<void> {
        if (isListening.value) return;

        // Check browser support
        if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
            logger.error('[WakeWord]', 'getUserMedia not supported');
            return;
        }

        try {
            // 1. Initialize ONNX Worker
            const workerReady = await initWorker();
            if (!workerReady) {
                logger.error('[WakeWord]', 'Failed to initialize ONNX worker');
                return;
            }

            // 2. Get mic stream (mono, 16kHz, echo cancellation ON)
            mediaStream = await navigator.mediaDevices.getUserMedia({
                audio: {
                    channelCount: 1,
                    sampleRate: { ideal: 16000 },
                    echoCancellation: true,
                    noiseSuppression: true,
                    autoGainControl: true,
                },
            });

            // 3. Setup AudioContext
            const AudioCtx = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
            audioContext = new AudioCtx({ sampleRate: 16000 });
            source = audioContext.createMediaStreamSource(mediaStream);

            // 4. ScriptProcessor for raw PCM extraction
            // Buffer size 4096 at 16kHz = ~256ms chunks
            processor = audioContext.createScriptProcessor(4096, 1, 1);

            processor.onaudioprocess = (e: AudioProcessingEvent) => {
                if (!isListening.value || isPaused || !wakeWordWorker) return;

                const inputData = e.inputBuffer.getChannelData(0);

                // Quick energy gate: skip silent frames to reduce inference load
                let sumSquares = 0;
                for (let i = 0; i < inputData.length; i++) {
                    sumSquares += inputData[i] * inputData[i];
                }
                const rms = Math.sqrt(sumSquares / inputData.length);

                // Only process audio with detectable energy (above noise floor)
                if (rms > 0.01) {
                    // Send audio data to worker for ONNX inference
                    // Worker will handle feature extraction + classification
                    sendToWorker('audio', {
                        audio: Array.from(inputData)
                    });
                }
            };

            // Connect pipeline: mic → processor → destination (required for ScriptProcessor)
            source.connect(processor);
            processor.connect(audioContext.destination);

            isListening.value = true;
            isReady.value = true;
            isPaused = false;
            logger.info('[WakeWord]', 'Always-on mic started with ONNX wake word detection');
        } catch (err: unknown) {
            logger.error('[WakeWord]', 'Failed to start:', err instanceof Error ? err.message : String(err));
            isListening.value = false;
            isReady.value = false;
        }
    }

    /**
     * Stop wake word detection and release all resources.
     */
    async function stopWakeWord(): Promise<void> {
        isListening.value = false;
        isReady.value = false;
        isPaused = false;

        // Cleanup audio pipeline
        if (processor) {
            processor.onaudioprocess = null;
            processor.disconnect();
            processor = null;
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

        // Terminate worker to release ONNX WASM memory
        if (wakeWordWorker) {
            sendToWorker('terminate');
            wakeWordWorker = null;
            isWorkerReady = false;
        }

        wsRef = null;
        logger.info('[WakeWord]', 'Stopped wake word detection');
    }

    /**
     * Pause wake word detection (e.g., while TTS is playing to avoid feedback).
     * Mic stays open but inference is paused.
     */
    function pauseWakeWord() {
        isPaused = true;
        sendToWorker('pause');
        logger.info('[WakeWord]', 'Paused');
    }

    /**
     * Resume wake word detection after pause.
     */
    function resumeWakeWord() {
        isPaused = false;
        sendToWorker('resume');
        logger.info('[WakeWord]', 'Resumed');
    }

    return {
        isListening,
        isReady,
        startWakeWord,
        stopWakeWord,
        pauseWakeWord,
        resumeWakeWord,
        onWakeWordDetected,
        setWebSocket,
    };
}
