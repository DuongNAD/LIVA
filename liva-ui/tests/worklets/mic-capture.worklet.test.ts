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
    expect(name).toBe("liva-mic-capture");
    Processor = constructor;
  };

  await import("../../src/worklets/mic-capture.worklet.js");
});

describe("liva-mic-capture AudioWorklet", () => {
  it("aggregates two render quanta into one transferable 16 ms frame by default", () => {
    const processor = new Processor({});
    const output = new Float32Array(128);
    output.fill(1);

    for (let quantum = 0; quantum < 2; quantum += 1) {
      const input = new Float32Array(128);
      input.fill(quantum + 1);
      expect(processor.process([[input]], [[output]])).toBe(true);
    }

    expect(processor.port.postMessage).toHaveBeenCalledOnce();
    const [frame, transferList] = processor.port.postMessage.mock.calls[0] as [
      Float32Array,
      ArrayBuffer[],
    ];
    expect(frame).toHaveLength(256);
    expect(Array.from(frame.slice(0, 4))).toEqual([1, 1, 1, 1]);
    expect(Array.from(frame.slice(128, 132))).toEqual([2, 2, 2, 2]);
    expect(transferList).toEqual([frame.buffer]);
    expect(output.every((sample) => sample === 0)).toBe(true);
  });

  it("preserves support for custom frameSize (e.g. 512 samples / 4 render quanta)", () => {
    const processor = new Processor({
      processorOptions: { frameSize: 512 },
    });
    const output = new Float32Array(128);

    for (let quantum = 0; quantum < 4; quantum += 1) {
      const input = new Float32Array(128);
      input.fill(quantum + 1);
      expect(processor.process([[input]], [[output]])).toBe(true);
    }

    expect(processor.port.postMessage).toHaveBeenCalledOnce();
    const [frame] = processor.port.postMessage.mock.calls[0] as [Float32Array];
    expect(frame).toHaveLength(512);
    expect(Array.from(frame.slice(0, 4))).toEqual([1, 1, 1, 1]);
    expect(Array.from(frame.slice(384, 388))).toEqual([4, 4, 4, 4]);
  });
});
