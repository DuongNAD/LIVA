import { describe, it, expect, vi, beforeEach } from 'vitest';
import { use3DModel } from '../../src/composables/use3DModel';
import * as THREE from 'three';

const {
  loadMixamoAnimationSetMock,
  animationSetStateMock,
  animationSetMotionWeightMock,
  animationSetThinkingMock,
  animationUpdateMock,
  animationGetStateMock,
  animationGetMotionWeightMock,
  animationGetStridePhaseMock,
  animationGetThinkingWeightMock,
  animationClips,
  rendererInstances,
} = vi.hoisted(() => ({
  loadMixamoAnimationSetMock: vi.fn(),
  animationSetStateMock: vi.fn(),
  animationSetMotionWeightMock: vi.fn(),
  animationSetThinkingMock: vi.fn(),
  animationUpdateMock: vi.fn(),
  animationGetStateMock: vi.fn(() => 'idle'),
  animationGetMotionWeightMock: vi.fn(() => 0),
  animationGetStridePhaseMock: vi.fn(() => 0),
  animationGetThinkingWeightMock: vi.fn(() => 0),
  animationClips: new Set<string>(),
  rendererInstances: [] as Array<{ setPixelRatio: ReturnType<typeof vi.fn> }>,
}));

vi.mock('../../src/composables/mixamoClipLoader', () => ({
  DEFAULT_MIXAMO_CLIP_PATHS: {},
  loadMixamoAnimationSet: loadMixamoAnimationSetMock,
}));

vi.mock('../../src/composables/useAvatarAnimation', () => ({
  useAvatarAnimation: () => ({
    setState: (st: string) => {
      animationSetStateMock(st);
      animationGetStateMock.mockReturnValue(st as any);
    },
    setMotionWeight: (w: number) => {
      animationSetMotionWeightMock(w);
      animationGetMotionWeightMock.mockReturnValue(w);
    },
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

// Mock THREE.js partially
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

    constructor() {
      rendererInstances.push(this);
    }
  }

  return {
    ...actual,
    WebGLRenderer: MockWebGLRenderer,
  };
});

// Mock GLTFLoader
const mockLoadGLTF = vi.fn();
vi.mock('three/examples/jsm/loaders/GLTFLoader.js', () => {
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
vi.mock('three/examples/jsm/loaders/FBXLoader.js', () => {
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
vi.mock('@pixiv/three-vrm', () => ({
  VRMLoaderPlugin: vi.fn(),
  VRM: vi.fn(),
  VRMUtils: {
    removeUnnecessaryVertices: vi.fn(),
    // Giữ trong mock dù mã nguồn KHÔNG còn gọi — có test khẳng định đúng
    // điều đó. Bỏ khỏi mock thì test kia mất ý nghĩa: nó sẽ xanh vì hàm
    // không tồn tại, chứ không phải vì ta đã thôi gọi.
    removeUnnecessaryJoints: vi.fn(),
    combineSkeletons: vi.fn(),
    combineMorphs: vi.fn(),
    rotateVRM0: vi.fn(),
  },
}));

describe('use3DModel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    animationClips.clear();
    rendererInstances.length = 0;
  });

  /** AnalyserNode giả — do useSpeakerPlayback sở hữu, engine chỉ đọc. */
  function makeAnalyser() {
    return {
      fftSize: 2048,
      smoothingTimeConstant: 0.8,
      frequencyBinCount: 128,
      getByteFrequencyData: vi.fn(),
      connect: vi.fn(),
      disconnect: vi.fn(),
    };
  }

  it('sàn ECO Mode là 30 FPS, không phải 5', async () => {
    // U31(d): ECO từng đặt 200 ms (5 FPS) trên một nhân vật NGƯỜI DÙNG ĐANG
    // NHÌN. ECO tồn tại để sống chung với workload nặng — tức đúng lúc người
    // dùng vẫn đang nhìn — nên hạ frame rate là hạ đúng thứ họ nhận ra ngay.
    const { ECO_FRAME_INTERVAL_MS } = await import('../../src/composables/use3DModel');
    expect(ECO_FRAME_INTERVAL_MS).toBeLessThanOrEqual(34); // >= ~30 FPS
    expect(ECO_FRAME_INTERVAL_MS).toBeGreaterThan(0);
  });

  it('should initialize default state', () => {
    const model = use3DModel();
    expect(model.vrm.value).toBeNull();
    expect(model.currentModelFormat.value).toBeNull();
  });

  it('should allow calling start/stop methods', () => {
    const model = use3DModel();
    model.startAutoBlink();
    model.startLipSync();
    model.stopLipSync();

    model.startAudioDrivenLipSync(makeAnalyser() as any);
    model.stopAudioDrivenLipSync();
    model.triggerMotion('wave');
  });

  // ── U24: engine ĐỌC analyser, không được sờ vào đồ thị âm thanh ───────────
  // Bản cũ tự tạo analyser rồi nối `source → analyser → destination` song song
  // với đường phát, và tháo nó ra ở mỗi chunk. Nay analyser thuộc về
  // useSpeakerPlayback và nằm TRONG chuỗi ra, nên mọi thao tác nối/tháo từ
  // phía engine đều là lỗi: nối thêm là nhân đôi tiếng, tháo là đứt tiếng.
  it('khong noi hay thao analyser — chi doc du lieu tan so', () => {
    const model = use3DModel();
    const analyser = makeAnalyser();

    model.startAudioDrivenLipSync(analyser as any);

    expect(analyser.connect).not.toHaveBeenCalled();
    expect(analyser.disconnect).not.toHaveBeenCalled();
    // fftSize phải khớp BAND_RANGES (bin 0..64 trong 128 bin) — hai thứ này đi cùng nhau.
    expect(analyser.fftSize).toBe(256);

    model.stopAudioDrivenLipSync();

    expect(analyser.connect).not.toHaveBeenCalled();
    expect(analyser.disconnect).not.toHaveBeenCalled();
  });

  it('bam lai cung mot analyser la khong-lam-gi, khong reset giua luot noi', () => {
    const model = use3DModel();
    const analyser = makeAnalyser();

    model.startAudioDrivenLipSync(analyser as any);
    analyser.fftSize = 999; // dấu vết: nếu bị dựng lại, giá trị này bị ghi đè
    model.startAudioDrivenLipSync(analyser as any);

    expect(analyser.fftSize).toBe(999);
  });

  it('should allow calling updateLookAt and updateExpressions', () => {
    const model = use3DModel();
    model.updateLookAt(10, 20);
    model.updateExpressions({ eyeBlinkLeft: 1.0 });
  });

  it('should attempt loading VRM and handle success', async () => {
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
    const canvas = document.createElement('canvas');
    model.initRenderer(canvas);
    await model.loadModel('models/avatar.vrm');

    expect(model.currentModelFormat.value).toBe('vrm');

    // U31(b): gộp skeleton thay vì tỉa joint.
    //
    // three-vrm 3.5.2 tự in cảnh báo cho `removeUnnecessaryJoints` ("deprecated
    // … will be removed in the next major version") và chỉ sang `combineSkeletons`.
    // Khẳng định cả hai chiều: có gọi hàm mới, VÀ đã thôi gọi hàm cũ — thiếu vế
    // sau thì một lần thêm lại hàm cũ sẽ lọt qua mà không ai biết.
    const { VRMUtils } = await import('@pixiv/three-vrm');
    expect(VRMUtils.combineSkeletons).toHaveBeenCalledTimes(1);
    expect(VRMUtils.removeUnnecessaryJoints).not.toHaveBeenCalled();
    expect(VRMUtils.removeUnnecessaryVertices).toHaveBeenCalledTimes(1);
    // `combineMorphs` cố ý CHƯA bật: nó tái cấu trúc đường morph đang dẫn chớp
    // mắt / khẩu hình / biểu cảm, và phiên này không nhìn tận mắt được để kiểm.
    expect(VRMUtils.combineMorphs).not.toHaveBeenCalled();

    model.updateLookAt(10, 20);
    model.updateExpressions({
      happy: 0.5,
      sad: 0,
      surprised: 0,
      angry: 0,
      blink: 0.2,
      blinkLeft: 0.2,
      blinkRight: 0.2,
      mouthOpen: 0.3,
      browUpLeft: 0,
      browUpRight: 0,
    });
    model.startAutoBlink();
    model.startLipSync();
    model.triggerMotion();
    model.setFaceTrackingActive(true);
    model.startRenderLoop();
    model.stopRenderLoop();
    model.dispose();
  });

  it('forwards real locomotion speed to the avatar animation', () => {
    const model = use3DModel();

    model.setLocomotionState('walk', 0.25);

    expect(animationSetMotionWeightMock).toHaveBeenCalledWith(0.25);
    expect(animationSetStateMock).toHaveBeenCalledWith('walk');
  });

  it('forwards conversational thinking state to the avatar animation', () => {
    const model = use3DModel();

    model.setThinking(true);
    model.setThinking(false);

    expect(animationSetThinkingMock).toHaveBeenNthCalledWith(1, true);
    expect(animationSetThinkingMock).toHaveBeenNthCalledWith(2, false);
  });

  it('triggers barge-in reaction: stops lip-sync, flashes surprised and zeros mouth', async () => {
    const scene = new THREE.Object3D();
    const setExpressionValueMock = vi.fn();
    const vrmInstance = {
      scene,
      humanoid: {},
      expressionManager: { setValue: setExpressionValueMock, getValue: vi.fn(), update: vi.fn() },
      lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
      update: vi.fn(),
    };
    mockLoadGLTF.mockImplementation((_url, onLoad) => {
      onLoad({ userData: { vrm: vrmInstance }, scene });
    });

    const model = use3DModel();
    await model.loadModel('models/avatar.vrm');

    model.onBargeIn();

    expect(setExpressionValueMock).toHaveBeenCalledWith('surprised', 0.45);
    expect(setExpressionValueMock).toHaveBeenCalledWith('aa', 0);
    expect(setExpressionValueMock).toHaveBeenCalledWith('ih', 0);
    expect(setExpressionValueMock).toHaveBeenCalledWith('ou', 0);
    expect(setExpressionValueMock).toHaveBeenCalledWith('ee', 0);
    expect(setExpressionValueMock).toHaveBeenCalledWith('oh', 0);

    model.dispose();
  });

  it('registers each successfully retargeted Mixamo clip and keeps missing states on fallback', async () => {
    const scene = new THREE.Object3D();
    const vrmInstance = { scene, humanoid: {}, update: vi.fn() };
    mockLoadGLTF.mockImplementation((_url, onLoad) => {
      onLoad({ userData: { vrm: vrmInstance }, scene });
    });
    loadMixamoAnimationSetMock.mockResolvedValue({
      clips: {
        walk: { name: 'walk', duration: 1, tracks: {} },
        wave: { name: 'wave', duration: 1, tracks: {} },
      },
      failures: { run: 'missing run' },
    });

    const model = use3DModel();
    await model.loadModel('models/avatar.vrm');
    const result = await model.loadAnimationClips();

    expect(result.loaded).toEqual(['walk', 'wave']);
    expect(result.failures).toEqual({ run: 'missing run' });
    expect(model.hasAnimationClip('walk')).toBe(true);
    expect(model.hasAnimationClip('run')).toBe(false);
  });

  it('updates retargeted animation before vrm.update so spring bones see the current frame', async () => {
    const scene = new THREE.Object3D();
    const vrmUpdate = vi.fn();
    const vrmInstance = {
      scene,
      humanoid: { getNormalizedBoneNode: vi.fn(() => null) },
      expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
      update: vrmUpdate,
    };
    mockLoadGLTF.mockImplementation((_url, onLoad) => {
      onLoad({ userData: { vrm: vrmInstance }, scene });
    });

    const model = use3DModel();
    await model.loadModel('models/avatar.vrm');
    animationUpdateMock.mockClear();
    vrmUpdate.mockClear();
    model.startRenderLoop();

    expect(animationUpdateMock).toHaveBeenCalled();
    expect(vrmUpdate).toHaveBeenCalled();
    expect(animationUpdateMock.mock.invocationCallOrder[0]).toBeLessThan(
      vrmUpdate.mock.invocationCallOrder[0]
    );
    model.stopRenderLoop();
  });

  it('uses one frame delta for locomotion callbacks and skeleton animation', async () => {
    const frameUpdate = vi.fn();
    const scene = new THREE.Object3D();
    mockLoadGLTF.mockImplementation((_url, onLoad) => {
      onLoad({
        userData: {
          vrm: {
            scene,
            humanoid: { getNormalizedBoneNode: vi.fn(() => null) },
            expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
            update: vi.fn(),
          },
        },
        scene,
      });
    });
    const model = use3DModel();
    await model.loadModel('models/avatar.vrm');
    animationUpdateMock.mockClear();

    model.setFrameUpdate(frameUpdate);
    model.startRenderLoop();

    expect(frameUpdate).toHaveBeenCalledTimes(1);
    expect(animationUpdateMock).toHaveBeenCalledTimes(1);
    expect(frameUpdate.mock.calls[0][0]).toBe(animationUpdateMock.mock.calls[0][1]);
    expect(frameUpdate.mock.invocationCallOrder[0]).toBeLessThan(
      animationUpdateMock.mock.invocationCallOrder[0]
    );
    model.stopRenderLoop();
  });

  it('caps full-screen render pixels instead of multiplying a 4K canvas by device DPR', () => {
    const originalDpr = window.devicePixelRatio;
    Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 2 });
    const model = use3DModel();

    model.initRenderer(document.createElement('canvas'), 3840, 2160);

    const pixelRatio = rendererInstances.at(-1)?.setPixelRatio.mock.calls.at(-1)?.[0];
    expect(pixelRatio).toBeCloseTo(0.5, 5);
    Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: originalDpr });
    model.dispose();
  });

  it('should attempt loading FBX and handle success', async () => {
    mockLoadFBX.mockImplementation((url, onLoad, onProgress, onError) => {
      const fbx = new THREE.Object3D() as any;
      fbx.animations = [new THREE.AnimationClip('idle', -1, [])];
      fbx.traverse = vi.fn();
      onLoad(fbx);
    });

    const model = use3DModel();
    const canvas = document.createElement('canvas');
    model.initRenderer(canvas);
    await model.loadModel('models/avatar.fbx');

    expect(model.currentModelFormat.value).toBe('fbx');
    model.startRenderLoop();
    model.stopRenderLoop();
    model.dispose();
  });

  it('should handle loader errors', async () => {
    mockLoadGLTF.mockImplementation((url, onLoad, onProgress, onError) => {
      onError(new Error('Failed to load'));
    });

    const model = use3DModel();
    const canvas = document.createElement('canvas');
    model.initRenderer(canvas);
    await expect(model.loadModel('models/avatar.vrm')).rejects.toThrow('Failed to load');
  });

  it('should dispose model and clean up', async () => {
    const model = use3DModel();
    model.dispose();
  });

  // ═══════════════════════════════════════════════════════
  //  Định vị nhân vật trên khung vẽ toàn màn hình
  // ═══════════════════════════════════════════════════════
  describe('vị trí trên màn hình', () => {
    /** Nạp một model FBX có hình khối thật để Box3 đo được (Object3D rỗng cho hộp vô hạn) */
    async function loadBoxModel(width: number, height: number) {
      mockLoadFBX.mockImplementation((url: string, onLoad: (fbx: unknown) => void) => {
        const THREEAny = THREE as any;
        const group = new THREEAny.Group();
        group.add(
          new THREEAny.Mesh(new THREEAny.BoxGeometry(1, 2, 1), new THREEAny.MeshBasicMaterial())
        );
        onLoad(group);
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), width, height);
      await model.loadModel('models/avatar.fbx');
      return model;
    }

    it('neo nhân vật theo toạ độ chuẩn hoá mà KHÔNG cần vòng lặp render', async () => {
      const model = await loadBoxModel(1000, 500);

      // Không gọi startRenderLoop(): getScreenBounds phải tự làm mới matrixWorld.
      // ECO mode và demote 'freeze' đều chặn render loop, nhưng zone gửi sang Rust
      // vẫn phải đúng chỗ — nếu không, vùng bắt chuột đứng lại ở vị trí cũ.
      model.setScreenPosition(0.5, 1.0);
      const middle = model.getScreenBounds();
      expect(middle).not.toBeNull();
      expect(middle!.x + middle!.width / 2).toBeCloseTo(500, 0);
      expect(middle!.y + middle!.height).toBeCloseTo(500, 0); // chân chạm đáy

      model.setScreenPosition(0.2, 1.0);
      const left = model.getScreenBounds()!;
      model.setScreenPosition(0.8, 1.0);
      const right = model.getScreenBounds()!;

      const cx = (b: { x: number; width: number }) => b.x + b.width / 2;
      expect(cx(left)).toBeLessThan(cx(middle!));
      expect(cx(middle!)).toBeLessThan(cx(right));
      expect(cx(left)).toBeCloseTo(200, -1);
      expect(cx(right)).toBeCloseTo(800, -1);

      model.dispose();
    });

    it('nâng nhân vật lên khi giảm toạ độ y', async () => {
      const model = await loadBoxModel(1000, 500);

      model.setScreenPosition(0.5, 1.0);
      const onFloor = model.getScreenBounds()!;
      model.setScreenPosition(0.5, 0.5);
      const raised = model.getScreenBounds()!;

      expect(raised.y + raised.height).toBeLessThan(onFloor.y + onFloor.height);
      expect(raised.y + raised.height).toBeCloseTo(250, -1);

      model.dispose();
    });

    it('giữ nguyên vị trí chuẩn hoá sau khi đổi kích thước khung vẽ', async () => {
      const model = await loadBoxModel(1000, 500);
      model.setScreenPosition(0.75, 1.0);

      model.resize(600, 400);
      const after = model.getScreenBounds()!;

      // 0.75 của bề ngang mới = 450px, chân vẫn ở đáy mới = 400px
      expect(after.x + after.width / 2).toBeCloseTo(450, -1);
      expect(after.y + after.height).toBeCloseTo(400, 0);

      model.dispose();
    });

    it('thu nhỏ hộp bao khi giảm tỉ lệ nhân vật', async () => {
      const model = await loadBoxModel(1000, 500);
      model.setScreenPosition(0.5, 1.0);

      model.setScale(0.45);
      const small = model.getScreenBounds()!;
      model.setScale(0.9);
      const big = model.getScreenBounds()!;

      expect(big.height).toBeGreaterThan(small.height);
      expect(big.width).toBeGreaterThan(small.width);

      model.dispose();
    });

    it('trả null khi chưa nạp model', () => {
      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      expect(model.getScreenBounds()).toBeNull();
      model.dispose();
    });

    it('bỏ qua kích thước không hợp lệ và kẹp toạ độ về [0,1]', async () => {
      const model = await loadBoxModel(1000, 500);

      model.resize(0, 0); // không được làm hỏng trạng thái
      model.setScreenPosition(-5, 42);
      expect(model.getScreenPosition()).toEqual({ x: 0, y: 1 });

      const bounds = model.getScreenBounds();
      expect(bounds).not.toBeNull();

      model.dispose();
    });

    it('hướng thân và mắt về một điểm màn hình cụ thể', () => {
      const model = use3DModel();
      model.setScreenPosition(0.8, 1);

      expect(model.lookAtScreenPoint(0.2, 0.6)).toEqual({
        direction: -1,
        yaw: -45,
        pitch: -28,
      });

      model.dispose();
    });
  });

  // ═══════════════════════════════════════════════════════
  //  Rim Lighting (Slice A4) & 6-Step Humanoid Pose Pipeline (Slice A1)
  // ═══════════════════════════════════════════════════════
  describe('Rim Lighting & Humanoid Upper-Body Kinematics Pipeline', () => {
    it('gắn rim light DirectionalLight ở vị trí rear-top chiếu về camera', () => {
      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);

      const dirLights = model.scene.children.filter(
        (child): child is THREE.DirectionalLight => child instanceof THREE.DirectionalLight
      );
      // Main dirLight, fillLight, rimLight
      expect(dirLights.length).toBeGreaterThanOrEqual(3);

      const rimLight = dirLights.find((light) => light.position.z < 0 && light.position.y > 1.5);
      expect(rimLight).toBeDefined();
      expect(rimLight!.intensity).toBeGreaterThan(0);
      expect(rimLight!.position.z).toBeLessThan(0); // phía sau avatar
      expect(rimLight!.position.y).toBeGreaterThan(1.0); // phía trên đỉnh đầu avatar

      model.dispose();
    });

    it('thực thi đúng quy trình 6 bước: motionWeight = 0 chỉ có idle breathing, motionWeight = 1 kết hợp additively', async () => {
      const scene = new THREE.Object3D();
      const bones = new Map<string, THREE.Object3D>();
      const spineNode = new THREE.Object3D();
      const headNode = new THREE.Object3D();
      const neckNode = new THREE.Object3D();
      const chestNode = new THREE.Object3D();
      bones.set('spine', spineNode);
      bones.set('head', headNode);
      bones.set('neck', neckNode);
      bones.set('chest', chestNode);

      const getNormalizedBoneNode = vi.fn((name: string) => {
        if (!bones.has(name)) {
          const obj = new THREE.Object3D();
          bones.set(name, obj);
        }
        return bones.get(name)!;
      });

      const vrmUpdateMock = vi.fn();
      const vrmInstance = {
        scene,
        humanoid: { getNormalizedBoneNode },
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
        update: vrmUpdateMock,
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      // ── Test 1: motionWeight = 0 (đứng yên) ──
      animationGetMotionWeightMock.mockReturnValue(0);
      animationGetStridePhaseMock.mockReturnValue(0);
      animationGetStateMock.mockReturnValue('idle');

      model.startRenderLoop();

      // Tại motionWeight = 0: spine.rotation.y = 0 (không counter-rotation),
      // spine.rotation.x chỉ có breathing oscillation
      expect(spineNode.rotation.y).toBe(0);
      expect(spineNode.rotation.x).toBeCloseTo(0, 4);

      model.stopRenderLoop();

      // ── Test 1b: motionWeight = 0, thinkingWeight = 1 (nghiêng đầu trước token đầu tiên) ──
      animationGetThinkingWeightMock.mockReturnValue(1);
      model.startRenderLoop();
      expect(headNode.rotation.z).toBeCloseTo(0.10, 4);
      expect(headNode.rotation.x).toBeGreaterThan(0.03);
      model.stopRenderLoop();
      animationGetThinkingWeightMock.mockReturnValue(0);

      // ── Test 2: motionWeight = 1, state = 'walk' ──
      animationGetMotionWeightMock.mockReturnValue(1);
      const phase = Math.PI / 2; // sin(phase) = 1
      animationGetStridePhaseMock.mockReturnValue(phase);
      animationGetStateMock.mockReturnValue('walk');

      model.startRenderLoop();

      // Spine yaw counter-rotation: -Math.sin(π/2) * 0.08 * 1 = -0.08
      expect(spineNode.rotation.y).toBeCloseTo(-0.08, 4);
      // Spine pitch forward lean: 0.06 * 1 + breathCycle (additive)
      expect(spineNode.rotation.x).toBeCloseTo(0.06, 2);

      // Head pitch stabilization (-spine.rotation.x * 0.6 = -0.036) combined additively with OpenSimplex swayX
      expect(headNode.rotation.x).toBeLessThan(0);
      expect(headNode.rotation.y).toBeGreaterThan(0);

      model.stopRenderLoop();

      // ── Test 2b: Khi faceTrackingActive = true, micro-sway tắt và head.rotation.x thuần là stabilization ──
      model.setFaceTrackingActive(true);
      model.startRenderLoop();
      expect(headNode.rotation.x).toBeCloseTo(-0.06 * 0.6, 4);
      model.stopRenderLoop();
      model.setFaceTrackingActive(false);

      // ── Test 3: motionWeight = 1, state = 'run' ──
      animationGetMotionWeightMock.mockReturnValue(1);
      animationGetStridePhaseMock.mockReturnValue(0);
      animationGetStateMock.mockReturnValue('run');

      model.startRenderLoop();

      // Run lean = 0.2 * 1 = 0.2 (+ breathCycle)
      expect(spineNode.rotation.x).toBeCloseTo(0.2, 2);
      expect(headNode.rotation.x).toBeLessThan(-0.08);

      model.stopRenderLoop();
      model.dispose();
    });

    it('reset rest pose trước mỗi khung hình ngăn tích luỹ rotation không kiểm soát', async () => {
      const scene = new THREE.Object3D();
      const bones = new Map<string, THREE.Object3D>();
      const spineNode = new THREE.Object3D();
      const headNode = new THREE.Object3D();
      const neckNode = new THREE.Object3D();
      const chestNode = new THREE.Object3D();
      bones.set('spine', spineNode);
      bones.set('head', headNode);
      bones.set('neck', neckNode);
      bones.set('chest', chestNode);

      const getNormalizedBoneNode = vi.fn((name: string) => {
        if (!bones.has(name)) {
          const obj = new THREE.Object3D();
          bones.set(name, obj);
        }
        return bones.get(name)!;
      });

      const vrmInstance = {
        scene,
        humanoid: { getNormalizedBoneNode },
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
        update: vi.fn(),
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      // Giả lập frame trước bị bẩn rotation x, y, z trên cả 4 xương
      spineNode.rotation.set(1.5, 2.0, 2.5);
      headNode.rotation.set(0.5, 0.6, 0.7);
      neckNode.rotation.set(0.2, 0.3, 0.4);
      chestNode.rotation.set(0.8, 0.9, 1.1);

      animationGetMotionWeightMock.mockReturnValue(0);
      animationGetStateMock.mockReturnValue('idle');

      // Tắt sway và breathing để kiểm tra rest pose thuần
      model.setFaceTrackingActive(true);
      model.startRenderLoop();

      // Rest pose reset đã xoá toàn bộ góc xoay về 0
      expect(spineNode.rotation.y).toBe(0);
      expect(spineNode.rotation.z).toBe(0);
      expect(headNode.rotation.x).toBe(0);
      expect(headNode.rotation.y).toBe(0);
      expect(headNode.rotation.z).toBe(0);
      expect(neckNode.rotation.x).toBe(0);
      expect(neckNode.rotation.y).toBe(0);
      expect(neckNode.rotation.z).toBe(0);
      expect(chestNode.rotation.x).toBe(0);
      expect(chestNode.rotation.y).toBe(0);
      expect(chestNode.rotation.z).toBe(0);

      model.stopRenderLoop();
      model.dispose();
    });

    it('an toàn khi model VRM thiếu node spine hoặc head', async () => {
      const scene = new THREE.Object3D();
      const vrmInstance = {
        scene,
        humanoid: {
          getNormalizedBoneNode: (_name: string) => null,
        },
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
        update: vi.fn(),
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      animationGetMotionWeightMock.mockReturnValue(1);
      animationGetStateMock.mockReturnValue('walk');
      animationGetStridePhaseMock.mockReturnValue(Math.PI / 2);

      expect(() => {
        model.startRenderLoop();
      }).not.toThrow();

      model.stopRenderLoop();
      model.dispose();
    });

    it('chuyển động thân trên không làm xê dịch root.position.y hoặc pelvis translation', async () => {
      const scene = new THREE.Object3D();
      const bones = new Map<string, THREE.Object3D>();
      const spineNode = new THREE.Object3D();
      const headNode = new THREE.Object3D();
      const neckNode = new THREE.Object3D();
      const chestNode = new THREE.Object3D();
      bones.set('spine', spineNode);
      bones.set('head', headNode);
      bones.set('neck', neckNode);
      bones.set('chest', chestNode);

      const getNormalizedBoneNode = vi.fn((name: string) => {
        if (!bones.has(name)) {
          const obj = new THREE.Object3D();
          bones.set(name, obj);
        }
        return bones.get(name)!;
      });

      const vrmInstance = {
        scene,
        humanoid: { getNormalizedBoneNode },
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
        update: vi.fn(),
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      model.setScreenPosition(0.5, 1.0);
      const initialPos = model.getScreenPosition();

      animationGetMotionWeightMock.mockReturnValue(1);
      animationGetStateMock.mockReturnValue('run');
      animationGetStridePhaseMock.mockReturnValue(Math.PI / 4);

      model.startRenderLoop();

      // Screen position không bị thay đổi bởi upper-body procedurals
      expect(model.getScreenPosition()).toEqual(initialPos);

      model.stopRenderLoop();
      model.dispose();
    });

    it('gọi initRenderer() nhiều lần không sinh ra đèn trùng lặp trong scene', () => {
      const model = use3DModel();
      const canvas1 = document.createElement('canvas');
      const canvas2 = document.createElement('canvas');

      model.initRenderer(canvas1, 800, 600);
      const initialLightCount = model.scene.children.filter(
        (child) => child instanceof THREE.Light
      ).length;

      // Gọi lại initRenderer lần 2 (ví dụ khi canvas re-mount)
      model.initRenderer(canvas2, 800, 600);
      const secondLightCount = model.scene.children.filter(
        (child) => child instanceof THREE.Light
      ).length;

      expect(secondLightCount).toBe(initialLightCount);

      const dirLights = model.scene.children.filter(
        (c): c is THREE.DirectionalLight => c instanceof THREE.DirectionalLight
      );
      const rimLights = dirLights.filter((l) => l.position.z < 0 && l.position.y > 1.5);
      expect(rimLights.length).toBe(1);

      model.dispose();
    });

    it('updateLocomotionUpperBody miễn nhiễm với giá trị NaN hoặc không hợp lệ từ animation', async () => {
      const scene = new THREE.Object3D();
      const spineNode = new THREE.Object3D();
      const headNode = new THREE.Object3D();
      const bones = new Map<string, THREE.Object3D>([
        ['spine', spineNode],
        ['head', headNode],
      ]);

      const vrmInstance = {
        scene,
        humanoid: {
          getNormalizedBoneNode: (name: string) => bones.get(name) || null,
        },
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
        update: vi.fn(),
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      animationGetMotionWeightMock.mockReturnValue(NaN as any);
      animationGetStridePhaseMock.mockReturnValue(NaN as any);

      model.startRenderLoop();

      expect(Number.isFinite(spineNode.rotation.x)).toBe(true);
      expect(Number.isFinite(spineNode.rotation.y)).toBe(true);
      expect(Number.isFinite(headNode.rotation.x)).toBe(true);
      expect(Number.isFinite(headNode.rotation.y)).toBe(true);

      model.stopRenderLoop();
      model.dispose();
    });

    it('upper-body kinematics tỉ lệ thuận mượt mà với intermediate motionWeight (0.5, 0.25)', async () => {
      const scene = new THREE.Object3D();
      const spineNode = new THREE.Object3D();
      const headNode = new THREE.Object3D();
      const bones = new Map<string, THREE.Object3D>([
        ['spine', spineNode],
        ['head', headNode],
      ]);

      const vrmInstance = {
        scene,
        humanoid: {
          getNormalizedBoneNode: (name: string) => bones.get(name) || null,
        },
        expressionManager: { setValue: vi.fn(), getValue: vi.fn(), update: vi.fn() },
        lookAt: { applier: { lookAt: vi.fn(), applyYawPitch: vi.fn() } },
        update: vi.fn(),
      };

      mockLoadGLTF.mockImplementation((_url, onLoad) => {
        onLoad({ userData: { vrm: vrmInstance }, scene });
      });

      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      const phase = Math.PI / 2;
      animationGetStridePhaseMock.mockReturnValue(phase);
      animationGetStateMock.mockReturnValue('walk');

      // Tắt sway để đo độ thuần của kinematics
      model.setFaceTrackingActive(true);

      // Tại motionWeight = 0.5
      animationGetMotionWeightMock.mockReturnValue(0.5);
      model.startRenderLoop();

      // counter-rotation: -sin(π/2) * 0.08 * 0.5 = -0.04
      expect(spineNode.rotation.y).toBeCloseTo(-0.04, 4);
      // lean: 0.06 * 0.5 = 0.03 (+ breathCycle)
      expect(spineNode.rotation.x).toBeCloseTo(0.03, 2);
      // head leveling: -0.03 * 0.6 = -0.018
      expect(headNode.rotation.x).toBeCloseTo(-0.018, 4);

      model.stopRenderLoop();

      // Tại motionWeight = 0.25
      animationGetMotionWeightMock.mockReturnValue(0.25);
      model.startRenderLoop();

      // counter-rotation: -sin(π/2) * 0.08 * 0.25 = -0.02
      expect(spineNode.rotation.y).toBeCloseTo(-0.02, 4);
      // lean: 0.06 * 0.25 = 0.015 (+ breathCycle)
      expect(spineNode.rotation.x).toBeCloseTo(0.015, 2);

      model.stopRenderLoop();
      model.dispose();
    });

    it("khởi tạo contact shadow mesh dưới chân avatar và giải phóng sạch sẽ khi dispose", () => {
      const model = use3DModel();
      model.initRenderer(document.createElement('canvas'), 800, 600);

      // Tìm mesh đổ bóng trong scene hierarchy của avatarRoot
      const avatarRoot = model.scene.children.find((c) => c instanceof THREE.Group);
      expect(avatarRoot).toBeDefined();

      const shadowMesh = avatarRoot?.children.find(
        (c) => c instanceof THREE.Mesh && c.position.y === 0.002
      );
      expect(shadowMesh).toBeDefined();

      model.dispose();
      const shadowAfterDispose = avatarRoot?.children.find(
        (c) => c instanceof THREE.Mesh && c.position.y === 0.002
      );
      expect(shadowAfterDispose).toBeUndefined();
    });

    it("lookAtScreenPoint tính toán góc yaw, pitch chuẩn và kẹp góc an toàn", () => {
      const model = use3DModel();
      // Mặc định avatarScreenX = 0.5, avatarScreenY = 1.0
      // Nhìn điểm giữa (0.5, 0.5): dx = 0, dy = -0.5
      const resCenter = model.lookAtScreenPoint(0.5, 0.5);
      expect(resCenter.direction).toBe(1);
      expect(resCenter.yaw).toBe(0);
      expect(resCenter.pitch).toBe(-35); // -0.5 * 70 = -35 (clamped)

      // Nhìn điểm lệch phải (0.7, 0.8): dx = 0.2, dy = -0.2
      const resRight = model.lookAtScreenPoint(0.7, 0.8);
      expect(resRight.direction).toBe(1);
      expect(resRight.yaw).toBe(18); // 0.2 * 90 = 18
      expect(resRight.pitch).toBe(-14); // -0.2 * 70 = -14

      // Điểm cực biên vượt quá màn hình: yaw và pitch phải được kẹp an toàn [-45, 45] và [-35, 35]
      const resExtreme = model.lookAtScreenPoint(1.5, -0.5);
      expect(resExtreme.yaw).toBeLessThanOrEqual(45);
      expect(resExtreme.yaw).toBeGreaterThanOrEqual(-45);
      expect(resExtreme.pitch).toBeLessThanOrEqual(35);
      expect(resExtreme.pitch).toBeGreaterThanOrEqual(-35);

      model.dispose();
    });

    it("lookAtScreenPoint bù trừ toạ độ windowOffset chính xác khi chạy trên multi-monitor desktop", () => {
      const model = use3DModel();
      // Giả lập cửa sổ widget đặt tại offset (0.2, 0.1) trên màn hình desktop ảo
      const windowOffset = { x: 0.2, y: 0.1 };
      // Phần tử màn hình desktop tại (0.8, 0.6)
      // Tọa độ mục tiêu hiệu dụng = (0.8 - 0.2, 0.6 - 0.1) = (0.6, 0.5)
      // dx = 0.6 - 0.5 = 0.1, dy = 0.5 - 1.0 = -0.5
      const res = model.lookAtScreenPoint(0.8, 0.6, windowOffset);
      expect(res.direction).toBe(1);
      expect(res.yaw).toBe(9); // 0.1 * 90 = 9
      expect(res.pitch).toBe(-35); // -0.5 * 70 = -35 (clamped)

      model.dispose();
    });

    it('setThinking kích hoạt animation và hướng ánh nhìn lên camera', () => {
      const model = use3DModel();
      model.setThinking(true);
      expect(animationSetThinkingMock).toHaveBeenCalledWith(true);

      model.setThinking(false);
      expect(animationSetThinkingMock).toHaveBeenCalledWith(false);
      model.dispose();
    });

    it('onBargeIn dập tắt lip sync và thiết lập biểu cảm ngạc nhiên phản xạ', async () => {
      mockLoadGLTF.mockImplementation((_path, onLoad) => {
        const scene = new THREE.Group();
        const mockVRMInstance = {
          scene,
          humanoid: null,
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
        onLoad({ userData: { vrm: mockVRMInstance }, scene });
      });

      const model = use3DModel();
      const canvas = document.createElement('canvas');
      model.initRenderer(canvas);
      await model.loadModel('models/avatar.vrm');

      const vrmInst = model.vrm.value!;
      const em = vrmInst.expressionManager;

      model.onBargeIn();

      expect(em.setValue).toHaveBeenCalledWith('surprised', 0.45);
      expect(em.setValue).toHaveBeenCalledWith('aa', 0);
      expect(em.setValue).toHaveBeenCalledWith('oh', 0);
      expect(em.setValue).toHaveBeenCalledWith('ee', 0);
      expect(em.setValue).toHaveBeenCalledWith('ih', 0);
      expect(em.setValue).toHaveBeenCalledWith('ou', 0);

      model.dispose();
    });
  });

  describe('Fixational Eye Micro-Saccades Integration (Step 5 Pose Pipeline)', () => {
    it('tích hợp vi dao động mắt cộng dồn vào applyYawPitch mà không làm trôi base gaze', async () => {
      const scene = new THREE.Object3D();
      const applyYawPitchMock = vi.fn();
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
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      // 1. Trạng thái ban đầu: offset bằng 0
      expect(model.getSaccadeOffset()).toEqual({ yaw: 0, pitch: 0 });

      // 2. Kích hoạt bước nhảy saccade có chủ đích (+0.60°, -0.35°)
      model.triggerSaccade(0.60, -0.35);

      model.startRenderLoop();

      expect(applyYawPitchMock).toHaveBeenCalled();
      const lastCall = applyYawPitchMock.mock.calls[applyYawPitchMock.mock.calls.length - 1];
      const appliedYaw = lastCall[0];
      const appliedPitch = lastCall[1];

      // Biên độ không vượt trần ±0.85°
      expect(Math.abs(appliedYaw)).toBeLessThanOrEqual(0.85);
      expect(Math.abs(appliedPitch)).toBeLessThanOrEqual(0.85);

      model.stopRenderLoop();
      model.dispose();
    });

    it('bảo toàn tính độc lập của xương thân trên, không làm biến đổi spine hay head rotation.z', async () => {
      const scene = new THREE.Object3D();
      const spineNode = new THREE.Object3D();
      const headNode = new THREE.Object3D();
      const bones = new Map<string, THREE.Object3D>([
        ['spine', spineNode],
        ['head', headNode],
      ]);

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
      model.initRenderer(document.createElement('canvas'), 800, 600);
      await model.loadModel('models/avatar.vrm');

      // Kích hoạt chu kỳ saccade
      model.triggerSaccade(0.80, 0.50);
      model.startRenderLoop();

      // Khẳng định spine và head rotation.z không bị ảnh hưởng bởi saccade
      expect(spineNode.rotation.z).toBe(0);
      expect(headNode.rotation.z).toBe(0);

      model.stopRenderLoop();
      model.dispose();
    });

    it('setSaccadesEnabled(false) triệt tiêu hoàn toàn vi dao động mắt về 0', async () => {
      const model = use3DModel();
      model.triggerSaccade(0.70, -0.40);
      model.setSaccadesEnabled(false);

      expect(model.getSaccadeOffset()).toEqual({ yaw: 0, pitch: 0 });
      model.dispose();
    });
  });
});

