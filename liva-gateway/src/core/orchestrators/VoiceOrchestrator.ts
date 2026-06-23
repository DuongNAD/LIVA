import { ITTSProvider } from "../../providers/ITTSProvider";
import { ISTTProvider } from "../../providers/ISTTProvider";
import { EdgeTTSProvider } from "../../providers/tts/EdgeTTSProvider";
import { KokoroTTSProvider } from "../../providers/tts/KokoroTTSProvider";
import { NemotronSTTProvider } from "../../providers/stt/NemotronSTTProvider";
import { SenseVoiceSTTProvider } from "../../providers/stt/SenseVoiceSTTProvider";
import { ConfigManager } from "../config/ConfigManager";
import { VADWorkerBridge } from "../../services/VADWorkerBridge";
import { logger } from "../../utils/logger";

export class VoiceOrchestrator {
    public voiceEngine: ITTSProvider | null = null;
    public whisperNode: ISTTProvider;
    public vadBridge: VADWorkerBridge | null = null;
    public onSpeechDetected?: () => void;
    public onVoiceEngineInitialized?: (v: ITTSProvider) => void;
    
    constructor() {
        this.whisperNode = new NemotronSTTProvider();
    }

    public async initialize(_agentLoop: unknown) {
        const configManager = ConfigManager.getInstance();
        
        // TTS Engine Selection
        const activeTts = configManager.activeTtsProvider;
        if (activeTts === 'kokoro') {
            logger.info(`🗣️ [VoiceOrchestrator] TTS Engine: Local Kokoro (Offline)`);
            this.voiceEngine = new KokoroTTSProvider();
        } else {
            logger.info(`🗣️ [VoiceOrchestrator] TTS Engine: Edge-TTS (Primary)`);
            this.voiceEngine = new EdgeTTSProvider();
        }

        if (this.onVoiceEngineInitialized) {
            this.onVoiceEngineInitialized(this.voiceEngine!);
        }

        // STT Engine Selection
        const activeStt = configManager.activeSttProvider;
        if (activeStt === 'sensevoice') {
            logger.info(`👂 [VoiceOrchestrator] STT Engine: SenseVoice`);
            this.whisperNode = new SenseVoiceSTTProvider();
        } else {
            logger.info(`👂 [VoiceOrchestrator] STT Engine: Nemotron`);
            this.whisperNode = new NemotronSTTProvider();
        }

        // Initialize STT Model
        try {
            await this.whisperNode.initialize();
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            logger.error(`[VoiceOrchestrator] ❌ STT init failed: ${msg}. Voice STT will be unavailable.`);
        }
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
