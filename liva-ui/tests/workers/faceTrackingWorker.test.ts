import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";

// ════════════════════════════════════════════════════════════════════════════
//  Mocks for MediaPipe Tasks Vision & Web Worker Environment
// ════════════════════════════════════════════════════════════════════════════

const mockDetect = vi.fn();
const mockCloseLandmarker = vi.fn();

const mockLandmarkerInstance = {
  detect: mockDetect,
  close: mockCloseLandmarker,
};

const mockCreateFromOptions = vi.fn().mockResolvedValue(mockLandmarkerInstance);
const mockForVisionTasks = vi.fn().mockResolvedValue({ dummyWasm: true });

vi.mock("@mediapipe/tasks-vision", () => ({
  FaceLandmarker: {
    createFromOptions: (...args: any[]) => mockCreateFromOptions(...args),
  },
  FilesetResolver: {
    forVisionTasks: (...args: any[]) => mockForVisionTasks(...args),
  },
}));

// ════════════════════════════════════════════════════════════════════════════
//  Test Helpers: 478 Face Landmarks & 52 ARKit Blendshape Categories
// ════════════════════════════════════════════════════════════════════════════

/**
 * Generate 478 normalized 3D landmarks.
 * Key indices for estimateHeadPose:
 * - noseTip: 1
 * - leftEye: 33
 * - rightEye: 263
 * - chin: 152
 * - forehead: 10
 */
function createMockLandmarks(
  overrides: Partial<Record<number, { x: number; y: number; z: number }>> = {}
) {
  const landmarks: { x: number; y: number; z: number }[] = [];
  for (let i = 0; i < 478; i++) {
    landmarks.push(overrides[i] ?? { x: 0.5, y: 0.5, z: 0 });
  }
  return landmarks;
}

/** Complete list of 52 ARKit-compatible blendshape category names */
const ARKIT_BLENDSHAPES_52 = [
  "_neutral",
  "browDownLeft",
  "browDownRight",
  "browInnerUp",
  "browOuterUpLeft",
  "browOuterUpRight",
  "cheekPuff",
  "cheekSquintLeft",
  "cheekSquintRight",
  "eyeBlinkLeft",
  "eyeBlinkRight",
  "eyeLookDownLeft",
  "eyeLookDownRight",
  "eyeLookInLeft",
  "eyeLookInRight",
  "eyeLookOutLeft",
  "eyeLookOutRight",
  "eyeLookUpLeft",
  "eyeLookUpRight",
  "eyeSquintLeft",
  "eyeSquintRight",
  "eyeWideLeft",
  "eyeWideRight",
  "jawForward",
  "jawLeft",
  "jawOpen",
  "jawRight",
  "mouthClose",
  "mouthDimpleLeft",
  "mouthDimpleRight",
  "mouthFrownLeft",
  "mouthFrownRight",
  "mouthFunnel",
  "mouthLeft",
  "mouthLowerDownLeft",
  "mouthLowerDownRight",
  "mouthPressLeft",
  "mouthPressRight",
  "mouthPucker",
  "mouthRight",
  "mouthRollLower",
  "mouthRollUpper",
  "mouthShrugLower",
  "mouthShrugUpper",
  "mouthSmileLeft",
  "mouthSmileRight",
  "mouthStretchLeft",
  "mouthStretchRight",
  "mouthUpperUpLeft",
  "mouthUpperUpRight",
  "noseSneerLeft",
  "noseSneerRight",
];

function createARKitBlendshapes(overrides: Record<string, number> = {}) {
  return ARKIT_BLENDSHAPES_52.map((categoryName) => ({
    categoryName,
    score: overrides[categoryName] ?? 0.0,
  }));
}

function createMockBitmap(withClose = true): ImageBitmap & { close?: ReturnType<typeof vi.fn> } {
  if (!withClose) {
    return {
      width: 640,
      height: 480,
    } as unknown as ImageBitmap;
  }
  return {
    width: 640,
    height: 480,
    close: vi.fn(),
  } as unknown as ImageBitmap & { close: ReturnType<typeof vi.fn> };
}

// ════════════════════════════════════════════════════════════════════════════
//  Test Suite: faceTrackingWorker
// ════════════════════════════════════════════════════════════════════════════

describe("faceTrackingWorker", () => {
  let originalSelf: any;
  let postMessageMock: ReturnType<typeof vi.fn>;
  let closeMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.resetModules();
    originalSelf = globalThis.self;

    postMessageMock = vi.fn();
    closeMock = vi.fn();

    globalThis.self = {
      postMessage: postMessageMock,
      close: closeMock,
    } as any;

    mockDetect.mockReset();
    mockCloseLandmarker.mockReset();
    mockCreateFromOptions.mockReset().mockResolvedValue(mockLandmarkerInstance);
    mockForVisionTasks.mockReset().mockResolvedValue({ dummyWasm: true });
  });

  afterEach(() => {
    globalThis.self = originalSelf;
  });

  // ──────────────────────────────────────────────────────────────────────────
  // 1. Worker Initialization & Ready Handshake
  // ──────────────────────────────────────────────────────────────────────────
  describe("1. Worker Initialization & Ready Handshake", () => {
    it("initializes MediaPipe FaceLandmarker with GPU delegate and emits 'ready'", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      expect(onmessage).toBeDefined();

      await onmessage({
        data: {
          type: "init",
          wasmPath: "/custom/wasm",
          modelAssetPath: "/custom/models/face_landmarker.task",
        },
      });

      expect(mockForVisionTasks).toHaveBeenCalledWith("/custom/wasm");
      expect(mockCreateFromOptions).toHaveBeenCalledWith(
        { dummyWasm: true },
        {
          baseOptions: {
            modelAssetPath: "/custom/models/face_landmarker.task",
            delegate: "GPU",
          },
          runningMode: "IMAGE",
          numFaces: 1,
          outputFaceBlendshapes: true,
          outputFacialTransformationMatrixes: false,
        }
      );

      expect(postMessageMock).toHaveBeenCalledWith({ type: "ready" });
    });

    it("uses default paths when wasmPath and modelAssetPath are omitted", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;

      await onmessage({ data: { type: "init" } });

      expect(mockForVisionTasks).toHaveBeenCalledWith("/assets/wasm");
      expect(mockCreateFromOptions).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining({
          baseOptions: expect.objectContaining({
            modelAssetPath: "/assets/models/face_landmarker.task",
            delegate: "GPU",
          }),
        })
      );
      expect(postMessageMock).toHaveBeenCalledWith({ type: "ready" });
    });

    it("prevents double initialization when already initialized", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;

      await onmessage({ data: { type: "init" } });
      expect(postMessageMock).toHaveBeenCalledTimes(1);

      // Second init call
      await onmessage({ data: { type: "init" } });
      // Should not call createFromOptions again
      expect(mockCreateFromOptions).toHaveBeenCalledTimes(1);
      expect(postMessageMock).toHaveBeenCalledTimes(1);
    });

    it("handles initialization errors gracefully and posts error message", async () => {
      mockCreateFromOptions.mockRejectedValueOnce(
        new Error("WebGL context not available")
      );

      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;

      await onmessage({ data: { type: "init" } });

      expect(postMessageMock).toHaveBeenCalledWith({
        type: "error",
        error: "WebGL context not available",
      });
    });

    it("handles non-Error exceptions during initialization", async () => {
      mockCreateFromOptions.mockRejectedValueOnce("Primitive string failure");

      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;

      await onmessage({ data: { type: "init" } });

      expect(postMessageMock).toHaveBeenCalledWith({
        type: "error",
        error: "Primitive string failure",
      });
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // 2. Video Frame Transfer via postMessage & Bitmap Cleanup
  // ──────────────────────────────────────────────────────────────────────────
  describe("2. Video Frame Transfer via postMessage & Bitmap Cleanup", () => {
    it("passes transferred bitmap to landmarker.detect and closes bitmap immediately", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      const mockBitmap = createMockBitmap();
      mockDetect.mockReturnValue({
        faceLandmarks: [createMockLandmarks()],
        faceBlendshapes: [{ categories: createARKitBlendshapes() }],
      });

      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 16.6,
        },
      });

      expect(mockDetect).toHaveBeenCalledWith(mockBitmap);
      expect(mockBitmap.close).toHaveBeenCalledTimes(1);
      expect(postMessageMock).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "result",
          data: expect.objectContaining({ isDetected: true }),
        })
      );
    });

    it("handles transferred bitmap without close method in successful detection", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      const mockBitmap = createMockBitmap(false);
      mockDetect.mockReturnValue({
        faceLandmarks: [createMockLandmarks()],
        faceBlendshapes: [{ categories: createARKitBlendshapes() }],
      });

      await expect(
        onmessage({
          data: {
            type: "detect",
            bitmap: mockBitmap,
            timestamp: 20.0,
          },
        })
      ).resolves.not.toThrow();

      expect(mockDetect).toHaveBeenCalledWith(mockBitmap);
      expect(postMessageMock).toHaveBeenCalledWith(
        expect.objectContaining({ type: "result" })
      );
    });

    it("safely closes bitmap and ignores detection if worker is not yet initialized", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;

      const mockBitmap = createMockBitmap();

      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 16.6,
        },
      });

      expect(mockDetect).not.toHaveBeenCalled();
      expect(mockBitmap.close).toHaveBeenCalledTimes(1);
      expect(postMessageMock).not.toHaveBeenCalled();
    });

    it("handles bitmap without close method before initialization gracefully", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;

      const mockBitmap = createMockBitmap(false);

      await expect(
        onmessage({
          data: {
            type: "detect",
            bitmap: mockBitmap,
            timestamp: 16.6,
          },
        })
      ).resolves.not.toThrow();

      expect(mockDetect).not.toHaveBeenCalled();
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // 3. 60 FPS Throughput Simulation (16.6ms intervals) & Memory Leak Protection
  // ──────────────────────────────────────────────────────────────────────────
  describe("3. 60 FPS Throughput Simulation (16.6ms intervals)", () => {
    it("processes 60 consecutive frames at 16.6ms intervals with 100% throughput and closes all bitmaps", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });
      postMessageMock.mockClear();

      const FRAME_COUNT = 60;
      const bitmaps = Array.from({ length: FRAME_COUNT }, () => createMockBitmap());

      mockDetect.mockReturnValue({
        faceLandmarks: [createMockLandmarks()],
        faceBlendshapes: [{ categories: createARKitBlendshapes() }],
      });

      const startTime = 1000.0;
      for (let i = 0; i < FRAME_COUNT; i++) {
        const frameTimestamp = startTime + i * 16.666;
        await onmessage({
          data: {
            type: "detect",
            bitmap: bitmaps[i],
            timestamp: frameTimestamp,
          },
        });
      }

      // Verify all 60 results were dispatched
      expect(postMessageMock).toHaveBeenCalledTimes(FRAME_COUNT);

      // Verify every single bitmap was cleanly closed (zero memory leak across 60 frames)
      for (let i = 0; i < FRAME_COUNT; i++) {
        expect(bitmaps[i].close).toHaveBeenCalledTimes(1);
      }

      // Verify consecutive calls
      expect(mockDetect).toHaveBeenCalledTimes(FRAME_COUNT);
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // 4. 52 ARKit Blendshapes & Landmark Head Pose Parsing
  // ──────────────────────────────────────────────────────────────────────────
  describe("4. 52 ARKit Blendshapes & Landmark Head Pose Parsing", () => {
    it("extracts and maps all 52 ARKit blendshapes into high-level avatar expressions", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      const blendshapes = createARKitBlendshapes({
        mouthSmileLeft: 0.85,
        mouthSmileRight: 0.75,
        mouthFrownLeft: 0.10,
        mouthFrownRight: 0.10,
        jawOpen: 0.60,
        eyeBlinkLeft: 0.90,
        eyeBlinkRight: 0.10,
        browInnerUp: 0.40,
        browDownLeft: 0.70,
        browDownRight: 0.70,
        browOuterUpLeft: 0.50,
        browOuterUpRight: 0.50,
      });

      // Override key landmarks to test head pose geometry
      const landmarks = createMockLandmarks({
        1: { x: 0.45, y: 0.50, z: 0 },   // noseTip
        33: { x: 0.40, y: 0.50, z: 0 },  // leftEye
        263: { x: 0.60, y: 0.50, z: 0 }, // rightEye
        152: { x: 0.50, y: 0.60, z: 0 }, // chin
        10: { x: 0.50, y: 0.40, z: 0 },  // forehead
      });

      mockDetect.mockReturnValue({
        faceLandmarks: [landmarks],
        faceBlendshapes: [{ categories: blendshapes }],
      });

      const mockBitmap = createMockBitmap();
      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 100,
        },
      });

      const lastCall = postMessageMock.mock.calls[postMessageMock.mock.calls.length - 1][0];
      expect(lastCall.type).toBe("result");
      expect(lastCall.data.isDetected).toBe(true);
      expect(lastCall.data.confidence).toBe(0.95);

      // Verify expression mappings derived from ARKit blendshapes
      const expr = lastCall.data.expressions;
      expect(expr.happy).toBeCloseTo(0.80, 2);      // (0.85 + 0.75) / 2
      expect(expr.sad).toBeCloseTo(0.10, 2);        // (0.10 + 0.10) / 2
      expect(expr.mouthOpen).toBeCloseTo(0.60, 2);  // jawOpen
      expect(expr.blinkLeft).toBeCloseTo(0.90, 2);
      expect(expr.blinkRight).toBeCloseTo(0.10, 2);
      expect(expr.blink).toBeCloseTo(0.50, 2);       // (0.90 + 0.10) / 2
      expect(expr.surprised).toBeCloseTo(0.50, 2);   // (0.40 + 0.60) / 2
      expect(expr.angry).toBeCloseTo(0.70, 2);       // (0.70 + 0.70) / 2
      expect(expr.browUpLeft).toBeCloseTo(0.50, 2);
      expect(expr.browUpRight).toBeCloseTo(0.50, 2);

      // Verify head pose
      const head = lastCall.data.head;
      expect(head).toHaveProperty("yaw");
      expect(head).toHaveProperty("pitch");
      expect(head).toHaveProperty("roll");
      expect(typeof head.yaw).toBe("number");
      expect(typeof head.pitch).toBe("number");
      expect(typeof head.roll).toBe("number");
    });

    it("falls back to default expressions when faceBlendshapes are empty or omitted", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      mockDetect.mockReturnValue({
        faceLandmarks: [createMockLandmarks()],
        faceBlendshapes: [],
      });

      const mockBitmap = createMockBitmap();
      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 200,
        },
      });

      const lastCall = postMessageMock.mock.calls[postMessageMock.mock.calls.length - 1][0];
      expect(lastCall.type).toBe("result");
      expect(lastCall.data.isDetected).toBe(true);
      expect(lastCall.data.expressions.happy).toBe(0);
      expect(lastCall.data.expressions.mouthOpen).toBe(0);
      expect(lastCall.data.expressions.blink).toBe(0);
    });

    it("falls back to default expressions when faceBlendshapes property is undefined", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      mockDetect.mockReturnValue({
        faceLandmarks: [createMockLandmarks()],
        faceBlendshapes: undefined as any,
      });

      const mockBitmap = createMockBitmap();
      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 250,
        },
      });

      const lastCall = postMessageMock.mock.calls[postMessageMock.mock.calls.length - 1][0];
      expect(lastCall.type).toBe("result");
      expect(lastCall.data.isDetected).toBe(true);
      expect(lastCall.data.expressions.happy).toBe(0);
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // 5. Graceful Handling of No-Face Detected Frames
  // ──────────────────────────────────────────────────────────────────────────
  describe("5. Graceful Handling of No-Face Detected Frames", () => {
    it("emits neutral face tracking data with isDetected: false when faceLandmarks is empty", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      mockDetect.mockReturnValue({
        faceLandmarks: [],
        faceBlendshapes: [],
      });

      const mockBitmap = createMockBitmap();
      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 300,
        },
      });

      expect(mockBitmap.close).toHaveBeenCalledTimes(1);

      const lastCall = postMessageMock.mock.calls[postMessageMock.mock.calls.length - 1][0];
      expect(lastCall).toEqual({
        type: "result",
        data: {
          isDetected: false,
          head: { yaw: 0, pitch: 0, roll: 0 },
          expressions: {
            happy: 0,
            sad: 0,
            surprised: 0,
            angry: 0,
            blink: 0,
            blinkLeft: 0,
            blinkRight: 0,
            mouthOpen: 0,
            browUpLeft: 0,
            browUpRight: 0,
          },
          confidence: 0,
        },
      });
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // 6. Error Isolation & Fault Tolerance
  // ──────────────────────────────────────────────────────────────────────────
  describe("6. Error Isolation & Fault Tolerance", () => {
    it("catches detection exception, closes bitmap, posts error, and stays alive", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      mockDetect.mockImplementationOnce(() => {
        throw new Error("GPU context lost during landmark inference");
      });

      const mockBitmap = createMockBitmap();
      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 400,
        },
      });

      // Bitmap MUST be closed even on exception to prevent VRAM leaks
      expect(mockBitmap.close).toHaveBeenCalledTimes(1);

      // Error message sent to main thread
      expect(postMessageMock).toHaveBeenCalledWith({
        type: "error",
        error: "GPU context lost during landmark inference",
      });

      // Verify worker remains functional on next frame
      mockDetect.mockReturnValueOnce({
        faceLandmarks: [createMockLandmarks()],
        faceBlendshapes: [{ categories: createARKitBlendshapes() }],
      });

      const nextBitmap = createMockBitmap();
      await onmessage({
        data: {
          type: "detect",
          bitmap: nextBitmap,
          timestamp: 416.6,
        },
      });

      expect(nextBitmap.close).toHaveBeenCalledTimes(1);
      const lastCall = postMessageMock.mock.calls[postMessageMock.mock.calls.length - 1][0];
      expect(lastCall.type).toBe("result");
      expect(lastCall.data.isDetected).toBe(true);
    });

    it("handles non-Error exception and bitmap without close method on detection failure", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      mockDetect.mockImplementationOnce(() => {
        throw "Hardware buffer allocation failed";
      });

      const mockBitmap = createMockBitmap(false);
      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 450,
        },
      });

      expect(postMessageMock).toHaveBeenCalledWith({
        type: "error",
        error: "Hardware buffer allocation failed",
      });
    });

    it("ignores null or empty message data without crashing", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;

      await expect(onmessage({ data: null as any })).resolves.not.toThrow();
      await expect(onmessage({ data: undefined as any })).resolves.not.toThrow();
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // 7. Worker Termination & Memory Cleanup (dispose)
  // ──────────────────────────────────────────────────────────────────────────
  describe("7. Worker Termination & Memory Cleanup (dispose)", () => {
    it("closes landmarker and calls self.close() on dispose message", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      await onmessage({ data: { type: "init" } });

      await onmessage({ data: { type: "dispose" } });

      expect(mockCloseLandmarker).toHaveBeenCalledTimes(1);
      expect(closeMock).toHaveBeenCalledTimes(1);

      // Subsequent detect call after dispose should safely close bitmap and return
      const mockBitmap = createMockBitmap();
      await onmessage({
        data: {
          type: "detect",
          bitmap: mockBitmap,
          timestamp: 500,
        },
      });

      expect(mockBitmap.close).toHaveBeenCalledTimes(1);
      expect(mockDetect).not.toHaveBeenCalled();
    });

    it("handles dispose cleanly even if init was never called", async () => {
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;

      await onmessage({ data: { type: "dispose" } });

      expect(mockCloseLandmarker).not.toHaveBeenCalled();
      expect(closeMock).toHaveBeenCalledTimes(1);
    });
  });
});
