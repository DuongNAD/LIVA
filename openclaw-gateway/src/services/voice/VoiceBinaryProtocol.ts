import { logger } from "../../utils/logger";

export const VOICE_OPCODES = {
    AUTH_HANDSHAKE: 0x00, // Yêu cầu xác thực Token (Zero-Trust)
    MIC_IN: 0x01,         // Luồng thu âm 16kHz
    SPEAKER_OUT: 0x02,    // Luồng phát âm 24kHz
    FLUSH: 0x03,          // Tín hiệu ngắt lời (Barge-in)
    ACK_PLAYING: 0x04     // Báo cáo mốc thời gian đã phát (Dùng cho Phoneme Truncation)
} as const;

export type VoiceOpCode = typeof VOICE_OPCODES[keyof typeof VOICE_OPCODES];

export class VoiceBinaryProtocol {
    static readonly #HEADER_SIZE = 9;
    static readonly #MAX_PAYLOAD_SIZE = 1024 * 1024; // 1MB Buffer Overflow Guard

    // Nâng cấp: Zero-GC Audio Buffer Pool (Chống GC Thrashing)
    readonly #bufferPool: Float32Array[] = [];

    constructor(poolSize: number = 50, bufferCapacity: number = 8192) {
        for (let i = 0; i < poolSize; i++) {
            this.#bufferPool.push(new Float32Array(bufferCapacity));
        }
    }

    public acquireBuffer(neededSize: number): Float32Array {
        const buf = this.#bufferPool.pop();
        if (buf && buf.length >= neededSize) return buf;
        return new Float32Array(neededSize); // Fallback nếu Pool cạn
    }

    public releaseBuffer(buffer: Float32Array): void {
        buffer.fill(0);
        this.#bufferPool.push(buffer);
    }

    /**
     * Encode Payload thành Binary Frame (Tốc độ C++ Backend)
     */
    public static encodeFrame(opCode: VoiceOpCode, seqId: number, payload: Uint8Array): Uint8Array {
        const payloadLength = payload.byteLength;
        const buffer = new ArrayBuffer(this.#HEADER_SIZE + payloadLength);
        const view = new DataView(buffer);

        view.setUint8(0, opCode);
        view.setUint32(1, seqId, true); // Little-Endian
        view.setUint32(5, payloadLength, true);

        // Zero-copy mem-set
        const destView = new Uint8Array(buffer, this.#HEADER_SIZE);
        destView.set(payload);

        return new Uint8Array(buffer);
    }

    /**
     * Decode Binary Frame. Xác thực bảo mật nội bộ.
     */
    public static decodeFrame(rawData: Buffer | ArrayBuffer): { opCode: number; seqId: number; payload: Uint8Array } | null {
        try {
            const buffer = rawData instanceof Buffer 
                ? rawData.buffer.slice(rawData.byteOffset, rawData.byteOffset + rawData.byteLength) 
                : rawData;
                
            if (buffer.byteLength < this.#HEADER_SIZE) return null;

            const view = new DataView(buffer);
            const opCode = view.getUint8(0);
            const seqId = view.getUint32(1, true);
            const payloadSize = view.getUint32(5, true);

            // Zero-Trust Guard: Tấn công tràn bộ đệm
            if (payloadSize > this.#MAX_PAYLOAD_SIZE) {
                logger.error({ payloadSize }, "VoiceGuard: Phát hiện Payload ngoại cỡ. Drop TCP!");
                return null;
            }

            if (buffer.byteLength !== this.#HEADER_SIZE + payloadSize) {
                logger.warn({ expected: this.#HEADER_SIZE + payloadSize, actual: buffer.byteLength }, "VoiceGuard: Frame rách (Corrupted)");
                return null;
            }

            const payload = new Uint8Array(buffer, this.#HEADER_SIZE, payloadSize);
            return { opCode, seqId, payload };
        } catch (error: any) {
            logger.error({ err: error.message }, "VoiceBinaryProtocol: Decode thất bại");
            return null;
        }
    }
}
