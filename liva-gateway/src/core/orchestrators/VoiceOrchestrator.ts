import { IVoiceEngine } from "../../services/IVoiceEngine";
import { KokoroVoiceEngine } from "../../services/KokoroVoiceEngine";
import { VoiceEngine } from "../../services/VoiceEngine";
import { NemotronSTTService } from "../../services/NemotronSTTService";
import { VADWorkerBridge } from "../../services/VADWorkerBridge";
import { AppConfig } from "../../config/AppConfig";
import { logger } from "../../utils/logger";


export class VoiceOrchestrator {
    public voiceEngine: IVoiceEngine | null = null;
    public whisperNode: NemotronSTTService;
    public vadBridge: VADWorkerBridge | null = null;
    public onSpeechDetected?: () => void;
    
    constructor() {
        this.whisperNode = new NemotronSTTService();
    }

    public async initialize(_agentLoop: unknown) {
        const appConfig = AppConfig.get();
        const forceMode = appConfig.LIVA_TTS_ENGINE;
        
        if (!forceMode || forceMode === 'python') {
            logger.info(`🗣️ [VoiceOrchestrator] TTS Engine: Python Edge-TTS (Primary)`);
            this.voiceEngine = new VoiceEngine();
        } else {
            logger.info(`🗣️ [VoiceOrchestrator] TTS Engine: Local Kokoro (Offline)`);
            this.voiceEngine = new KokoroVoiceEngine();
        }

        // [v31] Initialize Nemotron ASR ONNX model (CPU worker thread)
        try {
            await this.whisperNode.initialize();
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            logger.error(`[VoiceOrchestrator] ❌ Nemotron STT init failed: ${msg}. Voice STT will be unavailable.`);
        }

        // Connect Voice events to AgentLoop
        // Relying on frontend WebRTC AEC instead of muting VAD bridge during TTS playback to enable true voice barge-in.
        /*
        if (this.voiceEngine) {
            this.voiceEngine.on("play_started", () => {
                if (this.vadBridge) {
                    this.vadBridge.mute();
                }
            });
            this.voiceEngine.on("play_finished", () => {
                if (this.vadBridge) {
                    this.vadBridge.unmute();
                }
            });
        }
        */
    }

    public async dispose() {
        const safeExecAsync = async (fn: () => unknown) => { try { await fn(); } catch (e) { void e; } };
        await safeExecAsync(() => this.voiceEngine?.destroy());
        await safeExecAsync(() => this.whisperNode.flush());
        await safeExecAsync(() => this.whisperNode.destroy());
        await safeExecAsync(() => this.vadBridge?.dispose());
        logger.info("[VoiceOrchestrator] Disposed an toàn.");
    }
}
