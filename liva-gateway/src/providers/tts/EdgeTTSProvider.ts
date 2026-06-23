import { VoiceEngine } from "../../services/VoiceEngine";
import { ITTSProvider } from "../ITTSProvider";

/**
 * EdgeTTSProvider — Implements ITTSProvider for Edge-TTS by subclassing VoiceEngine.
 */
export class EdgeTTSProvider extends VoiceEngine implements ITTSProvider {
    constructor() {
        super();
    }
}
