import { IVoiceEngine } from "../../services/IVoiceEngine";
import { KokoroVoiceEngine } from "../../services/KokoroVoiceEngine";
import { VoiceEngine } from "../../services/VoiceEngine";
import { WhisperNode } from "../../services/WhisperNode";
import { SmartTurnVAD } from "../../services/SmartTurnVAD";
import { VADWorkerBridge } from "../../services/VADWorkerBridge";
import { AppConfig } from "../../config/AppConfig";
import { logger } from "../../utils/logger";
import { EventEmitter } from "node:events";

export class VoiceOrchestrator {
    public voiceEngine: IVoiceEngine | null = null;
    public whisperNode: WhisperNode;
    public smartTurnVAD: SmartTurnVAD | null = null;
    public vadBridge: VADWorkerBridge | null = null;
    public onSpeechDetected?: () => void;
    
    constructor() {
        this.whisperNode = new WhisperNode();
    }

    public async initialize(agentLoop: any) {
        const appConfig = AppConfig.get();
        const forceMode = appConfig.LIVA_TTS_ENGINE;
        
        if (!forceMode || forceMode === 'python') {
            logger.info(`🗣️ [VoiceOrchestrator] TTS Engine: Python Edge-TTS (Primary)`);
            this.voiceEngine = new VoiceEngine();
        } else {
            logger.info(`🗣️ [VoiceOrchestrator] TTS Engine: Local Kokoro (Offline)`);
            this.voiceEngine = new KokoroVoiceEngine();
        }

        // Connect Voice events to AgentLoop
        if (this.voiceEngine) {
            this.voiceEngine.on("play_started", () => {
                if (this.vadBridge) {
                    this.vadBridge.mute();
                } else if (this.smartTurnVAD) {
                    this.smartTurnVAD.mute();
                }
            });
            this.voiceEngine.on("play_finished", () => {
                if (this.vadBridge) {
                    this.vadBridge.unmute();
                } else if (this.smartTurnVAD) {
                    this.smartTurnVAD.unmute();
                }
            });
        }
    }

    public async dispose() {
        const safeExecAsync = async (fn: () => any) => { try { await fn(); } catch (e) { void e; } };
        await safeExecAsync(() => this.voiceEngine?.destroy());
        await safeExecAsync(() => this.whisperNode.flush());
        await safeExecAsync(() => this.whisperNode.destroy());
        await safeExecAsync(() => this.smartTurnVAD?.dispose());
        await safeExecAsync(() => this.vadBridge?.dispose());
        logger.info("[VoiceOrchestrator] Disposed an toàn.");
    }
}
