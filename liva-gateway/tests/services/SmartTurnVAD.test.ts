import { describe, it, expect, vi, beforeEach } from "vitest";

// DEV GUARD: Bắt buộc dùng vi.mock('onnxruntime-web') để chặn khởi tạo engine thực tế trong Node.js
vi.mock("onnxruntime-web", () => {
    return {
        env: {
            wasm: { numThreads: 1 }
        },
        InferenceSession: {
            create: vi.fn().mockResolvedValue({
                run: vi.fn().mockImplementation(async ({ input }) => {
                    return {
                        output: {
                            data: [0.5] // Default confidence < 0.7
                        }
                    };
                })
            })
        },
        Tensor: class {
            type: string;
            data: Float32Array;
            dims: number[];
            constructor(type: string, data: Float32Array, dims: number[]) {
                this.type = type;
                this.data = data;
                this.dims = dims;
            }
        }
    };
});

import { SmartTurnVAD } from "../../src/services/SmartTurnVAD";

describe("SmartTurnVAD", () => {
    let vad: SmartTurnVAD;

    let mockRun: any;

    beforeEach(async () => {
        vi.clearAllMocks();
        vad = new SmartTurnVAD();
        const ort = await import("onnxruntime-web");
        
        mockRun = vi.fn().mockImplementation(async ({ input }) => {
            return {
                output: {
                    data: [0.5] // Default confidence < 0.7
                }
            };
        });

        // Mock create to return our custom mockRun
        vi.mocked(ort.InferenceSession.create).mockResolvedValue({
            run: mockRun
        } as any);

        await vad.initialize("fake_path.onnx");
    });

    it("Ring Buffer Truncation (OOM Guard): should automatically slice off the oldest audio when exceeding 30s", async () => {
        const framesLimit = 16000 * 30; // 480,000 frames limit
        const excessFrames = 1000;
        const totalInput = framesLimit + excessFrames;

        // Tao 1 mang audio lon hon muc cho phep (481,000 frames)
        const massiveChunk = new Float32Array(totalInput);
        
        // Dien du lieu de kiem tra viec xoay vong. 
        // Sau khi vuot qua 480k, 1000 frames tiep theo se ghi vao index 0 -> 999.
        for (let i = 0; i < totalInput; i++) {
            massiveChunk[i] = i; 
        }

        // Chay chunk vao he thong
        const result = await vad.processAudioChunk(massiveChunk);

        // Khong vuot qua nguong nen chua reset buffer
        expect(result.isTurnEnd).toBe(false);
        
        // Lay tensor input tu viec goi onnx session
        // mockRun da duoc capture o beforeEach
        
        // Input phai co do dai dung bang MAX_BUFFER_FRAMES mac du dua vao nhieu hon
        const passedTensor = mockRun.mock.calls[0][0].input as any;
        expect(passedTensor.data.length).toBe(framesLimit);
        
        // Kiem tra gia tri xoay vong (vi no luon tra ve oldest -> newest)
        // Khi buffer ghi den 480k, sau do 1k vao index 0..999
        // Vay oldest data hien tai bat dau tu index 1000, value = 1000
        expect(passedTensor.data[0]).toBe(1000); // oldest surviving frame
        expect(passedTensor.data[framesLimit - 1]).toBe(totalInput - 1); // newest frame
    });

    it("should return false if disposed", async () => {
        vad.dispose();
        const result = await vad.processAudioChunk(new Float32Array(100));
        expect(result.confidence).toBe(0);
        expect(result.isTurnEnd).toBe(false);
    });

    it("should trigger turn end and reset buffer when confidence > 0.7", async () => {
        // Thay doi ket qua cua mockRun cho test nay
        mockRun.mockResolvedValueOnce({
            output: {
                data: [0.85] // > 0.7
            }
        });

        const result = await vad.processAudioChunk(new Float32Array(5000));
        expect(result.confidence).toBe(0.85);
        expect(result.isTurnEnd).toBe(true);
    });
});
