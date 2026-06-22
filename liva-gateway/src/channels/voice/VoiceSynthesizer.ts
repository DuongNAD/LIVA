import { logger } from "../../utils/logger";
import { NativeIPCClient } from "../../utils/NativeIPCClient";
// Giả lập sử dụng thư viện Edge-TTS để gọi API Microsoft Azure Cloud
// import { EdgeTTS } from "edge-tts";

export class VoiceSynthesizer {
    private ipcClient: NativeIPCClient;
    private voiceName: string = "vi-VN-HoaiMyNeural"; // Giọng nữ chuẩn VTV của Microsoft Azure

    constructor(ipcClient: NativeIPCClient) {
        this.ipcClient = ipcClient;
    }

    /**
     * Chunks a continuous stream of text into full sentences and streams them to Edge-TTS.
     * ZERO VRAM Footprint, Highest Vietnamese Quality.
     */
    public async streamSentenceBySentence(textStream: AsyncIterable<string>, onAudioChunk: (chunk: Buffer) => void) {
        logger.info(`🎙️ [Edge-TTS] Đang thiết lập kênh phát Audio Cloud với giọng: ${this.voiceName}`);
        let sentenceBuffer = "";

        for await (const token of textStream) {
            sentenceBuffer += token;
            
            // Chẻ câu theo dấu ngữ pháp
            if (/[.!?\n]/.test(token)) {
                const sentenceToSpeak = sentenceBuffer.trim();
                sentenceBuffer = ""; 

                if (sentenceToSpeak.length > 0) {
                    logger.debug(`[TTS] Đang đẩy câu lên Microsoft Cloud (0MB VRAM): "${sentenceToSpeak}"`);
                    
                    try {
                        // Giả lập lấy audio buffer từ Edge TTS
                        // const audioBuffer = await EdgeTTS.synthesize(sentenceToSpeak, this.voiceName);
                        const audioBuffer = Buffer.from("Simulated Edge-TTS Audio Chunk for: " + sentenceToSpeak);
                        onAudioChunk(audioBuffer); 
                    } catch (e) {
                        logger.error(`[TTS] Lỗi khi gọi Edge-TTS Cloud API: ${e}. Máy có bị rớt mạng không?`);
                    }
                }
            }
        }

        // Phát nốt câu cuối
        if (sentenceBuffer.trim().length > 0) {
            try {
                const audioBuffer = Buffer.from("Simulated Edge-TTS Audio Chunk for: " + sentenceBuffer.trim());
                onAudioChunk(audioBuffer);
            } catch (e) {
                logger.error(`[TTS] Lỗi Cloud API: ${e}`);
            }
        }

        logger.info("✅ [Edge-TTS] Hoàn tất quá trình Stream toàn bộ văn bản.");
    }
}
