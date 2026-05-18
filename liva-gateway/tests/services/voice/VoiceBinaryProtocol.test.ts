import { describe, it, expect, vi } from "vitest";
import { VoiceBinaryProtocol, VOICE_OPCODES } from "../../../src/services/voice/VoiceBinaryProtocol";
import { logger } from "../../../src/utils/logger";

vi.mock("../../../src/utils/logger", () => ({
    logger: {
        error: vi.fn(),
        warn: vi.fn()
    }
}));

describe("VoiceBinaryProtocol", () => {
    it("should encode and decode a frame correctly", () => {
        const payload = new Uint8Array([1, 2, 3, 4, 5]);
        const frame = VoiceBinaryProtocol.encodeFrame(VOICE_OPCODES.MIC_IN, 1234, payload);
        
        const decoded = VoiceBinaryProtocol.decodeFrame(frame.buffer);
        
        expect(decoded).toBeDefined();
        expect(decoded?.opCode).toBe(VOICE_OPCODES.MIC_IN);
        expect(decoded?.seqId).toBe(1234);
        expect(decoded?.payload).toEqual(payload);
    });

    it("should reject corrupted frames", () => {
        const payload = new Uint8Array([1, 2, 3, 4, 5]);
        const frame = VoiceBinaryProtocol.encodeFrame(VOICE_OPCODES.MIC_IN, 1234, payload);
        
        // Corrupt frame by removing bytes
        const corruptedFrame = frame.slice(0, frame.length - 2);
        
        const decoded = VoiceBinaryProtocol.decodeFrame(corruptedFrame.buffer);
        expect(decoded).toBeNull();
        expect(logger.warn).toHaveBeenCalledWith(
            expect.objectContaining({ expected: 14, actual: 12 }), 
            "VoiceGuard: Frame rách (Corrupted)"
        );
    });

    it("should block WebSocket Binary Frame overflow payload > 1MB", () => {
        const largePayload = new Uint8Array(1024 * 1024 + 1); // 1MB + 1
        const frame = VoiceBinaryProtocol.encodeFrame(VOICE_OPCODES.MIC_IN, 1234, largePayload);
        
        const decoded = VoiceBinaryProtocol.decodeFrame(frame.buffer);
        expect(decoded).toBeNull();
        expect(logger.error).toHaveBeenCalledWith(
            expect.objectContaining({ payloadSize: 1024 * 1024 + 1 }),
            "VoiceGuard: Phát hiện Payload ngoại cỡ. Drop TCP!"
        );
    });

    it("should manage buffer pool", () => {
        const protocol = new VoiceBinaryProtocol(2, 100);
        const buf1 = protocol.acquireBuffer(100);
        const buf2 = protocol.acquireBuffer(100);
        
        expect(buf1.length).toBe(100);
        expect(buf2.length).toBe(100);
        
        // Fallback size allocation
        const buf3 = protocol.acquireBuffer(200);
        expect(buf3.length).toBe(200);

        protocol.releaseBuffer(buf1);
        const buf4 = protocol.acquireBuffer(100);
        expect(buf4).toBe(buf1); // should reuse
    });
});
