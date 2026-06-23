import { KokoroVoiceEngine } from "../../services/KokoroVoiceEngine";
import { ITTSProvider } from "../ITTSProvider";

/**
 * KokoroTTSProvider — Implements ITTSProvider for Kokoro-TTS by subclassing KokoroVoiceEngine.
 */
export class KokoroTTSProvider extends KokoroVoiceEngine implements ITTSProvider {
    constructor() {
        super();
    }
}
