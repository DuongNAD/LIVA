import { describe, it, expect, vi, beforeEach } from "vitest";
import { use3DModel } from "../../src/composables/use3DModel";
import * as THREE from "three";

// Mock THREE.js partially
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

// Mock GLTFLoader
const mockLoadGLTF = vi.fn();
vi.mock("three/examples/jsm/loaders/GLTFLoader.js", () => {
  class MockGLTFLoader {
    register = vi.fn();
    load = mockLoadGLTF;
  }
  return {
    GLTFLoader: MockGLTFLoader,
  };
});

// Mock FBXLoader
const mockLoadFBX = vi.fn();
vi.mock("three/examples/jsm/loaders/FBXLoader.js", () => {
  class MockFBXLoader {
    load = mockLoadFBX;
  }
  return {
    FBXLoader: MockFBXLoader,
  };
});

// Mock Pixiv Three VRM
const mockVRM = {
  scene: { traverse: vi.fn() },
  update: vi.fn(),
  expressionManager: {
    setValue: vi.fn(),
    getValue: vi.fn(),
    update: vi.fn(),
  },
  lookAt: {
    applier: {
      lookAt: vi.fn(),
      applyYawPitch: vi.fn(),
    },
  },
};
vi.mock("@pixiv/three-vrm", () => ({
  VRMLoaderPlugin: vi.fn(),
  VRM: vi.fn(),
  VRMUtils: {
    removeUnnecessaryVertices: vi.fn(),
    removeUnnecessaryJoints: vi.fn(),
    rotateVRM0: vi.fn(),
  },
}));

describe("use3DModel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should initialize default state", () => {
    const model = use3DModel();
    expect(model.vrm.value).toBeNull();
    expect(model.currentModelFormat.value).toBeNull();
  });

  it("should allow calling start/stop methods", () => {
    const model = use3DModel();
    model.startAutoBlink();
    model.startLipSync();
    model.stopLipSync();

    const mockAudioCtx = {
      createAnalyser: () => ({
        fftSize: 0,
        smoothingTimeConstant: 0,
        frequencyBinCount: 128,
        getByteFrequencyData: vi.fn(),
        connect: vi.fn(),
        disconnect: vi.fn(),
      }),
      destination: {},
    };
    const mockSource = {
      connect: vi.fn(),
      disconnect: vi.fn(),
    };

    model.startAudioDrivenLipSync(mockAudioCtx as any, mockSource as any);
    model.stopAudioDrivenLipSync();
    model.triggerMotion("wave");
  });

  it("should allow calling updateLookAt and updateExpressions", () => {
    const model = use3DModel();
    model.updateLookAt(10, 20);
    model.updateExpressions({ eyeBlinkLeft: 1.0 });
  });

  it("should attempt loading VRM and handle success", async () => {
    mockLoadGLTF.mockImplementation((url, onLoad, onProgress, onError) => {
      const scene = new THREE.Object3D();
      const mockVRMInstance = {
        scene,
        update: vi.fn(),
        expressionManager: {
          setValue: vi.fn(),
          getValue: vi.fn(),
          update: vi.fn(),
        },
        lookAt: {
          applier: {
            lookAt: vi.fn(),
            applyYawPitch: vi.fn(),
          },
        },
      };
      const mockGltf = {
        userData: { vrm: mockVRMInstance },
        scene,
      };
      onLoad(mockGltf);
    });

    const model = use3DModel();
    const canvas = document.createElement("canvas");
    model.initRenderer(canvas);
    await model.loadModel("models/avatar.vrm");

    expect(model.currentModelFormat.value).toBe("vrm");

    model.updateLookAt(10, 20);
    model.updateExpressions({
      happy: 0.5, sad: 0, surprised: 0, angry: 0,
      blink: 0.2, blinkLeft: 0.2, blinkRight: 0.2,
      mouthOpen: 0.3, browUpLeft: 0, browUpRight: 0,
    });
    model.startAutoBlink();
    model.startLipSync();
    model.triggerMotion();
    model.setFaceTrackingActive(true);
    model.startRenderLoop();
    model.stopRenderLoop();
    model.dispose();
  });

  it("should attempt loading FBX and handle success", async () => {
    mockLoadFBX.mockImplementation((url, onLoad, onProgress, onError) => {
      const fbx = new THREE.Object3D() as any;
      fbx.animations = [new THREE.AnimationClip("idle", -1, [])];
      fbx.traverse = vi.fn();
      onLoad(fbx);
    });

    const model = use3DModel();
    const canvas = document.createElement("canvas");
    model.initRenderer(canvas);
    await model.loadModel("models/avatar.fbx");

    expect(model.currentModelFormat.value).toBe("fbx");
    model.startRenderLoop();
    model.stopRenderLoop();
    model.dispose();
  });

  it("should handle loader errors", async () => {
    mockLoadGLTF.mockImplementation((url, onLoad, onProgress, onError) => {
      onError(new Error("Failed to load"));
    });

    const model = use3DModel();
    const canvas = document.createElement("canvas");
    model.initRenderer(canvas);
    await expect(model.loadModel("models/avatar.vrm")).rejects.toThrow("Failed to load");
  });

  it("should dispose model and clean up", async () => {
    const model = use3DModel();
    model.dispose();
  });
});
