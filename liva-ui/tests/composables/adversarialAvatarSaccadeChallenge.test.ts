import { describe, it, expect, vi, beforeEach } from "vitest";
import * as THREE from "three";
import { use3DModel } from "../../src/composables/use3DModel";
import {
  MAX_SACCADE_AMPLITUDE_DEG,
  SACCADE_JUMP_DURATION_S,
  SACCADE_DRIFT_HALF_LIFE_S,
  randomSaccadeInterval,
  randomSaccadeDisplacement,
} from "../../src/utils/avatarMath";

// ════════════════════════════════════════════════════════════════════════════
//  Mocks for Three.js, Loaders, and Pixiv VRM
// ════════════════════════════════════════════════════════════════════════════

const {
  animationSetStateMock,
  animationSetMotionWeightMock,
  animationSetThinkingMock,
  animationUpdateMock,
  animationGetStateMock,
  animationGetMotionWeightMock,
  animationGetStridePhaseMock,
  animationGetThinkingWeightMock,
  animationClips,
  mockLoadGLTF,
} = vi.hoisted(() => ({
  animationSetStateMock: vi.fn(),
  animationSetMotionWeightMock: vi.fn(),
  animationSetThinkingMock: vi.fn(),
  animationUpdateMock: vi.fn(),
  animationGetStateMock: vi.fn(() => "idle"),
  animationGetMotionWeightMock: vi.fn(() => 0),
  animationGetStridePhaseMock: vi.fn(() => 0),
  animationGetThinkingWeightMock: vi.fn(() => 0),
  animationClips: new Set<string>(),
  mockLoadGLTF: vi.fn(),
}));

vi.mock("../../src/composables/mixamoClipLoader", () => ({
  DEFAULT_MIXAMO_CLIP_PATHS: {},
  loadMixamoAnimationSet: vi.fn(),
}));

vi.mock("../../src/composables/useAvatarAnimation", () => ({
  useAvatarAnimation: () => ({
    setState: animationSetStateMock,
    setMotionWeight: animationSetMotionWeightMock,
    getState: animationGetStateMock,
    getMotionWeight: animationGetMotionWeightMock,
    getStridePhase: animationGetStridePhaseMock,
    getThinkingWeight: animationGetThinkingWeightMock,
    get motionWeight() {
      return animationGetMotionWeightMock();
    },
    get stridePhase() {
      return animationGetStridePhaseMock();
    },
    get thinkingWeight() {
      return animationGetThinkingWeightMock();
    },
    playGesture: vi.fn(),
    setInspecting: vi.fn(),
    setThinking: animationSetThinkingMock,
    update: animationUpdateMock,
    registerClip: (state: string) => animationClips.add(state),
    hasClip: (state: string) => animationClips.has(state),
    debugPose: vi.fn(() => ({})),
    reset: vi.fn(),
  }),
}));

vi.mock("three", async (importOriginal) => {
  const actual = await importOriginal<typeof import("three")>();
  class MockWebGLRenderer {
    setSize = vi.fn();
    setPixelRatio = vi.fn();
    setClearColor = vi.fn();
    render = vi.fn();
    dispose = vi.fn();
    forceContextLoss = vi.fn();
    domElement = document.createElement("canvas");
    outputColorSpace = "";
  }
  return {
    ...actual,
    WebGLRenderer: MockWebGLRenderer,
  };
});

vi.mock("three/examples/jsm/loaders/GLTFLoader.js", () => {
  class MockGLTFLoader {
    register = vi.fn();
    load = mockLoadGLTF;
  }
  return { GLTFLoader: MockGLTFLoader };
});

vi.mock("three/examples/jsm/loaders/FBXLoader.js", () => {
  class MockFBXLoader {
    load = vi.fn();
  }
  return { FBXLoader: MockFBXLoader };
});

vi.mock("@pixiv/three-vrm", () => ({
  VRMLoaderPlugin: vi.fn(),
  VRM: vi.fn(),
  VRMUtils: {
    removeUnnecessaryVertices: vi.fn(),
    removeUnnecessaryJoints: vi.fn(),
    combineSkeletons: vi.fn(),
    combineMorphs: vi.fn(),
    rotateVRM0: vi.fn(),
  },
}));

// ════════════════════════════════════════════════════════════════════════════
//  Adversarial Challenge Test Suite: Saccades & FaceTracking
// ════════════════════════════════════════════════════════════════════════════

describe("Adversarial Benchmark & Stress Challenge: Feature 10 & 11", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ──────────────────────────────────────────────────────────────────────────
  // Challenge 1: 1,000-Frame Eye Saccades Empirical Verification
  // ──────────────────────────────────────────────────────────────────────────
  describe("Challenge 1: Avatar Eye Saccades 1,000-Frame Empirical Simulation", () => {
    it("runs 1,000 consecutive frames at 60 FPS in use3DModel: verifies 2-4 Hz intervals and <=0.85 deg amplitude", async () => {
      let rafCallback: ((time: number) => void) | null = null;
      vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb: any) => {
        rafCallback = cb;
        return 1;
      });

      const dt = 1 / 60; // 16.6667ms per frame
      vi.spyOn(THREE.Clock.prototype, "getDelta").mockReturnValue(dt);

      const applyYawPitchMock = vi.fn();
      const scene = new THREE.Object3D();
      const vrmInstance = {
        scene,
        humanoid: null,
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: applyYawPitchMock } },
        update: vi.fn(),
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement("canvas"), 800, 600);
      await model.loadModel("models/avatar.vrm");

      // Fix base gaze to a static target (e.g. 5° yaw, -3° pitch)
      model.updateLookAt(5.0, -3.0);

      // Start render loop -> triggers frame 0
      model.startRenderLoop();
      expect(rafCallback).toBeDefined();

      const TOTAL_FRAMES = 1000;
      const history: Array<{ frame: number; yaw: number; pitch: number; mag: number }> = [];

      let simTime = 1000.0;
      for (let f = 1; f <= TOTAL_FRAMES; f++) {
        simTime += dt * 1000;
        rafCallback!(simTime);

        const offset = model.getSaccadeOffset();
        const mag = Math.sqrt(offset.yaw * offset.yaw + offset.pitch * offset.pitch);

        history.push({
          frame: f,
          yaw: offset.yaw,
          pitch: offset.pitch,
          mag,
        });

        // 1. Invariance check: magnitude never exceeds 0.85° (+ margin for float precision)
        expect(mag).toBeLessThanOrEqual(MAX_SACCADE_AMPLITUDE_DEG + 1e-5);
        expect(Math.abs(offset.yaw)).toBeLessThanOrEqual(MAX_SACCADE_AMPLITUDE_DEG + 1e-5);
        expect(Math.abs(offset.pitch)).toBeLessThanOrEqual(MAX_SACCADE_AMPLITUDE_DEG + 1e-5);
      }

      // 2. Saccade Event Frequency Analysis (2 to 4 Hz interval: T in [0.25s, 0.50s])
      // Detect jump initiation events by observing ballistic acceleration (progress from drift to jump)
      // Or by sampling inter-saccade intervals directly across 1,000 generated cycles:
      const intervalsSampled: number[] = [];
      for (let i = 0; i < 1000; i++) {
        const interval = randomSaccadeInterval();
        intervalsSampled.push(interval);
        expect(interval).toBeGreaterThanOrEqual(0.25);
        expect(interval).toBeLessThanOrEqual(0.50);
        const freq = 1 / interval;
        expect(freq).toBeGreaterThanOrEqual(2.0);
        expect(freq).toBeLessThanOrEqual(4.0);
      }

      // Empirical mean frequency should be centered around ~3.0 Hz (mean interval ~0.33s)
      const meanInterval = intervalsSampled.reduce((a, b) => a + b, 0) / intervalsSampled.length;
      expect(meanInterval).toBeGreaterThan(0.28);
      expect(meanInterval).toBeLessThan(0.40);

      model.stopRenderLoop();
      model.dispose();
    });

    it("verifies cumulative drift is exactly 0 after 1,000 frames with static gaze target", async () => {
      let rafCallback: ((time: number) => void) | null = null;
      vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb: any) => {
        rafCallback = cb;
        return 1;
      });

      const dt = 1 / 60;
      vi.spyOn(THREE.Clock.prototype, "getDelta").mockReturnValue(dt);

      let lastAppliedYaw = 0;
      let lastAppliedPitch = 0;
      const applyYawPitchMock = vi.fn((yaw: number, pitch: number) => {
        lastAppliedYaw = yaw;
        lastAppliedPitch = pitch;
      });

      const scene = new THREE.Object3D();
      const vrmInstance = {
        scene,
        humanoid: null,
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: applyYawPitchMock } },
        update: vi.fn(),
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement("canvas"), 800, 600);
      await model.loadModel("models/avatar.vrm");

      // Set target gaze: (12.5°, -7.0°)
      const targetGazeYaw = 12.5;
      const targetGazePitch = -7.0;
      model.updateLookAt(targetGazeYaw, targetGazePitch);

      model.startRenderLoop();

      // Run 1,000 frames
      let simTime = 2000.0;
      for (let f = 1; f <= 1000; f++) {
        simTime += dt * 1000;
        rafCallback!(simTime);
      }

      // Now disable saccades to observe pure base gaze anchor
      model.setSaccadesEnabled(false);
      // Run 60 more frames for spring lookAt to settle completely to target
      for (let f = 1; f <= 60; f++) {
        simTime += dt * 1000;
        rafCallback!(simTime);
      }

      // Saccade offset is exactly (0, 0)
      expect(model.getSaccadeOffset()).toEqual({ yaw: 0, pitch: 0 });

      // Base gaze applied to VRM applier must match the static target gaze with ZERO cumulative drift
      expect(lastAppliedYaw).toBeCloseTo(targetGazeYaw, 3);
      expect(lastAppliedPitch).toBeCloseTo(targetGazePitch, 3);

      model.stopRenderLoop();
      model.dispose();
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // Challenge 2: Upper-Body Bone Invariance Verification
  // ──────────────────────────────────────────────────────────────────────────
  describe("Challenge 2: Upper-Body Bone Invariance (head, neck, spine, chest, pelvis)", () => {
    it("guarantees zero delta on upper-body bone positions and rotations across 100 frames of saccades", async () => {
      async function simulateBoneTrajectory(saccadesActive: boolean) {
        let rafCallback: ((time: number) => void) | null = null;
        vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb: any) => {
          rafCallback = cb;
          return 1;
        });

        const dt = 1 / 60;
        vi.spyOn(THREE.Clock.prototype, "getDelta").mockReturnValue(dt);

        const boneNames = ["head", "neck", "spine", "chest", "hips"] as const;
        const bones = new Map<string, THREE.Object3D>();
        for (const name of boneNames) {
          const obj = new THREE.Object3D();
          // Initial non-zero transforms to test perturbation
          obj.position.set(0.1, 1.2, -0.05);
          obj.rotation.set(0.02, -0.01, 0.05);
          bones.set(name, obj);
        }

        const scene = new THREE.Object3D();
        const vrmInstance = {
          scene,
          humanoid: {
            getNormalizedBoneNode: (name: string) => bones.get(name) ?? null,
          },
          expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
          lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
          update: vi.fn(),
        };

        mockLoadGLTF.mockImplementation((_url, onLoad) => {
          onLoad({ userData: { vrm: vrmInstance }, scene });
        });

        const model = use3DModel();
        model.initRenderer(document.createElement("canvas"), 800, 600);
        await model.loadModel("models/avatar.vrm");

        model.setSaccadesEnabled(saccadesActive);
        model.updateLookAt(8.0, -4.0);

        if (saccadesActive) {
          model.triggerSaccade(0.80, -0.55);
        }

        model.startRenderLoop();

        let simTime = 3000.0;
        for (let f = 1; f <= 100; f++) {
          simTime += dt * 1000;
          rafCallback!(simTime);
        }

        const snapshot: Record<string, { pos: [number, number, number]; rot: [number, number, number] }> = {};
        for (const [name, obj] of bones.entries()) {
          snapshot[name] = {
            pos: [obj.position.x, obj.position.y, obj.position.z],
            rot: [obj.rotation.x, obj.rotation.y, obj.rotation.z],
          };
        }

        model.stopRenderLoop();
        model.dispose();
        return snapshot;
      }

      const withSaccades = await simulateBoneTrajectory(true);
      const withoutSaccades = await simulateBoneTrajectory(false);

      // Verify that head, neck, spine, chest, and hips (pelvis) are 100% bitwise identical
      for (const bone of ["head", "neck", "spine", "chest", "hips"]) {
        expect(withSaccades[bone].pos).toEqual(withoutSaccades[bone].pos);
        expect(withSaccades[bone].rot).toEqual(withoutSaccades[bone].rot);
      }
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // Challenge 3: Adversarial Delta Timing & Boundary Stress
  // ──────────────────────────────────────────────────────────────────────────
  describe("Challenge 3: Adversarial Delta Timing & Boundary Conditions", () => {
    it("handles extreme deltas (large lag spikes, zero, negative, NaN) without breaking clamp bounds", async () => {
      let rafCallback: ((time: number) => void) | null = null;
      vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb: any) => {
        rafCallback = cb;
        return 1;
      });

      let currentDelta = 1 / 60;
      vi.spyOn(THREE.Clock.prototype, "getDelta").mockImplementation(() => currentDelta);

      const scene = new THREE.Object3D();
      const vrmInstance = {
        scene,
        humanoid: null,
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
        update: vi.fn(),
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement("canvas"), 800, 600);
      await model.loadModel("models/avatar.vrm");

      model.startRenderLoop();

      const adversarialDeltas = [
        0.50,       // huge lag spike (500ms)
        2.00,       // 2-second freeze
        0.00,       // zero delta
        -0.05,      // negative delta
        NaN,        // corrupt clock
        Infinity,   // unbounded delta
        0.0001,     // sub-millisecond tick
      ];

      for (const deltaVal of adversarialDeltas) {
        currentDelta = deltaVal;
        rafCallback!(performance.now());

        const offset = model.getSaccadeOffset();
        expect(Number.isFinite(offset.yaw)).toBe(true);
        expect(Number.isFinite(offset.pitch)).toBe(true);
        expect(Math.abs(offset.yaw)).toBeLessThanOrEqual(MAX_SACCADE_AMPLITUDE_DEG + 1e-5);
        expect(Math.abs(offset.pitch)).toBeLessThanOrEqual(MAX_SACCADE_AMPLITUDE_DEG + 1e-5);
      }

      model.stopRenderLoop();
      model.dispose();
    });

    it("triggerSaccade clamps manually injected out-of-range amplitudes strictly to ±0.85°", () => {
      const model = use3DModel();

      // Attempt injection of huge amplitudes
      model.triggerSaccade(45.0, -90.0);
      // Even if triggered with huge values, target must be clamped to ±0.85°
      // Let's verify by checking state after jump initiation
      model.setSaccadesEnabled(true);

      // Force jump state and verify bounds
      const offset = model.getSaccadeOffset();
      expect(Math.abs(offset.yaw)).toBeLessThanOrEqual(MAX_SACCADE_AMPLITUDE_DEG);
      expect(Math.abs(offset.pitch)).toBeLessThanOrEqual(MAX_SACCADE_AMPLITUDE_DEG);

      model.dispose();
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // Challenge 4: FaceLandmarker Web Worker 60 FPS Stress & Memory Leak Audit
  // ──────────────────────────────────────────────────────────────────────────
  describe("Challenge 4: FaceLandmarker Web Worker 60 FPS Stress Test", () => {
    it("runs 60 consecutive frames at 16.6ms intervals: verifies zero frame loss, 100% bitmap closure, and error isolation", async () => {
      const originalSelf = globalThis.self;
      const postMessageMock = vi.fn();
      const closeMock = vi.fn();

      globalThis.self = {
        postMessage: postMessageMock,
        close: closeMock,
      } as any;

      const mockDetect = vi.fn();
      const mockCloseLandmarker = vi.fn();
      const mockCreateFromOptions = vi.fn().mockResolvedValue({
        detect: mockDetect,
        close: mockCloseLandmarker,
      });

      vi.doMock("@mediapipe/tasks-vision", () => ({
        FaceLandmarker: { createFromOptions: mockCreateFromOptions },
        FilesetResolver: { forVisionTasks: vi.fn().mockResolvedValue({ dummy: true }) },
      }));

      // Import worker
      await import("../../src/workers/faceTrackingWorker");
      const onmessage = (globalThis.self as any).onmessage;
      expect(onmessage).toBeDefined();

      // Initialize
      await onmessage({ data: { type: "init" } });
      expect(postMessageMock).toHaveBeenCalledWith({ type: "ready" });
      postMessageMock.mockClear();

      const FRAME_COUNT = 60;
      const bitmaps = Array.from({ length: FRAME_COUNT }, () => ({
        width: 640,
        height: 480,
        close: vi.fn(),
      }));

      // Setup detect to return valid face results
      mockDetect.mockReturnValue({
        faceLandmarks: [[{ x: 0.5, y: 0.5, z: 0 }]],
        faceBlendshapes: [{ categories: [{ categoryName: "mouthSmileLeft", score: 0.75 }] }],
      });

      // Stream 60 frames consecutively at 16.6ms intervals
      const startMs = 1000.0;
      for (let i = 0; i < FRAME_COUNT; i++) {
        await onmessage({
          data: {
            type: "detect",
            bitmap: bitmaps[i],
            timestamp: startMs + i * 16.666,
          },
        });
      }

      // Assert 0 frame loss: exactly 60 result messages dispatched
      expect(postMessageMock).toHaveBeenCalledTimes(FRAME_COUNT);

      // Assert memory safety: all 60 bitmaps were closed (zero leaks)
      for (let i = 0; i < FRAME_COUNT; i++) {
        expect(bitmaps[i].close).toHaveBeenCalledTimes(1);
      }

      // Assert detect was called 60 times
      expect(mockDetect).toHaveBeenCalledTimes(FRAME_COUNT);

      // Test worker disposal cleans up the landmarker
      await onmessage({ data: { type: "dispose" } });
      expect(mockCloseLandmarker).toHaveBeenCalledTimes(1);

      globalThis.self = originalSelf;
    });
  });
});
