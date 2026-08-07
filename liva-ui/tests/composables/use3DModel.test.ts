import { describe, it, expect, vi, beforeEach } from 'vitest';
import { use3DModel } from '../../src/composables/use3DModel';
import * as THREE from 'three';

const {
  loadMixamoAnimationSetMock,
  animationSetStateMock,
  animationSetMotionWeightMock,
  animationSetThinkingMock,
  animationUpdateMock,
  animationClips,
  rendererInstances,
} = vi.hoisted(() => ({
  loadMixamoAnimationSetMock: vi.fn(),
  animationSetStateMock: vi.fn(),
  animationSetMotionWeightMock: vi.fn(),
  animationSetThinkingMock: vi.fn(),
  animationUpdateMock: vi.fn(),
  animationClips: new Set<string>(),
  rendererInstances: [] as Array<{ setPixelRatio: ReturnType<typeof vi.fn> }>,
}));

vi.mock('../../src/composables/mixamoClipLoader', () => ({
  DEFAULT_MIXAMO_CLIP_PATHS: {},
  loadMixamoAnimationSet: loadMixamoAnimationSetMock,
}));

vi.mock('../../src/composables/useAvatarAnimation', () => ({
  useAvatarAnimation: () => ({
    setState: animationSetStateMock,
    setMotionWeight: animationSetMotionWeightMock,
    getState: vi.fn(() => 'idle'),
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
});
