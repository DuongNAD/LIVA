/**
 * useWakeWord.ts — Wake Word Detection via Whisper STT
 * =====================================================
 * [v25 Pillar 4: Wake-Word Edge Offloading]
 *
 * Architecture:
 *   - Mic is always-on in passive mode (getUserMedia with echo cancellation)
 *   - Audio chunks are streamed to Gateway via WebSocket (binary PCM)
 *   - Gateway routes audio to WhisperNode.pushWakeAudioChunk()
 *   - WhisperNode transcribes on silence → checks for wake phrases
 *   - On match: Gateway broadcasts "wake_word_detected" → UI activates
 *
 * Zero new dependencies — reuses existing Whisper STT pipeline.
 */
import { ref, type Ref } from "vue";

export interface UseWakeWordReturn {
    isListening: Ref<boolean>;
    isReady: Ref<boolean>;
    startWakeWord: (ws: WebSocket) => Promise<void>;
    stopWakeWord: () => Promise<void>;
    pauseWakeWord: () => void;
    resumeWakeWord: () => void;
    onWakeWordDetected: (callback: (keyword: string) => void) => void;
}

export function useWakeWord(): UseWakeWordReturn {
    const isListening = ref(false);
    const isReady = ref(false);

    // Internal state
    let mediaStream: MediaStream | null = null;
    let audioContext: AudioContext | null = null;
    let processor: ScriptProcessorNode | null = null;
    let source: MediaStreamAudioSourceNode | null = null;
    let wsRef: WebSocket | null = null;
    let isPaused = false;
    let detectedCallback: ((keyword: string) => void) | null = null;

    /**
     * Register callback for wake word detection.
     * Called when Gateway sends "wake_word_detected" event.
     */
    function onWakeWordDetected(callback: (keyword: string) => void) {
        detectedCallback = callback;
    }

    /**
     * Trigger the registered callback (called from WidgetApp when Gateway sends event).
     */
    function _triggerDetection(keyword: string) {
        if (detectedCallback) {
            detectedCallback(keyword);
        }
    }

    /**
     * Start always-on mic for wake word detection.
     * Audio is sent as binary PCM to Gateway via WebSocket.
     * Gateway will route it to WhisperNode's wake detection pipeline.
     */
    async function startWakeWord(ws: WebSocket): Promise<void> {
        if (isListening.value) return;

        // Check browser support
        if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
            console.error("[WakeWord] getUserMedia not supported");
            return;
        }

        try {
            // 1. Get mic stream (mono, 16kHz for Whisper, echo cancellation ON)
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

            // 2. Setup AudioContext
            const AudioCtx = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
            audioContext = new AudioCtx({ sampleRate: 16000 });
            source = audioContext.createMediaStreamSource(mediaStream);

            // 3. ScriptProcessor for raw PCM extraction
            // Buffer size 4096 at 16kHz = ~256ms chunks
            processor = audioContext.createScriptProcessor(4096, 1, 1);

            processor.onaudioprocess = (e: AudioProcessingEvent) => {
                if (!isListening.value || isPaused) return;
                if (!wsRef || wsRef.readyState !== WebSocket.OPEN) return;

                const inputData = e.inputBuffer.getChannelData(0);

                // Quick energy gate: skip silent frames to reduce network traffic
                let sumSquares = 0;
                for (let i = 0; i < inputData.length; i++) {
                    sumSquares += inputData[i] * inputData[i];
                }
                const rms = Math.sqrt(sumSquares / inputData.length);

                // Only send audio with detectable energy (above noise floor)
                if (rms > 0.01) {
                    const buffer = new Float32Array(inputData.length);
                    buffer.set(inputData);
                    wsRef.send(buffer.buffer);
                }
            };

            // Connect pipeline: mic → processor → destination (required for ScriptProcessor)
            source.connect(processor);
            processor.connect(audioContext.destination);

            // 4. Tell Gateway to enable wake word mode
            ws.send(JSON.stringify({
                event: "wake_word_mode",
                payload: { enabled: true },
            }));

            isListening.value = true;
            isReady.value = true;
            isPaused = false;

            console.log("[WakeWord] ✅ Always-on mic started — listening for 'Hey Liva'");
        } catch (err: any) {
            console.error("[WakeWord] Failed to start:", err?.message ?? err);
            isListening.value = false;
            isReady.value = false;
        }
    }

    /**
     * Stop wake word detection and release all resources.
     */
    async function stopWakeWord(): Promise<void> {
        // Tell Gateway to disable wake word mode
        if (wsRef && wsRef.readyState === WebSocket.OPEN) {
            wsRef.send(JSON.stringify({
                event: "wake_word_mode",
                payload: { enabled: false },
            }));
        }

        isListening.value = false;
        isReady.value = false;
        isPaused = false;

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

        wsRef = null;
        console.log("[WakeWord] 🛑 Stopped wake word detection");
    }

    /**
     * Pause wake word detection (e.g., while TTS is playing to avoid feedback).
     * Mic stays open but audio is not sent.
     */
    function pauseWakeWord() {
        isPaused = true;
    }

    /**
     * Resume wake word detection after pause.
     */
    function resumeWakeWord() {
        isPaused = false;
    }

    return {
        isListening,
        isReady,
        startWakeWord,
        stopWakeWord,
        pauseWakeWord,
        resumeWakeWord,
        onWakeWordDetected,
        // Internal: exposed for WidgetApp to call when Gateway event arrives
        _triggerDetection,
    } as UseWakeWordReturn & { _triggerDetection: (keyword: string) => void };
}
