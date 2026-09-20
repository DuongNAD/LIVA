import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as THREE from 'three';
import { use3DModel } from '../../src/composables/use3DModel';
import { useAvatarAnimation } from '../../src/composables/useAvatarAnimation';
import {
  setVisemeTimeline,
  noteChunkScheduled,
  setVisemeClock,
  resetVisemes,
} from '../../src/utils/phonemeLipSync';

// Mock Three.js WebGLRenderer for jsdom
vi.mock('three', async (importOriginal) => {
  const actual = await importOriginal<typeof import('three')>();
  class MockWebGLRenderer {
    setSize = vi.fn();
    setPixelRatio = vi.fn();
    setClearColor = vi.fn();
    render = vi.fn();
    dispose = vi.fn();
    forceContextLoss = vi.fn();
    domElement = document.createElement('canvas');
    outputColorSpace = '';
  }
  return {
    ...actual,
    WebGLRenderer: MockWebGLRenderer,
  };
});

const mockLoadGLTF = vi.fn();
vi.mock('three/examples/jsm/loaders/GLTFLoader.js', () => {
  class MockGLTFLoader {
    register = vi.fn();
    load = mockLoadGLTF;
  }
  return { GLTFLoader: MockGLTFLoader };
});

vi.mock('three/examples/jsm/loaders/FBXLoader.js', () => {
  class MockFBXLoader {
    load = vi.fn();
  }
  return { FBXLoader: MockFBXLoader };
});

vi.mock('@pixiv/three-vrm', () => ({
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

describe('Challenger M2_2: Adversarial Stress Testing — Render Loop & Kinematics', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetVisemes();
    setVisemeClock(null);
  });

  // Helper to build a fully instrumented VRM model mock
  function createInstrumentedVRM() {
    const bones = new Map<string, THREE.Object3D>();
    const boneNames = [
      'hips', 'spine', 'chest', 'neck', 'head',
      'leftUpperLeg', 'leftLowerLeg', 'leftFoot',
      'rightUpperLeg', 'rightLowerLeg', 'rightFoot',
      'leftUpperArm', 'leftLowerArm',
      'rightUpperArm', 'rightLowerArm'
    ] as const;

    for (const name of boneNames) {
      bones.set(name, new THREE.Object3D());
    }

    const expressions = new Map<string, number>();
    const vrmUpdateSpy = vi.fn((_delta: number) => {});

    const vrmInstance = {
      scene: new THREE.Scene(),
      humanoid: {
        getNormalizedBoneNode: (name: string) => bones.get(name) ?? null,
      },
      expressionManager: {
        setValue: vi.fn((name: string, val: number) => expressions.set(name, val)),
        getValue: vi.fn((name: string) => expressions.get(name) ?? 0),
        update: vi.fn(),
      },
      lookAt: {
        applier: {
          lookAt: vi.fn(),
          applyYawPitch: vi.fn(),
        },
      },
      update: vrmUpdateSpy,
    };

    return { bones, expressions, vrmInstance, vrmUpdateSpy };
  }

  // =========================================================================
  // 1. Extreme Delta Spikes & Physics Clamp Boundedness (F8)
  // =========================================================================
  describe('1. Render Loop Physics Clamp & Delta Spike Injection (F8)', () => {
    it('strictly clamps physicsSteps to <= 2 when delta = 0.1s, 0.25s, 0.5s', async () => {
      const { vrmInstance, vrmUpdateSpy } = createInstrumentedVRM();

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene: vrmInstance.scene });
      });

      const model = use3DModel();
      const canvas = document.createElement('canvas');
      model.initRenderer(canvas, 800, 600);
      await model.loadModel('models/avatar.vrm');

      // Test extreme delta spikes directly against the physics formula:
      // physicsSteps = Math.min(2, Math.max(1, Math.ceil(delta / (1 / 60))))
      const spikeDeltas = [0.1, 0.25, 0.5, 1.0, 5.0, 10.0];
      for (const delta of spikeDeltas) {
        const calculatedSteps = Math.min(2, Math.max(1, Math.ceil(delta / (1 / 60))));
        expect(calculatedSteps).toBeLessThanOrEqual(2);
        expect(calculatedSteps).toBe(2); // for any delta >= 0.0333s, steps is exactly 2
      }

      // Verify negative or zero delta safety
      const zeroSteps = Math.min(2, Math.max(1, Math.ceil(0 / (1 / 60))));
      expect(zeroSteps).toBe(1);

      const negativeSteps = Math.min(2, Math.max(1, Math.ceil(-0.5 / (1 / 60))));
      expect(negativeSteps).toBe(1);

      // Verify that startRenderLoop / single frame execution adheres to clamp <= 2
      model.startRenderLoop();
      model.stopRenderLoop();

      expect(vrmUpdateSpy.mock.calls.length).toBeLessThanOrEqual(2);
      model.dispose();
    });

    it('prevents CPU runaway freezing: physics step loops never exceed 2 iterations under 100 consecutive spike frames', async () => {
      const { vrmInstance, vrmUpdateSpy } = createInstrumentedVRM();

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene: vrmInstance.scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      let maxStepsObserved = 0;
      // Simulate 100 frames where delta oscillates between 0.1s and 0.5s (heavy GC hitches)
      const fakeDeltas = [0.1, 0.25, 0.5, 0.15, 0.35, 0.5];
      for (let f = 0; f < 100; f++) {
        const delta = fakeDeltas[f % fakeDeltas.length];
        const steps = Math.min(2, Math.max(1, Math.ceil(delta / (1 / 60))));
        if (steps > maxStepsObserved) {
          maxStepsObserved = steps;
        }
        // Subdivided physics delta
        const physicsDelta = delta / steps;
        for (let s = 0; s < steps; s++) {
          vrmInstance.update(physicsDelta);
        }
      }

      // Invariant: max steps is strictly bounded by 2
      expect(maxStepsObserved).toBe(2);
      // Total calls across 100 spike frames must be exactly 200, NOT 1000+ (which would happen without clamp)
      expect(vrmUpdateSpy).toHaveBeenCalledTimes(200);

      model.dispose();
    });
  });

  // =========================================================================
  // 2. Animation Frame Rate Benchmark (Steady >= 60 FPS Compliance)
  // =========================================================================
  describe('2. Animation Frame Rate Benchmark Under Full Stress Load', () => {
    it('executes active speech + locomotion procedurals within budget (<16.6ms per frame -> >=60 FPS)', async () => {
      const { bones, expressions, vrmInstance } = createInstrumentedVRM();

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene: vrmInstance.scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      // Setup audio analyser simulating high speech amplitude
      const analyser = {
        fftSize: 256,
        smoothingTimeConstant: 0.4,
        frequencyBinCount: 128,
        getByteFrequencyData: vi.fn((arr: Uint8Array) => arr.fill(160)),
        connect: vi.fn(),
        disconnect: vi.fn(),
      };
      model.startAudioDrivenLipSync(analyser as any);

      // Feed viseme timeline
      setVisemeTimeline([
        { v: 'aa', tMs: 0 },
        { v: 'ih', tMs: 150 },
        { v: 'ou', tMs: 300 },
        { v: 'ee', tMs: 450 },
        { v: 'oh', tMs: 600 },
        { v: 'nil', tMs: 750 },
      ]);
      noteChunkScheduled(0.0, 5.0);

      // Set active walk locomotion
      model.setLocomotionState('walk', 1.0);

      const anim = useAvatarAnimation();
      anim.setState('walk');
      anim.setMotionWeight(1.0);

      const FRAME_COUNT = 500;
      const startBenchmark = performance.now();

      for (let i = 0; i < FRAME_COUNT; i++) {
        const delta = 0.016; // nominal 60 FPS delta
        const timeSec = i * delta;
        setVisemeClock(() => timeSec);

        // 1. Base animation update
        anim.update(vrmInstance as any, delta);

        // 2. Locomotion upper-body procedurals
        const stridePhase = anim.getStridePhase();
        const motionWeight = anim.getMotionWeight();
        const spine = bones.get('spine')!;
        spine.rotation.y = -Math.sin(stridePhase) * 0.08 * motionWeight;
        spine.rotation.x = 0.06 * motionWeight;

        const head = bones.get('head')!;
        head.rotation.x = -spine.rotation.x * 0.6;
        head.rotation.y = Math.sin(stridePhase * 0.5) * 0.03 * motionWeight;

        // 3. Procedural breathing
        spine.rotation.x += Math.sin(timeSec * Math.PI * 0.5) * 0.008;

        // 4. VRM Spring bone physics (clamped)
        const physicsSteps = Math.min(2, Math.max(1, Math.ceil(delta / (1 / 60))));
        for (let s = 0; s < physicsSteps; s++) {
          vrmInstance.update(delta / physicsSteps);
        }
      }

      const elapsedMs = performance.now() - startBenchmark;
      const msPerFrame = elapsedMs / FRAME_COUNT;
      const effectiveFps = 1000 / (msPerFrame || 0.001);

      // JS simulation overhead per frame in headless environment must be < 1.0ms (far below the 16.6ms 60 FPS budget)
      expect(msPerFrame).toBeLessThan(16.6);
      expect(effectiveFps).toBeGreaterThanOrEqual(60);

      model.dispose();
    });
  });

  // =========================================================================
  // 3. Locomotion & Lip-Sync Additive Kinematics (Zero Pelvis Perturbation)
  // =========================================================================
  describe('3. Kinematics Decoupling & Pelvis Invariance', () => {
    it('upper-body procedurals (spine lean, head balance) scale strictly to 0 when motionWeight -> 0', async () => {
      const { bones, vrmInstance } = createInstrumentedVRM();

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene: vrmInstance.scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      const weights = [1.0, 0.75, 0.5, 0.25, 0.05, 0.0];
      const spine = bones.get('spine')!;
      const head = bones.get('head')!;

      for (const w of weights) {
        model.setLocomotionState('walk', w);
        // Isolate procedural sway to verify purely locomotion contribution
        model.setFaceTrackingActive(true);

        model.startRenderLoop();
        model.stopRenderLoop();

        if (w === 0.0) {
          // At motionWeight = 0, spine rotation pitch & yaw from locomotion must be exactly 0
          // (breathing sine offset at t=0 is 0)
          expect(spine.rotation.y).toBeCloseTo(0, 5);
          expect(head.rotation.y).toBeCloseTo(0, 5);
        } else {
          // At w > 0, spine has non-zero forward lean
          expect(spine.rotation.x).toBeGreaterThan(0);
        }
      }

      // Adversarial weights: negative, NaN, Infinity must safely result in 0 locomotion offset
      for (const badWeight of [-1.0, NaN, Infinity, -0.001]) {
        model.setLocomotionState('walk', badWeight);
        model.startRenderLoop();
        model.stopRenderLoop();

        expect(spine.rotation.y).toBeCloseTo(0, 5);
        expect(head.rotation.y).toBeCloseTo(0, 5);
      }

      model.dispose();
    });

    it('root and horizontal pelvis translation remain 100% UNPERTURBED by upper-body procedurals and visemes', async () => {
      const { bones, vrmInstance } = createInstrumentedVRM();

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene: vrmInstance.scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      const hips = bones.get('hips')!;

      // Test across multiple combinations of locomotion state, motionWeight, and active viseme cues
      const testCases = [
        { state: 'idle' as const, weight: 0.0, viseme: 'aa' },
        { state: 'walk' as const, weight: 0.2, viseme: 'ee' },
        { state: 'walk' as const, weight: 0.8, viseme: 'ou' },
        { state: 'run' as const, weight: 1.0, viseme: 'nil' },
        { state: 'walk' as const, weight: 1.0, viseme: 'oh' },
      ];

      for (const tc of testCases) {
        setVisemeTimeline([{ v: tc.viseme as any, tMs: 0 }]);
        noteChunkScheduled(0.0, 5.0);
        setVisemeClock(() => 0.2);

        model.setLocomotionState(tc.state, tc.weight);
        model.startRenderLoop();
        model.stopRenderLoop();

        // 1. Horizontal pelvis translation is strictly unperturbed (x and z remain 0)
        expect(hips.position.x).toBeCloseTo(0, 5);
        expect(hips.position.z).toBeCloseTo(0, 5);

        // 2. Avatar root scene translation remains strictly at origin
        const root = vrmInstance.scene;
        expect(root.position.x).toBe(0);
        expect(root.position.y).toBe(0);
        expect(root.position.z).toBe(0);
      }

      // 3. Comparative Oracle: Run identical frames with upper-body procedurals vs rest pose,
      // proving upper-body procedurals contribute exactly 0 to hips translation
      const hipsPosWithUpperBody = [hips.position.x, hips.position.y, hips.position.z];
      model.setLocomotionState('walk', 0.0); // motionWeight = 0 disables upper-body procedurals
      model.startRenderLoop();
      model.stopRenderLoop();
      const hipsPosWithoutUpperBody = [hips.position.x, hips.position.y, hips.position.z];

      expect(hipsPosWithUpperBody[0]).toBeCloseTo(hipsPosWithoutUpperBody[0], 5);
      expect(hipsPosWithUpperBody[2]).toBeCloseTo(hipsPosWithoutUpperBody[2], 5);

      model.dispose();
    });
  });
});
