import { describe, it, expect, vi, beforeEach } from 'vitest';
import { EventEmitter } from 'events';
import { pipeline } from '@huggingface/transformers';

const mockParentPort = new EventEmitter() as any;
mockParentPort.postMessage = vi.fn();

vi.mock('node:worker_threads', () => ({
    parentPort: mockParentPort
}));

const mockPipelineFunc = vi.fn();
vi.mock('@huggingface/transformers', () => ({
    pipeline: vi.fn()
}));

describe('WhisperWorker', () => {
    beforeEach(async () => {
        vi.resetModules();
        vi.clearAllMocks();
        mockParentPort.removeAllListeners();
        mockParentPort.postMessage.mockClear();
        (pipeline as any).mockResolvedValue(mockPipelineFunc);
        
        // Re-import to re-attach listeners and reset local state
        await import('../../src/workers/WhisperWorker');
    });

    it('should initialize successfully on init message', async () => {
        mockParentPort.emit('message', { type: 'init' });
        await new Promise(r => setTimeout(r, 10)); // Allow async init to finish

        expect(pipeline).toHaveBeenCalledWith('automatic-speech-recognition', 'onnx-community/whisper-base', {
            dtype: 'q8',
            device: 'cpu'
        });
        expect(mockParentPort.postMessage).toHaveBeenCalledWith({ type: 'ready' });
    });

    it('should send error if init fails', async () => {
        (pipeline as any).mockRejectedValueOnce(new Error('Model loading failed'));
        
        mockParentPort.emit('message', { type: 'init' });
        await new Promise(r => setTimeout(r, 10));

        expect(mockParentPort.postMessage).toHaveBeenCalledWith({ type: 'error', message: 'Model loading failed' });
    });

    it('should ignore process messages if not ready', async () => {
        const float32Arr = new Float32Array([0.1, -0.2, 0.3]);
        mockParentPort.emit('message', { type: 'process', buffer: float32Arr.buffer });
        
        await new Promise(r => setTimeout(r, 10));
        
        // Should not call pipeline
        expect(mockPipelineFunc).not.toHaveBeenCalled();
        expect(mockParentPort.postMessage).not.toHaveBeenCalled();
    });

    it('should process audio buffer and send transcription', async () => {
        // Init first
        mockParentPort.emit('message', { type: 'init' });
        await new Promise(r => setTimeout(r, 10));

        mockPipelineFunc.mockResolvedValueOnce({ text: '  Hello World  ' });

        const float32Arr = new Float32Array([0.1, -0.2, 0.3]);
        mockParentPort.emit('message', { type: 'process', buffer: float32Arr.buffer });
        
        await new Promise(r => setTimeout(r, 10));

        // It should call pipeline with Blob and proper args
        expect(mockPipelineFunc).toHaveBeenCalledWith(
            expect.any(Blob),
            { language: 'vi', task: 'transcribe' }
        );

        expect(mockParentPort.postMessage).toHaveBeenCalledWith({ type: 'transcription', text: 'Hello World' });
    });

    it('should not send transcription if text is empty', async () => {
        // Init first
        mockParentPort.emit('message', { type: 'init' });
        await new Promise(r => setTimeout(r, 10));

        mockPipelineFunc.mockResolvedValueOnce({ text: '    ' });

        const float32Arr = new Float32Array([0.1, -0.2, 0.3]);
        mockParentPort.emit('message', { type: 'process', buffer: float32Arr.buffer });
        
        await new Promise(r => setTimeout(r, 10));

        // Wait, ready message was sent during init
        expect(mockParentPort.postMessage).toHaveBeenCalledTimes(1);
        expect(mockParentPort.postMessage).toHaveBeenCalledWith({ type: 'ready' });
    });

    it('should catch and send error during processing', async () => {
        // Init first
        mockParentPort.emit('message', { type: 'init' });
        await new Promise(r => setTimeout(r, 10));

        mockPipelineFunc.mockRejectedValueOnce(new Error('Inference error'));

        const float32Arr = new Float32Array([0.1, -0.2, 0.3]);
        mockParentPort.emit('message', { type: 'process', buffer: float32Arr.buffer });
        
        await new Promise(r => setTimeout(r, 10));

        expect(mockParentPort.postMessage).toHaveBeenCalledWith({ type: 'error', message: 'Inference error' });
    });

    it('should ignore unknown message types (Line 59)', async () => {
        mockParentPort.emit('message', { type: 'unknown' });
        await new Promise(r => setTimeout(r, 10));
        expect(pipeline).not.toHaveBeenCalled();
    });

    it('should handle missing text in result gracefully (Line 73)', async () => {
        mockParentPort.emit('message', { type: 'init' });
        await new Promise(r => setTimeout(r, 10));

        mockPipelineFunc.mockResolvedValueOnce({}); // No text field

        const float32Arr = new Float32Array([0.1]);
        mockParentPort.emit('message', { type: 'process', buffer: float32Arr.buffer });
        
        await new Promise(r => setTimeout(r, 10));

        expect(mockParentPort.postMessage).toHaveBeenCalledTimes(1); // Only the 'ready' message
    });
});
