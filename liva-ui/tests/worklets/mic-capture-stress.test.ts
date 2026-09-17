import { beforeAll, describe, expect, it, vi } from "vitest";

type WorkletConstructor = new (options?: {
  processorOptions?: { frameSize?: number };
}) => {
  port: { postMessage: ReturnType<typeof vi.fn> };
  process: (inputs: Float32Array[][], outputs: Float32Array[][]) => boolean;
};

let Processor: WorkletConstructor;

beforeAll(async () => {
  class MockAudioWorkletProcessor {
    port = {
      postMessage: vi.fn(),
    };
  }

  (globalThis as Record<string, unknown>).AudioWorkletProcessor =
    MockAudioWorkletProcessor;
  (globalThis as Record<string, unknown>).registerProcessor = (
    name: string,
    constructor: WorkletConstructor,
  ) => {
    if (name === "liva-mic-capture") {
      Processor = constructor;
    }
  };

  await import("../../src/worklets/mic-capture.worklet.js");
});

describe("LivaMicCaptureProcessor — Empirical Stress & Boundary Harness", () => {
  it("processes 10,000 continuous render quanta (128 samples) into exactly 5,000 frames bit-for-bit", () => {
    const processor = new Processor();
    const emittedFrames: Float32Array[] = [];
    const transferLists: (Transferable[] | undefined)[] = [];

    processor.port.postMessage = vi.fn((frame: Float32Array, transfer?: Transferable[]) => {
      emittedFrames.push(new Float32Array(frame));
      transferLists.push(transfer);
    });

    const TOTAL_QUANTA = 10000;
    const QUANTUM_SIZE = 128;
    const EXPECTED_FRAMES = 5000;
    const FRAME_SIZE = 256;

    const totalSamples = TOTAL_QUANTA * QUANTUM_SIZE;
    const groundTruth = new Float32Array(totalSamples);
    for (let i = 0; i < totalSamples; i++) {
      groundTruth[i] = Math.fround(Math.sin(i * 0.05) * 0.8 + ((i % 257) - 128) / 1000);
    }

    const output = new Float32Array(QUANTUM_SIZE);

    for (let q = 0; q < TOTAL_QUANTA; q++) {
      const inputQuantum = groundTruth.subarray(q * QUANTUM_SIZE, (q + 1) * QUANTUM_SIZE);
      output.fill(0.5);
      const keepAlive = processor.process([[inputQuantum]], [[output]]);
      expect(keepAlive).toBe(true);
    }

    expect(emittedFrames).toHaveLength(EXPECTED_FRAMES);

    const gtUintView = new Uint32Array(groundTruth.buffer, groundTruth.byteOffset, groundTruth.length);

    let mismatchCount = 0;
    for (let f = 0; f < EXPECTED_FRAMES; f++) {
      const frame = emittedFrames[f];
      expect(frame).toHaveLength(FRAME_SIZE);
      expect(transferLists[f]).toHaveLength(1);

      const frameUintView = new Uint32Array(frame.buffer, frame.byteOffset, frame.length);
      const expectedOffset = f * FRAME_SIZE;

      for (let s = 0; s < FRAME_SIZE; s++) {
        if (frameUintView[s] !== gtUintView[expectedOffset + s]) {
          mismatchCount++;
        }
      }
    }
    expect(mismatchCount).toBe(0);
  }, 15000);

  it("handles non-standard quantum sizes (64, 256, 384) with exact boundary safety", () => {
    // 1. Quantum size 64: 500 quanta -> 125 frames of 256
    const p64 = new Processor();
    const frames64: Float32Array[] = [];
    p64.port.postMessage = vi.fn((f: Float32Array) => frames64.push(new Float32Array(f)));

    for (let q = 0; q < 500; q++) {
      const in64 = new Float32Array(64);
      in64.fill(q + 1);
      p64.process([[in64]], [[]]);
    }
    expect(frames64).toHaveLength(125);

    // 2. Quantum size 384 (1.5 frames per quantum): 100 quanta -> 150 frames of 256
    const p384 = new Processor();
    const frames384: Float32Array[] = [];
    p384.port.postMessage = vi.fn((f: Float32Array) => frames384.push(new Float32Array(f)));

    for (let q = 0; q < 100; q++) {
      const in384 = new Float32Array(384);
      in384.fill(0.123);
      p384.process([[in384]], [[]]);
    }
    expect(frames384).toHaveLength(150);
  });

  it("handles empty, nullish, and zero-length inputs gracefully", () => {
    const processor = new Processor();
    let emitCount = 0;
    processor.port.postMessage = vi.fn(() => emitCount++);

    expect(processor.process([], [])).toBe(true);
    expect(processor.process([[]], [[]])).toBe(true);
    expect(processor.process([[undefined as unknown as Float32Array]], [[]])).toBe(true);
    expect(processor.process([[new Float32Array(0)]], [[]])).toBe(true);
    expect(emitCount).toBe(0);

    // Resumption after empty inputs
    const valid = new Float32Array(256);
    valid.fill(0.9);
    expect(processor.process([[valid]], [[]])).toBe(true);
    expect(emitCount).toBe(1);
  });

  it("guarantees output muting to prevent speaker feedback loop", () => {
    const processor = new Processor();
    const output = new Float32Array(128);
    output.fill(0.88);

    const input = new Float32Array(128);
    input.fill(0.44);

    expect(processor.process([[input]], [[output]])).toBe(true);
    expect(output.every((sample) => sample === 0)).toBe(true);
  });

  it("preserves IEEE-754 special values (NaN, +/-Infinity, subnormals) without corruption", () => {
    const processor = new Processor();
    let emittedFrame: Float32Array | null = null;
    processor.port.postMessage = vi.fn((f: Float32Array) => {
      emittedFrame = new Float32Array(f);
    });

    const special = new Float32Array(256);
    special[0] = NaN;
    special[1] = Infinity;
    special[2] = -Infinity;
    special[3] = -0.0;
    special[4] = 1.401298464324817e-45;

    processor.process([[special]], [[]]);
    expect(emittedFrame).not.toBeNull();
    if (emittedFrame) {
      const frame: Float32Array = emittedFrame;
      expect(Number.isNaN(frame[0])).toBe(true);
      expect(frame[1]).toBe(Infinity);
      expect(frame[2]).toBe(-Infinity);
      expect(Object.is(frame[3], -0.0)).toBe(true);
      expect(frame[4]).toBe(1.401298464324817e-45);
    }
  });
});
