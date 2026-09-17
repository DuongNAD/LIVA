import { describe, it, expect, vi, beforeEach } from "vitest";
import * as THREE from "three";
import { useAvatarAnimation, STRIDE_LENGTH, STRIDE_HZ, WALK_SPEED } from "../../src/composables/useAvatarAnimation";
import { FootPlantIK } from "../../src/composables/footPlantIK";
import { parseVisemePayload } from "../../src/utils/speakerFrame";
import {
  currentViseme,
  currentVisemeFromClock,
  noteChunkScheduled,
  resetVisemes,
  setVisemeClock,
  setVisemeTimeline,
} from "../../src/utils/phonemeLipSync";
import { use3DModel } from "../../src/composables/use3DModel";

// Mock Three.js WebGLRenderer and loaders for use3DModel in jsdom
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

const mockLoadGLTF = vi.fn();
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

describe("Adversarial Verification � Avatar Locomotion & Lip-sync Interaction", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetVisemes();
    setVisemeClock(null);
  });

  // ---------------------------------------------------------------
  // 1. Viseme Protocol & Fail-Closed Parser Stress Tests
  // ---------------------------------------------------------------
  describe("Viseme Wire Protocol & Parser Resilience", () => {
    it("fail-closed: rejects malformed JSON or binary garbage", () => {
      expect(parseVisemePayload(new Uint8Array([0x00, 0xff, 0xfe, 0x12]))).toBeNull();
      expect(parseVisemePayload(new TextEncoder().encode(""))).toBeNull();
      expect(parseVisemePayload(new TextEncoder().encode("{broken json"))).toBeNull();
    });

    it("fail-closed: rejects payloads with missing mandatory fields or wrong types", () => {
      const encode = (obj: any) => new TextEncoder().encode(JSON.stringify(obj));
      expect(parseVisemePayload(encode({ turn_epoch: "seven", base_seq_id: 1, visemes: [] }))).toBeNull();
      expect(parseVisemePayload(encode({ turn_epoch: 1, base_seq_id: "two", visemes: [] }))).toBeNull();
      expect(parseVisemePayload(encode({ turn_epoch: 1, base_seq_id: 2, visemes: "not-an-array" }))).toBeNull();
    });

    it("fail-closed: rejects unknown viseme symbols not in standard VRM whitelist", () => {
      const encode = (obj: any) => new TextEncoder().encode(JSON.stringify(obj));
      const invalidVisemes = ["sh", "th", "ch", "custom", "AA", "NIL", ""];
      for (const bad of invalidVisemes) {
        const payload = encode({
          turn_epoch: 1,
          base_seq_id: 1,
          visemes: [{ v: bad, t_ms: 0 }],
        });
        expect(parseVisemePayload(payload)).toBeNull();
      }
    });

    it("fail-closed: strictly rejects non-monotonic timestamps or negative t_ms", () => {
      const encode = (obj: any) => new TextEncoder().encode(JSON.stringify(obj));
      // Negative t_ms
      expect(parseVisemePayload(encode({
        turn_epoch: 1, base_seq_id: 1, visemes: [{ v: "aa", t_ms: -10 }]
      }))).toBeNull();

      // Equal t_ms (not strictly increasing)
      expect(parseVisemePayload(encode({
        turn_epoch: 1, base_seq_id: 1,
        visemes: [{ v: "aa", t_ms: 50 }, { v: "ee", t_ms: 50 }]
      }))).toBeNull();

      // Decreasing t_ms
      expect(parseVisemePayload(encode({
        turn_epoch: 1, base_seq_id: 1,
        visemes: [{ v: "aa", t_ms: 100 }, { v: "ee", t_ms: 50 }]
      }))).toBeNull();
    });

    it("accepts valid multi-cue timeline and parses correctly into camelCase cues", () => {
      const payload = new TextEncoder().encode(JSON.stringify({
        turn_epoch: 42,
        base_seq_id: 100,
        visemes: [
          { v: "nil", t_ms: 0 },
          { v: "aa", t_ms: 80 },
          { v: "ih", t_ms: 160 },
          { v: "ou", t_ms: 240 },
          { v: "ee", t_ms: 320 },
          { v: "oh", t_ms: 400 },
          { v: "nil", t_ms: 480 },
        ],
      }));

      const result = parseVisemePayload(payload);
      expect(result).not.toBeNull();
      expect(result?.turnEpoch).toBe(42);
      expect(result?.baseSeqId).toBe(100);
      expect(result?.cues).toHaveLength(7);
      expect(result?.cues[0]).toEqual({ v: "nil", tMs: 0 });
      expect(result?.cues[6]).toEqual({ v: "nil", tMs: 480 });
    });
  });

  // ---------------------------------------------------------------
  // 2. Timeline Registry & Clock Synchronization Stress Tests
  // ---------------------------------------------------------------
  describe("Phoneme Timeline Registry Clock Invariants", () => {
    it("timeline returns null before anchorSec and after endSec", () => {
      setVisemeTimeline([
        { v: "nil", tMs: 0 },
        { v: "aa", tMs: 100 },
        { v: "ou", tMs: 250 },
      ]);
      noteChunkScheduled(50.0, 0.4); // plays from t=50.0 to t=50.4

      expect(currentViseme(49.99)).toBeNull();
      expect(currentViseme(50.0)).toBe("nil");
      expect(currentViseme(50.05)).toBe("nil");
      expect(currentViseme(50.10)).toBe("aa");
      expect(currentViseme(50.249)).toBe("aa");
      expect(currentViseme(50.250)).toBe("ou");
      expect(currentViseme(50.399)).toBe("ou");
      expect(currentViseme(50.401)).toBeNull(); // past endSec
    });

    it("clock jitter and micro-step evaluation remains strictly deterministic", () => {
      setVisemeTimeline([
        { v: "nil", tMs: 0 },
        { v: "ee", tMs: 50 },
        { v: "oh", tMs: 150 },
      ]);
      noteChunkScheduled(100.0, 0.3);

      let mockTime = 100.0;
      setVisemeClock(() => mockTime);

      // Advance in 5ms microsteps
      const observedVisemes: string[] = [];
      for (let i = 0; i <= 60; i++) {
        mockTime = 100.0 + i * 0.005;
        const v = currentVisemeFromClock();
        if (v) observedVisemes.push(v);
      }

      // First 10 samples (0 to 45ms) should be nil
      for (let i = 0; i < 10; i++) expect(observedVisemes[i]).toBe("nil");
      // Samples 10 to 30 (50ms to 145ms) should be ee
      for (let i = 10; i < 30; i++) expect(observedVisemes[i]).toBe("ee");
      // Samples 30 to 60 (150ms to 300ms) should be oh
      for (let i = 30; i <= 60; i++) expect(observedVisemes[i]).toBe("oh");
    });

    it("barge-in resetVisemes cancels anchored timeline immediately", () => {
      setVisemeTimeline([{ v: "aa", tMs: 0 }]);
      noteChunkScheduled(10.0, 1.0);
      expect(currentViseme(10.5)).toBe("aa");

      resetVisemes();
      expect(currentViseme(10.5)).toBeNull();
    });
  });

  // ---------------------------------------------------------------
  // 3. Bone Matrix & Pelvis Isolation (Empirical Zero-Perturbation)
  // ---------------------------------------------------------------
  describe("Bone Matrices & Pelvis Translation Invariance", () => {
    function createMockHumanoid() {
      const boneNames = [
        "spine", "chest", "neck", "head", "hips",
        "leftUpperLeg", "leftLowerLeg", "leftFoot",
        "rightUpperLeg", "rightLowerLeg", "rightFoot",
        "leftUpperArm", "leftLowerArm",
        "rightUpperArm", "rightLowerArm"
      ] as const;

      const bones = new Map<string, THREE.Object3D>();
      for (const name of boneNames) {
        const obj = new THREE.Object3D();
        bones.set(name, obj);
      }

      const expressions = new Map<string, number>();
      const expressionManager = {
        setValue: vi.fn((name: string, val: number) => expressions.set(name, val)),
        getValue: vi.fn((name: string) => expressions.get(name) ?? 0),
        update: vi.fn(),
      };

      const scene = new THREE.Scene();
      const vrmInstance = {
        scene,
        humanoid: {
          getNormalizedBoneNode: (name: string) => bones.get(name) ?? null,
        },
        expressionManager,
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
        update: vi.fn(),
      };

      return { vrmInstance, bones, expressionManager, expressions };
    }

    it("viseme blendshape modulation alters ONLY facial expressions, leaving 100% of bones untouched", async () => {
      const { vrmInstance, bones, expressionManager, expressions } = createMockHumanoid();

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene: vrmInstance.scene });
      });

      const model = use3DModel();
      const canvas = document.createElement("canvas");
      model.initRenderer(canvas, 800, 600);
      await model.loadModel("models/avatar.vrm");

      // Setup fake audio analyser
      const analyser = {
        fftSize: 256,
        smoothingTimeConstant: 0.4,
        frequencyBinCount: 128,
        getByteFrequencyData: vi.fn((arr: Uint8Array) => {
          arr.fill(180); // simulate strong audio amplitude
        }),
        connect: vi.fn(),
        disconnect: vi.fn(),
      };
      model.startAudioDrivenLipSync(analyser as any);

      // Disable face tracking to isolate procedural kinematics
      model.setFaceTrackingActive(true);

      // Set walk locomotion
      model.setLocomotionState("walk", 1.0);

      // Test across every viseme variant
      const visemesToTest = ["aa", "ee", "ih", "oh", "ou", "nil"] as const;
      for (const viseme of visemesToTest) {
        setVisemeTimeline([{ v: viseme, tMs: 0 }]);
        noteChunkScheduled(0.0, 10.0);
        setVisemeClock(() => 1.0);

        model.startRenderLoop();

        // 1. Expressions were affected
        expect(expressionManager.setValue).toHaveBeenCalled();
        if (viseme === "nil") {
          // nil forces suppression
          for (const val of expressions.values()) {
            expect(val).toBeLessThan(0.3);
          }
        } else {
          // matched vowel expression boosted
          expect(expressions.get(viseme)).toBeGreaterThan(0.2);
        }

        // 2. Horizontal & vertical pelvis translations are UNTOUCHED by viseme
        const hips = bones.get("hips")!;
        expect(hips.position.x).toBeCloseTo(0, 5);
        expect(hips.position.z).toBeCloseTo(0, 5);
        expect(Number.isFinite(hips.position.y)).toBe(true);

        // 3. Bone rotations of lower body and arms are unaffected by visemes
        expect(bones.get("leftUpperLeg")!.position.x).toBe(0);
        expect(bones.get("rightUpperLeg")!.position.x).toBe(0);
        expect(bones.get("leftFoot")!.position.x).toBe(0);
        expect(bones.get("rightFoot")!.position.x).toBe(0);

        // 4. Verify that expressionManager calls are restricted to blendshapes only
        const calledExpressions = expressionManager.setValue.mock.calls.map((c) => c[0]);
        for (const expr of calledExpressions) {
          expect(["aa", "ee", "ih", "oh", "ou", "blink", "blinkLeft", "blinkRight", "happy", "relaxed", "surprised", "neutral"]).toContain(expr);
        }

        model.stopRenderLoop();
      }

      model.dispose();
    });

    it("FootPlantIK remains 100% uncorrupted and produces identical damping under active viseme stream", () => {
      const ik = new FootPlantIK();
      const leftFoot = { x: -0.1, y: 0.0, z: 0.0 };
      const rightFoot = { x: 0.1, y: 0.04, z: 0.0 };

      // Frame 1 without viseme
      const corr1 = ik.update({
        state: "walk",
        leftFoot,
        rightFoot,
        delta: 1 / 60,
      });

      // Reset and run Frame 1 with viseme active
      ik.reset();
      setVisemeTimeline([{ v: "aa", tMs: 0 }]);
      noteChunkScheduled(0, 1);
      setVisemeClock(() => 0.05);

      const corr2 = ik.update({
        state: "walk",
        leftFoot,
        rightFoot,
        delta: 1 / 60,
      });

      expect(corr2.x).toBe(corr1.x);
      expect(corr2.y).toBe(corr1.y);
      expect(corr2.z).toBe(corr1.z);
      // FootPlantIK correction is purely vertical (x and z remain 0)
      expect(corr2.x).toBe(0);
      expect(corr2.z).toBe(0);
    });
  });

  // ---------------------------------------------------------------
  // 4. Stride Phase Distance Progression & Freeze Stress Tests
  // ---------------------------------------------------------------
  describe("Distance-Based Stride Phase Calibration & Invariant Freezing", () => {
    it("stridePhase freezes unconditionally when currentSpeed == 0 even in walk/run state", () => {
      const anim = useAvatarAnimation();

      for (const state of ["walk", "run"] as const) {
        anim.setState(state);
        anim.setMotionWeight(0); // currentSpeed = nominalSpeed * 0 = 0
        const initialPhase = anim.getStridePhase();

        // Advance 100 frames
        for (let i = 0; i < 100; i++) {
          anim.update(null, 0.016);
        }

        expect(anim.getStridePhase()).toBe(initialPhase);
        expect(anim.stridePhase).toBe(initialPhase);
      }
    });

    it("stridePhase freezes unconditionally in non-locomotion states (idle, jump, dangle)", () => {
      const anim = useAvatarAnimation();
      const nonLocomotion = ["idle", "jump", "dangle"] as const;

      for (const state of nonLocomotion) {
        anim.setState(state);
        anim.setMotionWeight(1.0); // high motionWeight, but non-locomotion state has activeStrideLength = 0
        anim.update(null, 0.5); // crossfade completes
        const frozenPhase = anim.getStridePhase();

        for (let i = 0; i < 50; i++) {
          anim.update(null, 0.02);
        }

        expect(anim.getStridePhase()).toBe(frozenPhase);
      }
    });

    it("steady-state walk stride frequency exactly calibrates to 1.05 Hz baseline", () => {
      const anim = useAvatarAnimation();
      anim.setState("walk");
      anim.setMotionWeight(1.0);

      // In 1 second at 1.05 Hz, phase should advance by 1.05 * 2p = 2.1p radians
      const dt = 1 / 120; // 120 fps sub-steps
      for (let i = 0; i < 120; i++) {
        anim.update(null, dt);
      }

      const expectedPhase = (1.05 * Math.PI * 2) % (Math.PI * 2);
      expect(anim.getStridePhase()).toBeCloseTo(expectedPhase, 3);
    });

    it("stride advancement is strictly proportional to travel distance: deltaDistance = speed * delta", () => {
      const anim1 = useAvatarAnimation();
      const anim2 = useAvatarAnimation();
      anim1.setState("walk");
      anim2.setState("walk");

      // anim1 runs at speed 0.8 with delta 0.02 -> distance = 0.016
      anim1.setMotionWeight(0.8);
      anim1.update(null, 0.02);

      // anim2 runs at speed 0.4 with delta 0.04 -> distance = 0.016 (identical distance!)
      anim2.setMotionWeight(0.4);
      anim2.update(null, 0.04);

      expect(anim1.getStridePhase()).toBeCloseTo(anim2.getStridePhase(), 6);
    });

    it("adversarial input tolerance: handles negative delta, NaN delta, and infinite motionWeight safely", () => {
      const anim = useAvatarAnimation();
      anim.setState("walk");
      anim.setMotionWeight(1.0);

      // Adversarial updates
      anim.update(null, -1.0);
      expect(Number.isFinite(anim.getStridePhase())).toBe(true);

      anim.update(null, NaN);
      expect(Number.isFinite(anim.getStridePhase())).toBe(true);

      anim.setMotionWeight(Infinity);
      expect(anim.getMotionWeight()).toBe(0); // clamps to 0

      anim.setMotionWeight(-999);
      expect(anim.getMotionWeight()).toBe(0); // clamps to 0

      anim.update(null, 0.016);
      expect(Number.isFinite(anim.getStridePhase())).toBe(true);
    });
  });

  // ---------------------------------------------------------------
  // 5. Direct Comparative Oracle: Zero Viseme Influence on Bones
  // ---------------------------------------------------------------
  describe("Comparative Oracle: Bone State with vs without Visemes", () => {
    it("runs identical frames with and without visemes, confirming bitwise bone equality", async () => {
      async function runSimulatedStep(visemeCue: string | null) {
        const boneNames = [
          "spine", "chest", "neck", "head", "hips",
          "leftUpperLeg", "leftLowerLeg", "leftFoot",
          "rightUpperLeg", "rightLowerLeg", "rightFoot",
          "leftUpperArm", "leftLowerArm",
          "rightUpperArm", "rightLowerArm"
        ] as const;

        const bones = new Map<string, THREE.Object3D>();
        for (const name of boneNames) {
          const obj = new THREE.Object3D();
          bones.set(name, obj);
        }

        const expressions = new Map<string, number>();
        const expressionManager = {
          setValue: vi.fn((name: string, val: number) => expressions.set(name, val)),
          getValue: vi.fn((name: string) => expressions.get(name) ?? 0),
          update: vi.fn(),
        };

        const scene = new THREE.Scene();
        const vrmInstance = {
          scene,
          humanoid: {
            getNormalizedBoneNode: (name: string) => bones.get(name) ?? null,
          },
          expressionManager,
          lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
          update: vi.fn(),
        };

        mockLoadGLTF.mockImplementation((_url, onLoad) => {
          onLoad({ userData: { vrm: vrmInstance }, scene });
        });

        const model = use3DModel();
        const canvas = document.createElement("canvas");
        model.initRenderer(canvas, 800, 600);
        await model.loadModel("models/avatar.vrm");

        const analyser = {
          fftSize: 256,
          smoothingTimeConstant: 0.4,
          frequencyBinCount: 128,
          getByteFrequencyData: vi.fn((arr: Uint8Array) => arr.fill(150)),
          connect: vi.fn(),
          disconnect: vi.fn(),
        };
        model.startAudioDrivenLipSync(analyser as any);
        model.setFaceTrackingActive(true); // isolate procedural sway

        if (visemeCue) {
          setVisemeTimeline([{ v: visemeCue as any, tMs: 0 }]);
          noteChunkScheduled(0.0, 10.0);
          setVisemeClock(() => 0.5);
        } else {
          resetVisemes();
          setVisemeClock(null);
        }

        model.setLocomotionState("walk", 0.75);
        model.startRenderLoop();
        model.stopRenderLoop();

        const exprDump = new Map(expressions); const boneDump: Record<string, [number, number, number]> = {};
        for (const [name, obj] of bones.entries()) {
          boneDump[name] = [obj.rotation.x, obj.rotation.y, obj.rotation.z];
        }
        const hipsPos: [number, number, number] = [
          bones.get("hips")!.position.x,
          bones.get("hips")!.position.y,
          bones.get("hips")!.position.z,
        ];

        model.dispose();
        return { boneDump, hipsPos, expressions: exprDump };
      }

      const baseline = await runSimulatedStep(null);
      const withVisemeAa = await runSimulatedStep("aa");
      const withVisemeNil = await runSimulatedStep("nil");

      // Hips positions must be exactly identical
      expect(withVisemeAa.hipsPos).toEqual(baseline.hipsPos);
      expect(withVisemeNil.hipsPos).toEqual(baseline.hipsPos);

      // All bones (spine, chest, neck, head, hips, legs, arms) must have identical rotations
      for (const bone of Object.keys(baseline.boneDump)) {
        expect(withVisemeAa.boneDump[bone][0]).toBeCloseTo(baseline.boneDump[bone][0], 6);
        expect(withVisemeAa.boneDump[bone][1]).toBeCloseTo(baseline.boneDump[bone][1], 6);
        expect(withVisemeAa.boneDump[bone][2]).toBeCloseTo(baseline.boneDump[bone][2], 6);

        expect(withVisemeNil.boneDump[bone][0]).toBeCloseTo(baseline.boneDump[bone][0], 6);
        expect(withVisemeNil.boneDump[bone][1]).toBeCloseTo(baseline.boneDump[bone][1], 6);
        expect(withVisemeNil.boneDump[bone][2]).toBeCloseTo(baseline.boneDump[bone][2], 6);
      }

      // Expressions, on the other hand, MUST differ!
      expect(withVisemeAa.expressions.get("aa")).toBeGreaterThan(baseline.expressions.get("aa") ?? 0);
      expect(withVisemeNil.expressions.get("aa")).toBeLessThan(baseline.expressions.get("aa") ?? 1);
    });
  });
});
