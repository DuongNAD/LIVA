/**
 * use3DModel.ts — Vue Composable for 3D Avatar (VRM + FBX)
 * ==========================================================
 * Handles: VRM and FBX model loading, auto-blink, lip-sync, lookAt, Deep Dispose.
 * Face Tracking: updateLookAt() + updateExpressions() for MediaPipe (VRM only).
 * Idle: OpenSimplex noise-based micro-sway + breathing.
 * FBX: Auto-scale/center via Box3, AnimationMixer for embedded clips.
 */
import * as THREE from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { FBXLoader } from 'three/examples/jsm/loaders/FBXLoader.js';
import { VRMLoaderPlugin, VRM, VRMUtils } from '@pixiv/three-vrm';
import { ref, shallowRef, type Ref, type ShallowRef } from 'vue';
import type { FaceExpressions } from './useFaceTracking';
import {
  useAvatarAnimation,
  type AvatarAnimationApi,
  type AvatarClipState,
  type LocomotionState,
} from './useAvatarAnimation';
import { DEFAULT_MIXAMO_CLIP_PATHS, loadMixamoAnimationSet } from './mixamoClipLoader';
import {
  easeInQuad,
  easeOutQuad,
  lerp,
  randomBlinkInterval,
  weightedRandom,
} from '../utils/avatarMath';
import { logger } from '../utils/logger';
import { currentVisemeFromClock } from '../utils/phonemeLipSync';

// ═══════════════════════════════════════════
//  OpenSimplex 2D Noise (inline, zero-dep)
//  Value noise with smooth gradients — never repeats
// ═══════════════════════════════════════════
const STRETCH_2D = (Math.sqrt(3) - 1) / 2;
const SQUISH_2D = (1 / Math.sqrt(3) - 1) / 2;

// Deterministic gradient table (seeded via permutation)
const GRADIENTS_2D = [5, 2, 2, 5, -5, 2, -2, 5, 5, -2, 2, -5, -5, -2, -2, -5];

// Generate permutation table with seed
function buildPerm(seed: number): Int16Array {
  const perm = new Int16Array(256);
  const source = new Int16Array(256);
  for (let i = 0; i < 256; i++) source[i] = i;
  seed = Math.trunc(seed * 6364136223 + 1442695040);
  for (let i = 255; i >= 0; i--) {
    seed = (seed * 25214903917 + 11) & 0xffffffffffff;
    let r = (seed + 31) % (i + 1);
    if (r < 0) r += i + 1;
    perm[i] = source[r];
    source[r] = source[i];
  }
  return perm;
}

const PERM = buildPerm(42); // Fixed seed for consistency

/**
 * 2D OpenSimplex noise. Returns value in [-1, 1].
 */
function simplex2D(x: number, y: number): number {
  const stretchOffset = (x + y) * STRETCH_2D;
  const xs = x + stretchOffset;
  const ys = y + stretchOffset;
  const xsb = Math.floor(xs);
  const ysb = Math.floor(ys);
  const squishOffset = (xsb + ysb) * SQUISH_2D;
  const dx0 = x - (xsb + squishOffset);
  const dy0 = y - (ysb + squishOffset);
  const xins = xs - xsb;
  const yins = ys - ysb;

  let value = 0;

  // Contribution (0,0)
  const attn0 = 2 - dx0 * dx0 - dy0 * dy0;
  if (attn0 > 0) {
    const attn0sq = attn0 * attn0;
    value += attn0sq * attn0sq * extrapolate(xsb, ysb, dx0, dy0);
  }

  // Contribution (1,0) or (0,1)
  if (xins + yins <= 1) {
    const dx1 = dx0 - 1 - SQUISH_2D;
    const dy1 = dy0 - SQUISH_2D;
    const attn1 = 2 - dx1 * dx1 - dy1 * dy1;
    if (attn1 > 0) {
      const attn1sq = attn1 * attn1;
      value += attn1sq * attn1sq * extrapolate(xsb + 1, ysb, dx1, dy1);
    }
    const dx2 = dx0 - SQUISH_2D;
    const dy2 = dy0 - 1 - SQUISH_2D;
    const attn2 = 2 - dx2 * dx2 - dy2 * dy2;
    if (attn2 > 0) {
      const attn2sq = attn2 * attn2;
      value += attn2sq * attn2sq * extrapolate(xsb, ysb + 1, dx2, dy2);
    }
  } else {
    const dx1 = dx0 - 1 - 2 * SQUISH_2D;
    const dy1 = dy0 - 1 - 2 * SQUISH_2D;
    const attn1 = 2 - dx1 * dx1 - dy1 * dy1;
    if (attn1 > 0) {
      const attn1sq = attn1 * attn1;
      value += attn1sq * attn1sq * extrapolate(xsb + 1, ysb + 1, dx1, dy1);
    }
    const dx2 = dx0 - SQUISH_2D;
    const dy2 = dy0 - 1 - SQUISH_2D;
    const attn2 = 2 - dx2 * dx2 - dy2 * dy2;
    if (attn2 > 0) {
      const attn2sq = attn2 * attn2;
      value += attn2sq * attn2sq * extrapolate(xsb, ysb + 1, dx2, dy2);
    }
    const dx3 = dx0 - 1 - SQUISH_2D;
    const dy3 = dy0 - SQUISH_2D;
    const attn3 = 2 - dx3 * dx3 - dy3 * dy3;
    if (attn3 > 0) {
      const attn3sq = attn3 * attn3;
      value += attn3sq * attn3sq * extrapolate(xsb + 1, ysb, dx3, dy3);
    }
  }

  return value / 6;
}

function extrapolate(xsb: number, ysb: number, dx: number, dy: number): number {
  const index = (PERM[(PERM[xsb & 0xff] + ysb) & 0xff] % 8) * 2;
  return GRADIENTS_2D[index] * dx + GRADIENTS_2D[index + 1] * dy;
}

/**
 * Khoảng cách khung hình tối thiểu khi ECO Mode bật, tính bằng mili-giây.
 * 33 ms ≈ **30 FPS** — sàn của một nhân vật đang hiển thị. Xem U31(d).
 */
export const ECO_FRAME_INTERVAL_MS = 33;

export type ModelFormat = 'vrm' | 'fbx' | null;

export interface Use3DModelReturn {
  vrm: ShallowRef<VRM | null>;
  currentModelFormat: Ref<ModelFormat>;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer | null;
  loadModel: (path: string, onProgress?: (pct: number) => void) => Promise<void>;
  loadAnimationClips: (paths?: Record<AvatarClipState, string>) => Promise<{
    loaded: AvatarClipState[];
    failures: Partial<Record<AvatarClipState, string>>;
  }>;
  hasAnimationClip: (state: AvatarClipState) => boolean;
  initRenderer: (canvas: HTMLCanvasElement, width: number, height: number) => void;
  resize: (width: number, height: number) => void;
  setScreenPosition: (nx: number, ny: number) => void;
  setScale: (scale: number) => void;
  setFacing: (direction: 1 | -1, turned: boolean) => void;
  setLocomotionState: (state: LocomotionState, motionWeight?: number) => void;
  playGesture: (name: 'wave' | 'nod' | 'shake') => void;
  setInspecting: (active: boolean) => void;
  setThinking: (active: boolean) => void;
  lookAtScreenPoint: (
    nx: number,
    ny: number
  ) => {
    direction: 1 | -1;
    yaw: number;
    pitch: number;
  };
  getScreenPosition: () => { x: number; y: number };
  getScreenBounds: () => { x: number; y: number; width: number; height: number } | null;
  setFrameUpdate: (callback: ((delta: number) => void) | null) => void;
  startRenderLoop: () => void;
  stopRenderLoop: () => void;
  startAutoBlink: () => void;
  startLipSync: () => void;
  stopLipSync: () => void;
  /** Read lip-sync from an analyser owned and wired by the playback composable. */
  startAudioDrivenLipSync: (analyser: AnalyserNode) => void;
  stopAudioDrivenLipSync: () => void;
  triggerMotion: () => void;
  updateLookAt: (yaw: number, pitch: number) => void;
  updateExpressions: (expressions: FaceExpressions) => void;
  setFaceTrackingActive: (active: boolean) => void;
  dispose: () => void;
}

/**
 * Deep Dispose — Giải phóng VRAM hoàn toàn
 * Gọi khi swap model hoặc unmount component.
 * Works for BOTH VRM and FBX scenes.
 */
function deepDispose(root: THREE.Object3D) {
  root.traverse((object) => {
    const obj = object as THREE.Object3D & {
      geometry?: { dispose: () => void };
      material?: THREE.Material | THREE.Material[];
      skeleton?: { dispose: () => void };
    };
    // Dispose geometry
    if (obj.geometry) {
      obj.geometry.dispose();
    }

    // Dispose materials + textures
    if (obj.material) {
      const materials = Array.isArray(obj.material) ? obj.material : [obj.material];
      materials.forEach((mat) => {
        // Quét tất cả texture maps (diffuse, normal, emissive, etc.)
        Object.values(mat).forEach((val) => {
          if (
            val &&
            typeof val === 'object' &&
            'isTexture' in val &&
            typeof (val as { dispose?: () => void }).dispose === 'function'
          ) {
            (val as { dispose: () => void }).dispose();
          }
        });
        mat.dispose();
      });
    }

    // Dispose skeleton (FBX models often have these)
    if (object.skeleton) {
      object.skeleton.dispose();
    }
  });
}

export function use3DModel(): Use3DModelReturn {
  const vrm = shallowRef<VRM | null>(null);
  const currentModelFormat = ref<ModelFormat>(null);
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(30, 400 / 700, 0.1, 20);
  let renderer: THREE.WebGLRenderer | null = null;
  let animFrameId: number | null = null;
  const clock = new THREE.Clock();
  let frameUpdate: ((delta: number) => void) | null = null;

  const MAX_RENDER_PIXELS = 1920 * 1080;

  function getRenderPixelRatio(width: number, height: number): number {
    const safeWidth = Number.isFinite(width) && width > 0 ? width : 1;
    const safeHeight = Number.isFinite(height) && height > 0 ? height : 1;
    const deviceRatio = Math.max(window.devicePixelRatio || 1, 1);
    return Math.min(deviceRatio, Math.sqrt(MAX_RENDER_PIXELS / (safeWidth * safeHeight)));
  }

  // ═══════════════════════════════════════════
  //  Avatar Root — lớp dịch chuyển của nhân vật
  //  Model (VRM/FBX) được gắn VÀO group này thay vì thẳng vào scene.
  //  Lý do: autoScaleAndCenter() đã ghi một offset vào position của FBX để
  //  kéo chân về y=0; ghi đè position đó để di chuyển sẽ làm model lệch theo
  //  tâm hộp bao của chính nó. Dịch chuyển group giữ offset ấy nguyên vẹn,
  //  và đây cũng là root mà lớp locomotion sẽ điều khiển sau này.
  // ═══════════════════════════════════════════
  const avatarRoot = new THREE.Group();
  scene.add(avatarRoot);

  // Khung vẽ hiện tại (px) — cần cho resize và phép chiếu world→màn hình
  let viewportWidth = 400;
  let viewportHeight = 700;

  // Vị trí nhân vật trên màn hình, chuẩn hoá [0,1]; (0,0) = góc trái trên.
  // avatarScreenY là vị trí CHÂN, nên mặc định 1.0 = đứng ở đáy màn hình.
  let avatarScreenX = 0.5;
  let avatarScreenY = 1.0;

  // Cỡ nhân vật so với khung nhìn. Ở tỉ lệ 1.0 model cao ~64% chiều cao màn
  // hình — quá lớn cho một nhân vật thường trực trên desktop. 0.45 cho ra
  // ~29%, xấp xỉ cảm giác của khung 400×700 thu nhỏ trước đây.
  let avatarScale = 0.45;

  // Hướng nhìn. Không quay hẳn 90° khi đi ngang: nhân vật trên desktop nên vẫn
  // hơi hướng về phía người dùng, nên chỉ xoay tới MAX_TURN rồi thôi.
  const MAX_TURN = 0.9; // radian, ~52°
  let facingTarget = 0;
  let facingCurrent = 0;

  // Tư thế thân thể (đi/chạy/nhảy/vẫy) — xem useAvatarAnimation.ts
  const animation: AvatarAnimationApi = useAvatarAnimation();

  // FBX state
  let fbxModel: THREE.Group | null = null;
  let mixer: THREE.AnimationMixer | null = null;
  let debugProbe: THREE.Mesh | null = null;

  // Blink state
  let blinkTimer = 0;
  let nextBlinkAt = randomBlinkInterval();
  let blinkPhase: 'idle' | 'closing' | 'opening' | 'closed' = 'idle';
  let blinkProgress = 0;
  let pendingDoubleBlink = false;
  let isBlinking = false;

  // Lip-sync state (procedural fallback)
  let lipSyncActive = false;
  let lipTime = 0;
  let lipSyncRAF: number | null = null;

  // Audio-driven lip-sync state (RMS viseme mapping)
  let audioAnalyserNode: AnalyserNode | null = null;
  let audioAnalyserActive = false;
  let audioFreqData: Uint8Array | null = null;
  /** Smoothed RMS values for 5 frequency bands: [aa, oh, ee, ih, ou] */
  const smoothedBandRMS = new Float32Array(5);

  // Expression animation RAF tracker — prevents multiple simultaneous animation chains
  let expressionRAF: number | null = null;

  // Idle animation state
  let idleTime = 0;
  let microExprTimer = 0;
  let nextMicroExprAt = 5 + Math.random() * 8; // 5-13s
  // NOSONAR
  let activeMicroExpr: string | null = null;
  let microExprIntensity = 0;
  let microExprFading = false;

  // Spring-damped LookAt state
  let currentYaw = 0;
  let currentPitch = 0;
  let targetYaw = 0;
  let targetPitch = 0;

  // Face tracking state — when active, disables auto-blink (real blinks take over)
  let faceTrackingActive = false;

  // ═══════════════════════════════════════════
  //  Renderer Init
  // ═══════════════════════════════════════════
  function initRenderer(canvas: HTMLCanvasElement, width: number, height: number) {
    viewportWidth = width;
    viewportHeight = height;
    renderer = new THREE.WebGLRenderer({
      canvas,
      alpha: true, // Transparent background
      antialias: true,
    });
    const pixelRatio = getRenderPixelRatio(width, height);
    renderer.setClearColor(0x000000, 0); // Fully transparent
    renderer.setPixelRatio(pixelRatio);
    renderer.setSize(width, height);
    (renderer as unknown as Record<string, unknown>).outputColorSpace = (
      THREE as unknown as { SRGBColorSpace: string }
    ).SRGBColorSpace;
    logger.info('[use3DModel]', 'Renderer created', {
      width,
      height,
      pixelRatio,
      hasWebGL2: !!(renderer as unknown as { capabilities?: { isWebGL2?: boolean } }).capabilities
        ?.isWebGL2,
    });

    if (!debugProbe) {
      const THREEAny = THREE as unknown as {
        BoxGeometry: new (w: number, h: number, d: number) => unknown;
        MeshBasicMaterial: new (params: unknown) => unknown;
        Mesh: new (g: unknown, m: unknown) => THREE.Mesh;
      };
      const geometry = new THREEAny.BoxGeometry(0.45, 0.45, 0.45);
      const material = new THREEAny.MeshBasicMaterial({ color: 0x66ccff, wireframe: false });
      debugProbe = new THREEAny.Mesh(geometry, material);
      debugProbe.position.set(0, 1.0, 0);
      scene.add(debugProbe);
      logger.info('[use3DModel]', 'Debug probe added to scene', {
        sceneChildren: (scene as unknown as { children: unknown[] }).children.length,
      });
    }

    // Camera position
    camera.aspect = width / height;
    camera.position.set(0, 1.05, 5.8);
    camera.lookAt(0, 1.0, 0);
    camera.updateProjectionMatrix();

    // Lighting — Enhanced for BOTH MToon (VRM) and PBR (FBX)
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.5);
    scene.add(ambientLight);

    const hemiLight = new THREE.HemisphereLight(0xffffff, 0x444444, 0.6);
    scene.add(hemiLight);

    const dirLight = new THREE.DirectionalLight(0xffffff, 1.2);
    dirLight.position.set(1, 1.5, 1);
    scene.add(dirLight);

    // Subtle fill light from below (prevents dark underside on FBX PBR)
    const fillLight = new THREE.DirectionalLight(0x8888ff, 0.4);
    fillLight.position.set(-1, -0.5, 0.5);
    scene.add(fillLight);

    applyScreenPosition();
  }

  // ═══════════════════════════════════════════
  //  Định vị nhân vật theo toạ độ màn hình
  // ═══════════════════════════════════════════

  /**
   * Kích thước khung nhìn (đơn vị world) tại mặt phẳng z=0 — nơi nhân vật đứng.
   * Camera phối cảnh: chiều cao = 2·d·tan(fov/2); chiều rộng = chiều cao · aspect.
   */
  function getViewSize(): { width: number; height: number } {
    const distance = Math.abs(camera.position.z);
    const height = 2 * distance * Math.tan((camera.fov * Math.PI) / 360);
    return { width: height * camera.aspect, height };
  }

  /** Đưa avatarRoot về đúng vị trí world ứng với toạ độ màn hình đã đặt. */
  function applyScreenPosition() {
    const view = getViewSize();
    const worldX = (avatarScreenX - 0.5) * view.width;
    // Mép trên khung nhìn, rồi đi xuống theo tỉ lệ ny
    const topY = camera.position.y + view.height / 2;
    const worldY = topY - avatarScreenY * view.height;
    avatarRoot.position.set(worldX, worldY, 0);
    avatarRoot.scale.set(avatarScale, avatarScale, avatarScale);
  }

  /**
   * Đổi cỡ nhân vật so với khung nhìn (1.0 = cao ~64% màn hình).
   * Chân vẫn bám đúng avatarScreenY vì scale áp lên chính root đang đặt tại đó.
   */
  function setScale(scale: number) {
    avatarScale = Math.min(Math.max(scale, 0.05), 4);
    applyScreenPosition();
  }

  /**
   * Hướng nhân vật theo chiều đang đi.
   * @param direction 1 = sang phải màn hình, -1 = sang trái
   * @param turned true khi đang di chuyển; false thì quay lại nhìn người dùng
   */
  function setFacing(direction: 1 | -1, turned: boolean) {
    facingTarget = turned ? direction * MAX_TURN : 0;
  }

  function lookAtScreenPoint(nx: number, ny: number) {
    const targetX = Math.min(Math.max(nx, 0), 1);
    const targetY = Math.min(Math.max(ny, 0), 1);
    const dx = targetX - avatarScreenX;
    const dy = targetY - avatarScreenY;
    const direction: 1 | -1 = dx >= 0 ? 1 : -1;
    const yaw = Math.round(Math.min(Math.max(dx * 90, -45), 45) * 1000) / 1000;
    const pitch = Math.round(Math.min(Math.max(dy * 70, -35), 35) * 1000) / 1000;

    setFacing(direction, true);
    updateLookAt(yaw, pitch);
    return { direction, yaw, pitch };
  }

  /**
   * Đặt vị trí nhân vật theo toạ độ màn hình chuẩn hoá.
   * @param nx 0 = mép trái, 1 = mép phải
   * @param ny 0 = mép trên, 1 = mép dưới — tính theo CHÂN nhân vật
   */
  function setScreenPosition(nx: number, ny: number) {
    avatarScreenX = Math.min(Math.max(nx, 0), 1);
    avatarScreenY = Math.min(Math.max(ny, 0), 1);
    applyScreenPosition();
  }

  function getScreenPosition(): { x: number; y: number } {
    return { x: avatarScreenX, y: avatarScreenY };
  }

  /**
   * Hộp bao của nhân vật trên màn hình, tính bằng px của khung vẽ.
   * Chiếu 8 đỉnh hộp bao world qua camera rồi lấy min/max — cần cho việc
   * đăng ký vùng click (InteractiveZones) đúng bằng thân nhân vật, thay vì
   * để cả canvas toàn màn hình nuốt chuột của desktop.
   * Trả null khi chưa có model hoặc model nằm ngoài khung nhìn.
   */
  function getScreenBounds(): { x: number; y: number; width: number; height: number } | null {
    const root = vrm.value?.scene ?? fbxModel;
    if (!root) return null;

    // setFromObject đọc matrixWorld, mà matrixWorld chỉ được làm mới bên trong
    // renderer.render(). Vòng lặp render lại có thể đang bị chặn — ECO mode,
    // demote 'freeze', hoặc cửa sổ ẩn khiến rAF ngừng. Khi đó zone gửi sang Rust
    // sẽ đứng ở chỗ cũ trong lúc nhân vật đã dời đi. Tự cập nhật để phép đo
    // không phụ thuộc vào việc khung hình có được vẽ hay không.
    avatarRoot.updateMatrixWorld(true);

    const box = new THREE.Box3().setFromObject(root);
    if (!Number.isFinite(box.min.x) || !Number.isFinite(box.max.x)) return null;

    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;

    const corner = new THREE.Vector3();
    for (let i = 0; i < 8; i++) {
      corner.set(
        i & 1 ? box.max.x : box.min.x,
        i & 2 ? box.max.y : box.min.y,
        i & 4 ? box.max.z : box.min.z
      );
      corner.project(camera);
      // Clip space [-1,1] → px màn hình (trục y lật vì clip đi lên, màn hình đi xuống)
      const px = (corner.x * 0.5 + 0.5) * viewportWidth;
      const py = (-corner.y * 0.5 + 0.5) * viewportHeight;
      if (px < minX) minX = px;
      if (px > maxX) maxX = px;
      if (py < minY) minY = py;
      if (py > maxY) maxY = py;
    }

    // Cắt về trong khung vẽ — vùng ngoài màn hình không có ý nghĩa hit-test
    const clampedMinX = Math.max(minX, 0);
    const clampedMinY = Math.max(minY, 0);
    const clampedMaxX = Math.min(maxX, viewportWidth);
    const clampedMaxY = Math.min(maxY, viewportHeight);
    if (clampedMaxX <= clampedMinX || clampedMaxY <= clampedMinY) return null;

    return {
      x: clampedMinX,
      y: clampedMinY,
      width: clampedMaxX - clampedMinX,
      height: clampedMaxY - clampedMinY,
    };
  }

  /**
   * Đổi kích thước khung vẽ. Giữ nguyên vị trí màn hình chuẩn hoá của nhân vật —
   * đổi độ phân giải không được làm nhân vật nhảy chỗ.
   */
  function resize(width: number, height: number) {
    if (width <= 0 || height <= 0) return;
    viewportWidth = width;
    viewportHeight = height;
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    renderer?.setPixelRatio(getRenderPixelRatio(width, height));
    renderer?.setSize(width, height);
    applyScreenPosition();
  }

  // ═══════════════════════════════════════════
  //  Model Loading
  // ═══════════════════════════════════════════
  /**
   * Dispose previous model (VRM or FBX) — prevents VRAM leak on swap.
   */
  function disposePreviousModel() {
    if (vrm.value) {
      deepDispose(vrm.value.scene);
      avatarRoot.remove(vrm.value.scene);
      vrm.value = null;
    }
    if (fbxModel) {
      deepDispose(fbxModel);
      avatarRoot.remove(fbxModel);
      fbxModel = null;
    }
    if (debugProbe) {
      scene.remove(debugProbe);
      debugProbe.geometry.dispose();
      const mat = debugProbe.material;
      if (Array.isArray(mat)) {
        mat.forEach((m) => m.dispose());
      } else {
        mat.dispose();
      }
      debugProbe = null;
    }
    if (mixer) {
      mixer.stopAllAction();
      mixer = null;
    }
    currentModelFormat.value = null;
  }

  /**
   * Auto-scale and center any 3D object using its bounding box.
   * Handles arbitrary FBX scales (0.01, 1, 100) from Blender/Maya/Mixamo.
   */
  function autoScaleAndCenter(object: THREE.Object3D, targetHeight = 2.6) {
    const box = new THREE.Box3().setFromObject(object);
    const size = box.getSize(new THREE.Vector3());

    // Scale to target height so the avatar can better fill the screen
    const maxDim = Math.max(size.x, size.y, size.z);
    if (maxDim > 0) {
      const scale = targetHeight / maxDim;
      object.scale.multiplyScalar(scale);
    }

    // Re-center: move pivot so model stands at origin
    const boxAfter = new THREE.Box3().setFromObject(object);
    const centerAfter = boxAfter.getCenter(new THREE.Vector3());
    object.position.sub(centerAfter);
    // Place feet on ground (Y=0)
    const boxFinal = new THREE.Box3().setFromObject(object);
    object.position.y -= boxFinal.min.y;
  }

  async function loadModel(path: string, onProgress?: (pct: number) => void) {
    // Dispose previous model (VRM or FBX) — critical for memory
    logger.info('[use3DModel]', 'loadModel called', { path });
    disposePreviousModel();

    const ext = path.split('.').pop()?.toLowerCase();
    logger.info('[use3DModel]', 'Resolved model extension', { ext, path });

    if (ext === 'fbx') {
      await loadFBX(path, onProgress);
    } else {
      await loadVRM(path, onProgress);
    }
  }

  /** Load VRM model (original logic preserved) */
  function loadVRM(path: string, onProgress?: (pct: number) => void): Promise<void> {
    const loader = new GLTFLoader();
    loader.register(
      (parser: ConstructorParameters<typeof VRMLoaderPlugin>[0]) => new VRMLoaderPlugin(parser)
    );

    logger.info('[use3DModel]', 'Starting VRM load', { path });

    return new Promise<void>((resolve, reject) => {
      loader.load(
        path,
        (gltf: { userData: { vrm?: VRM }; scene: THREE.Object3D }) => {
          const loadedVRM = gltf.userData.vrm as VRM;
          if (!loadedVRM) {
            const err = new Error('Failed to load VRM from GLTF');
            logger.error('[use3DModel]', 'VRM parse result missing vrm payload', { path, gltf });
            reject(err);
            return;
          }

          VRMUtils.removeUnnecessaryVertices(gltf.scene);
          // U31(b): `combineSkeletons` thay `removeUnnecessaryJoints`.
          //
          // three-vrm 3.5.2 tự in cảnh báo cho hàm cũ: "removeUnnecessaryJoints
          // is deprecated. Use combineSkeletons instead. combineSkeletons
          // contributes more to the performance improvement. This function will
          // be removed in the next major version."
          //
          // Khác biệt thực chất: hàm cũ *tỉa* danh sách joint của TỪNG skeleton,
          // để lại đúng số skeleton như cũ. Hàm mới *gộp* chúng về một skeleton
          // dùng chung. Với Liva.vrm đó là **23 SkinnedMesh** (8 + 12 + 3
          // primitive) — three.js tách mỗi primitive glTF thành một mesh riêng,
          // nên số skeleton phải gánh là 23 chứ không phải 3 như đọc trong file.
          VRMUtils.combineSkeletons(gltf.scene);
          // CỐ Ý KHÔNG gọi `VRMUtils.combineMorphs(loadedVRM)` ở lượt này.
          //
          // Nó là tối ưu **tuỳ chọn** (model có 456 morph target), nhưng nó tái
          // cấu trúc chính đường morph đang dẫn chớp mắt, khẩu hình và biểu
          // cảm — thứ vừa được sửa ở U24. Kiểm chứng nó cần nhìn tận mắt, mà
          // `requestAnimationFrame` bị treo khi khung nhìn ẩn nên phiên này
          // không nhìn được. Khác với `combineSkeletons`: hàm kia là bản thay
          // thế do chính thư viện chỉ định cho một hàm đã deprecated, nên
          // không làm gì mới là lựa chọn tệ hơn.
          VRMUtils.rotateVRM0(loadedVRM);

          if (loadedVRM.lookAt) {
            loadedVRM.lookAt.target = undefined;
          }

          avatarRoot.add(loadedVRM.scene);
          vrm.value = loadedVRM;
          currentModelFormat.value = 'vrm';
          applyScreenPosition();
          logger.info('[use3DModel]', 'VRM model added to scene', {
            path,
            sceneChildren: (scene as unknown as { children: unknown[] }).children.length,
          });

          resolve();
        },
        (event: ProgressEvent<EventTarget>) => {
          if (onProgress && event.total > 0) {
            onProgress(Math.round((event.loaded / event.total) * 100));
          }
        },
        (error: unknown) => {
          logger.error(
            '[use3DModel]',
            'VRM load failed:',
            error instanceof Error ? error.message : String(error)
          );
          reject(error);
        }
      );
    });
  }

  async function loadAnimationClips(paths = DEFAULT_MIXAMO_CLIP_PATHS) {
    const currentVrm = vrm.value;
    if (!currentVrm) return { loaded: [], failures: {} };

    const result = await loadMixamoAnimationSet(currentVrm, paths);
    const loaded = Object.keys(result.clips) as AvatarClipState[];
    for (const state of loaded) {
      const clip = result.clips[state];
      if (clip) animation.registerClip(state, clip);
    }
    return { loaded, failures: result.failures };
  }

  /** Load FBX model with auto-scale/center and optional AnimationMixer */
  function loadFBX(path: string, onProgress?: (pct: number) => void): Promise<void> {
    const loader = new FBXLoader();

    logger.info('[use3DModel]', 'Starting FBX load', { path });

    return new Promise<void>((resolve, reject) => {
      loader.load(
        path,
        (fbx: THREE.Group) => {
          try {
            logger.info('[use3DModel]', 'FBX raw model loaded', {
              path,
              animations: fbx.animations?.length ?? 0,
              children: (fbx as unknown as { children: unknown[] }).children.length,
            });
            // Auto-scale & center (handles 0.01x, 1x, 100x FBX scales)
            autoScaleAndCenter(fbx, 1.9);

            // Rotate FBX to face camera (Tripo3D exports face sideways)
            fbx.rotation.y = -Math.PI / 2;

            // Setup AnimationMixer if FBX has embedded animations
            if (fbx.animations && fbx.animations.length > 0) {
              mixer = new THREE.AnimationMixer(fbx);
              const idleClip = fbx.animations[0];
              const action = mixer.clipAction(idleClip);
              action.play();
              logger.info('[use3DModel]', 'FBX animation mixer started', {
                clips: fbx.animations.length,
              });
            }
            // If no animations, mixer stays null — safe, no crash

            avatarRoot.add(fbx);
            fbxModel = fbx;
            currentModelFormat.value = 'fbx';
            applyScreenPosition();
            logger.info('[use3DModel]', 'FBX model added to scene', {
              path,
              sceneChildren: (scene as unknown as { children: unknown[] }).children.length,
            });

            resolve();
          } catch (e: unknown) {
            logger.error(
              '[use3DModel]',
              'FBX post-process failed:',
              e instanceof Error ? e.message : String(e),
              e
            );
            reject(e);
          }
        },
        (event: ProgressEvent<EventTarget>) => {
          if (onProgress && event.total > 0) {
            onProgress(Math.round((event.loaded / event.total) * 100));
          }
        },
        (error: unknown) => {
          logger.error(
            '[use3DModel]',
            'FBX load failed:',
            error instanceof Error ? error.message : String(error)
          );
          reject(error);
        }
      );
    });
  }

  // ═══════════════════════════════════════════
  //  Render Loop (with procedural idle + adaptive throttle)
  // ═══════════════════════════════════════════
  let isWindowVisible = true;
  let visibilityHandler: (() => void) | null = null;
  let lastFrameTime = 0;

  function setFrameUpdate(callback: ((delta: number) => void) | null) {
    frameUpdate = callback;
  }

  function startRenderLoop() {
    if (animFrameId !== null) return;

    // Adaptive throttle: reduce FPS when window hidden
    if (typeof document !== 'undefined') {
      // [Audit C-1] Store handler ref for cleanup in dispose()
      visibilityHandler = () => {
        isWindowVisible = !document.hidden;
      };
      document.addEventListener('visibilitychange', visibilityHandler);
    }

    function animate(now: number) {
      animFrameId = requestAnimationFrame(animate);

      // Adaptive throttle: ~15fps when hidden (66ms interval), >=30fps in ECO Mode
      // [Phase 3] Avatar freeze: 0fps when VRAM demote level is 'freeze' or 'preempted'
      const demoteLevel = (globalThis as unknown as Record<string, unknown>)
        .LIVA_AVATAR_DEMOTE_LEVEL as string | undefined;
      if (demoteLevel === 'freeze' || demoteLevel === 'preempted') return; // Skip frame entirely
      const isEcoMode = (globalThis as unknown as Record<string, unknown>).LIVA_ECO_MODE === true;
      // U31(d): ECO từng hạ xuống 200 ms — tức **5 FPS** — trên một nhân vật
      // NGƯỜI DÙNG ĐANG NHÌN. 5 FPS thì chắc chắn giật khi cô ấy còn di chuyển,
      // và ECO tồn tại để sống chung với workload nặng, tức đúng lúc người dùng
      // vẫn đang nhìn. Sàn nay là 30 FPS.
      //
      // Muốn tiết kiệm thêm thì hạ những thứ người dùng khó nhận ra — DPR,
      // antialias, tần suất spring bone — chứ đừng hạ frame rate. Xem mục U31
      // trong docs/03-danh-gia/05-nang-cap-toan-dien.md.
      //
      // Cửa sổ ẩn vẫn 66 ms: lúc đó không ai nhìn, và trình duyệt còn treo hẳn
      // requestAnimationFrame nên con số này phần lớn là lý thuyết.
      const throttleInterval = isEcoMode ? ECO_FRAME_INTERVAL_MS : !isWindowVisible ? 66 : 0;
      if (throttleInterval > 0 && now - lastFrameTime < throttleInterval) return;
      lastFrameTime = now;

      // Position, pose and mixer share this clock. Only cap very large wake-up jumps;
      // spring physics is sub-stepped separately below.
      const rawDelta = clock.getDelta();
      const delta = Math.min(rawDelta, 0.1);

      frameUpdate?.(delta);

      // Quay người mượt về hướng đang đi
      if (facingCurrent !== facingTarget) {
        const turnFactor = 1 - Math.pow(0.005, delta);
        facingCurrent = lerp(facingCurrent, facingTarget, turnFactor);
        if (Math.abs(facingCurrent - facingTarget) < 0.001) facingCurrent = facingTarget;
        avatarRoot.rotation.y = facingCurrent;
      }

      if (vrm.value) {
        // Tư thế thân thể — phải chạy TRƯỚC vrm.update() để spring bone (tóc,
        // váy) phản ứng với chuyển động của khung xương trong chính khung hình
        // này, thay vì trễ một nhịp.
        animation.update(vrm.value, delta);

        // Procedural idle animation (VRM only)
        updateIdle(delta);

        // Organic auto-blink (only when face tracking is OFF)
        if (!faceTrackingActive) {
          updateBlink(delta);
        }

        // Lip-sync — audio-driven takes priority over procedural fallback
        if (audioAnalyserActive) {
          updateAudioLipSync();
        } else if (lipSyncActive) {
          updateProceduralLipSync(delta);
        }

        // Spring-damped lookAt
        updateSpringLookAt(delta);

        // Micro-expressions
        updateMicroExpressions(delta);

        // Keep spring physics stable without slowing the pose/locomotion timeline.
        const physicsSteps = Math.max(1, Math.ceil(delta / (1 / 60)));
        const physicsDelta = delta / physicsSteps;
        for (let step = 0; step < physicsSteps; step += 1) {
          vrm.value.update(physicsDelta);
        }
      }

      // FBX AnimationMixer update (runs independently of VRM)
      if (mixer) {
        mixer.update(delta);
      }

      if (renderer) {
        if (debugProbe) {
          debugProbe.rotation.x += delta * 0.8;
          debugProbe.rotation.y += delta * 1.2;
        }
        renderer.render(scene, camera);
      }
    }
    animate(performance.now());
  }

  function stopRenderLoop() {
    if (animFrameId !== null) {
      cancelAnimationFrame(animFrameId);
      animFrameId = null;
    }
  }

  // ═══════════════════════════════════════════
  //  Procedural Idle — Breathing + Micro-Sway
  // ═══════════════════════════════════════════
  function updateIdle(delta: number) {
    if (!vrm.value) return;
    idleTime += delta;

    // 1. Breathing — subtle spine/chest oscillation
    const spine = vrm.value.humanoid?.getNormalizedBoneNode('spine');
    if (spine) {
      // Slow sine wave: 4-second cycle (15 breaths/min, natural resting rate)
      const breathCycle = Math.sin(idleTime * Math.PI * 0.5) * 0.008;
      spine.rotation.x = breathCycle;
    }

    // 2. OpenSimplex head micro-sway (natural, never repeats)
    if (!faceTrackingActive) {
      const head = vrm.value.humanoid?.getNormalizedBoneNode('head');
      if (head) {
        // 2D simplex noise at different time scales for organic motion
        const swayX =
          simplex2D(idleTime * 0.15, 0) * 0.005 + simplex2D(idleTime * 0.4, 1.7) * 0.002;
        const swayY =
          simplex2D(0, idleTime * 0.12) * 0.004 + simplex2D(2.3, idleTime * 0.35) * 0.002;
        head.rotation.x = swayX;
        head.rotation.y = swayY;
      }
    }
  }

  // ═══════════════════════════════════════════
  //  Organic Auto-Blink (easeOutQuad curve)
  // ═══════════════════════════════════════════
  function startAutoBlink() {
    // Reset blink state — the actual blinking runs inside render loop
    blinkTimer = 0;
    nextBlinkAt = randomBlinkInterval();
    blinkPhase = 'idle';
    isBlinking = false;
  }

  function updateBlink(delta: number) {
    if (!vrm.value?.expressionManager) return;
    const em = vrm.value.expressionManager;

    blinkTimer += delta;

    switch (blinkPhase) {
      case 'idle':
        if (blinkTimer >= nextBlinkAt) {
          blinkPhase = 'closing';
          blinkProgress = 0;
          isBlinking = true;
          // 20% chance of double-blink
          pendingDoubleBlink = Math.random() < 0.2;
          // NOSONAR
        }
        break;

      case 'closing':
        // Close in ~60ms (easeOutQuad for natural speed)
        blinkProgress += delta / 0.06;
        if (blinkProgress >= 1) {
          blinkProgress = 1;
          blinkPhase = 'closed';
        }
        em.setValue('blink', easeOutQuad(blinkProgress));
        break;

      case 'closed':
        // Stay closed for 30-60ms (natural closed duration)
        blinkProgress += delta / (0.03 + Math.random() * 0.03);
        // NOSONAR
        if (blinkProgress >= 2) {
          blinkPhase = 'opening';
          blinkProgress = 0;
        }
        em.setValue('blink', 1);
        break;

      case 'opening':
        // Open in ~100ms (slower than close — asymmetric = natural)
        blinkProgress += delta / 0.1;
        if (blinkProgress >= 1) {
          blinkProgress = 0;
          em.setValue('blink', 0);
          isBlinking = false;

          if (pendingDoubleBlink) {
            // Double-blink: blink again after tiny pause
            pendingDoubleBlink = false;
            blinkPhase = 'closing';
            blinkTimer = nextBlinkAt - 0.15; // Re-trigger in ~150ms
          } else {
            blinkPhase = 'idle';
            blinkTimer = 0;
            nextBlinkAt = randomBlinkInterval();
          }
          return;
        }
        em.setValue('blink', 1 - easeOutQuad(blinkProgress));
        break;
    }
  }

  // ═══════════════════════════════════════════
  //  Procedural Lip-Sync (Multi-Vowel + Noise)
  //  Fallback when no audio source is available
  // ═══════════════════════════════════════════
  function startLipSync() {
    if (lipSyncActive) return;
    lipSyncActive = true;
    lipTime = 0;
  }

  function updateProceduralLipSync(delta: number) {
    if (!vrm.value?.expressionManager || !lipSyncActive) return;
    const em = vrm.value.expressionManager;
    lipTime += delta;

    // Generate organic mouth movement via layered oscillation
    // This simulates natural speech patterns (NOT random rectangles)
    const speed = 8; // ~8 syllables/second

    // Primary jaw movement
    const jaw = Math.max(
      0,
      Math.sin(lipTime * speed) * 0.5 +
        Math.sin(lipTime * speed * 1.7 + 0.5) * 0.25 +
        Math.sin(lipTime * speed * 0.6 + 1.2) * 0.15
    );

    // Cycle through vowel shapes
    const vowelPhase = (lipTime * speed * 0.5) % 4;

    // 'aa' — open mouth (primary talking shape)
    em.setValue('aa', jaw * 0.8);

    // 'ih' — slight smile shape (secondary)
    em.setValue('ih', vowelPhase > 1 && vowelPhase < 2 ? jaw * 0.3 : 0);

    // 'ou' — rounded lips
    em.setValue('ou', vowelPhase > 2 && vowelPhase < 3 ? jaw * 0.4 : 0);

    // 'ee' — wide mouth
    em.setValue('ee', vowelPhase > 3 ? jaw * 0.3 : 0);
  }

  function stopLipSync() {
    lipSyncActive = false;
    lipTime = 0;
    if (lipSyncRAF !== null) {
      cancelAnimationFrame(lipSyncRAF);
      lipSyncRAF = null;
    }
    if (!vrm.value?.expressionManager) return;
    const em = vrm.value.expressionManager;
    // Smooth close (don't snap to 0)
    em.setValue('aa', 0);
    em.setValue('ih', 0);
    em.setValue('ou', 0);
    em.setValue('ee', 0);
  }

  // ═══════════════════════════════════════════
  //  Audio-Driven Lip-Sync (RMS Viseme Mapping)
  //  Real-time frequency analysis → VRM mouth expressions
  // ═══════════════════════════════════════════

  /**
   * Band layout for 128 frequency bins (fftSize=256, sampleRate≈44.1kHz):
   *   Band 0 (bins 0-3):   Sub-bass  → 'aa' (jaw open, speech fundamental 80-300Hz)
   *   Band 1 (bins 4-8):   Low-mid   → 'oh' (vowel formant F1, 300-800Hz)
   *   Band 2 (bins 9-16):  Mid       → 'ee' (vowel formant F2, 800-2kHz)
   *   Band 3 (bins 17-32): Upper-mid → 'ih' (consonant energy, 2-4kHz)
   *   Band 4 (bins 33-64): High      → 'ou' (sibilance, 4-8kHz)
   */
  const BAND_RANGES: ReadonlyArray<readonly [number, number]> = [
    [0, 3], // Band 0: sub-bass → aa
    [4, 8], // Band 1: low-mid → oh
    [9, 16], // Band 2: mid → ee
    [17, 32], // Band 3: upper-mid → ih
    [33, 64], // Band 4: high → ou
  ] as const;

  /** Sensitivity scaling per band — lower frequencies need more amplification */
  const BAND_SENSITIVITY: ReadonlyArray<number> = [1.2, 0.8, 0.6, 0.5, 0.4];
  /** VRM expression names mapped to each band */
  const BAND_EXPRESSIONS: ReadonlyArray<string> = ['aa', 'oh', 'ee', 'ih', 'ou'];
  /** Dead zone threshold — below this, treat as silence to prevent jitter */
  const RMS_DEAD_ZONE = 0.05;
  /** Smoothing factor for lerp (0=no change, 1=instant snap) */
  const RMS_SMOOTH_FACTOR = 0.3;

  /**
   * Start real-time audio-driven lip-sync by reading an analyser that the
   * playback composable already keeps wired into its output chain
   * (`source → analyser → masterGain → destination`).
   *
   * This function does NOT build or connect any audio node. It used to create
   * its own analyser and hang it off the source in parallel, which sent the
   * same buffer to the destination twice and let ducked audio keep playing at
   * full volume through the second route. It also rebuilt the analyser on every
   * scheduled chunk — and chunks are scheduled ahead of playback, so the
   * analyser spent most of each utterance reading a source that had not started
   * yet, closing the mouth mid-sentence.
   *
   * The render loop picks up frequency data each frame via updateAudioLipSync().
   */
  function startAudioDrivenLipSync(analyser: AnalyserNode) {
    // Idempotent: re-arming on the same analyser mid-run must not reset the
    // smoothing state, or the mouth twitches at every re-arm.
    if (audioAnalyserActive && audioAnalyserNode === analyser) return;

    // BAND_RANGES above indexes bins of THIS fftSize — the two move together.
    analyser.fftSize = 256; // 128 frequency bins
    analyser.smoothingTimeConstant = 0.4; // Moderate temporal smoothing in the analyser itself

    audioAnalyserNode = analyser;
    audioFreqData = new Uint8Array(analyser.frequencyBinCount); // 128 bins
    audioAnalyserActive = true;

    // Zero out smoothed values for fresh start
    smoothedBandRMS.fill(0);

    logger.info('[use3DModel]', 'Audio-driven lip-sync started', {
      fftSize: analyser.fftSize,
      binCount: analyser.frequencyBinCount,
    });
  }

  /**
   * Per-frame audio lip-sync update — called from the render loop.
   * Reads frequency data, computes RMS per band, maps to VRM expressions.
   *
   * VC-8: khi có timeline phoneme đang hiệu lực (`OP_VISME` từ core) thì RMS
   * chỉ giữ vai trò BIÊN ĐỘ — HÌNH miệng do phoneme quyết: nhóm môi (m/b/p/f/v)
   * ép gần khép dù âm vẫn to (chỗ RMS không bao giờ phân biệt được), nguyên âm
   * tăng biểu cảm tương ứng và hạ các cái còn lại. Không có timeline ⇒ hành vi
   * RMS thuần cũ giữ nguyên.
   */
  function updateAudioLipSync() {
    if (!audioAnalyserNode || !audioFreqData || !vrm.value?.expressionManager) return;
    const em = vrm.value.expressionManager;

    // Read current frequency spectrum
    audioAnalyserNode.getByteFrequencyData(audioFreqData as unknown as Uint8Array<ArrayBuffer>);

    const viseme = currentVisemeFromClock();
    const values: number[] = new Array(BAND_EXPRESSIONS.length).fill(0);

    for (let band = 0; band < BAND_RANGES.length; band++) {
      const [startBin, endBin] = BAND_RANGES[band];
      const count = endBin - startBin + 1;

      // Compute RMS for this band: sqrt(sum(bin²) / count) / 255 → [0.0, 1.0]
      let sumSq = 0;
      for (let bin = startBin; bin <= endBin; bin++) {
        const normalized = audioFreqData[bin] / 255;
        sumSq += normalized * normalized;
      }
      let rms = Math.sqrt(sumSq / count);

      // Dead zone: suppress jitter during silence
      if (rms < RMS_DEAD_ZONE) rms = 0;

      // Smooth: lerp from previous value for natural movement
      smoothedBandRMS[band] = lerp(smoothedBandRMS[band], rms, RMS_SMOOTH_FACTOR);

      // Map to VRM expression with sensitivity scaling, clamped to [0, 1]
      values[band] = Math.min(smoothedBandRMS[band] * BAND_SENSITIVITY[band], 1.0);
    }

    if (viseme !== null) {
      const matchedBand = BAND_EXPRESSIONS.indexOf(viseme);
      for (let band = 0; band < values.length; band++) {
        if (viseme === 'nil') {
          // Âm môi/không phát — miệng gần khép bất kể năng lượng.
          values[band] *= 0.12;
        } else if (band === matchedBand) {
          values[band] = Math.min(values[band] * 1.35 + 0.25, 1.0);
        } else {
          values[band] *= 0.55;
        }
      }
    }

    for (let band = 0; band < BAND_EXPRESSIONS.length; band++) {
      em.setValue(BAND_EXPRESSIONS[band], values[band]);
    }
  }

  /**
   * Stop audio-driven lip-sync and zero all mouth expressions.
   *
   * Deliberately does NOT disconnect the analyser: it belongs to the playback
   * composable and carries the audio through to the speakers, so disconnecting
   * it here would cut playback entirely. We only drop our reference to it.
   */
  function stopAudioDrivenLipSync() {
    audioAnalyserNode = null;
    audioFreqData = null;
    audioAnalyserActive = false;
    smoothedBandRMS.fill(0);

    // Zero all mouth expressions
    if (vrm.value?.expressionManager) {
      const em = vrm.value.expressionManager;
      for (const expr of BAND_EXPRESSIONS) {
        em.setValue(expr, 0);
      }
    }
  }

  // ═══════════════════════════════════════════
  //  Micro-Expressions (Idle Personality)
  // ═══════════════════════════════════════════
  function updateMicroExpressions(delta: number) {
    if (!vrm.value?.expressionManager || faceTrackingActive || lipSyncActive) return;
    const em = vrm.value.expressionManager;

    microExprTimer += delta;

    if (!activeMicroExpr) {
      // Schedule next micro-expression
      if (microExprTimer >= nextMicroExprAt) {
        const options = ['happy', 'relaxed', 'surprised'];
        const weights = [0.5, 0.35, 0.15]; // Happy most common
        activeMicroExpr = weightedRandom(options, weights);
        microExprIntensity = 0;
        microExprFading = false;
        microExprTimer = 0;
      }
    } else {
      // Animate the micro-expression
      if (!microExprFading) {
        // Ramp up over ~400ms
        microExprIntensity += delta / 0.4;
        const targetIntensity = 0.2 + Math.random() * 0.3; // 0.2-0.5 (subtle)
        // NOSONAR
        if (microExprIntensity >= targetIntensity) {
          microExprIntensity = targetIntensity;
          microExprFading = true;
          microExprTimer = 0;
        }
        em.setValue(activeMicroExpr, easeOutQuad(microExprIntensity));
      } else {
        // Hold for 0.5-1.5s then fade out
        if (microExprTimer < 0.5 + Math.random()) {
          // NOSONAR
          em.setValue(activeMicroExpr, microExprIntensity);
        } else {
          // Fade out over ~600ms
          microExprIntensity -= delta / 0.6;
          if (microExprIntensity <= 0) {
            em.setValue(activeMicroExpr, 0);
            activeMicroExpr = null;
            microExprTimer = 0;
            nextMicroExprAt = 5 + Math.random() * 10; // 5-15s until next
            // NOSONAR
          } else {
            em.setValue(activeMicroExpr, easeOutQuad(microExprIntensity));
          }
        }
      }
    }
  }

  // ═══════════════════════════════════════════
  //  Trigger Motion (Smooth Ramp, not Flash)
  // ═══════════════════════════════════════════
  function triggerMotion() {
    if (!vrm.value?.expressionManager) return;

    // Cancel any existing expression animation to prevent accumulation
    if (expressionRAF !== null) {
      cancelAnimationFrame(expressionRAF);
      expressionRAF = null;
    }

    const em = vrm.value.expressionManager;

    // Pick expression with weighted randomness
    const options = ['happy', 'surprised', 'relaxed'];
    const weights = [0.45, 0.3, 0.25];
    const expr = weightedRandom(options, weights);
    const peakIntensity = 0.4 + Math.random() * 0.4; // 0.4-0.8
    // NOSONAR

    // Smooth ramp up (300ms) → hold (200-500ms) → ramp down (500ms)
    const rampUpMs = 300;
    const holdMs = 200 + Math.random() * 300;
    // NOSONAR
    const rampDownMs = 500;

    let elapsed = 0;
    const startTime = performance.now();

    function animateExpression() {
      elapsed = performance.now() - startTime;

      if (elapsed < rampUpMs) {
        // Ramp up with ease-out
        const t = elapsed / rampUpMs;
        em.setValue(expr, easeOutQuad(t) * peakIntensity);
      } else if (elapsed < rampUpMs + holdMs) {
        // Hold at peak with slight oscillation
        const holdT = (elapsed - rampUpMs) / holdMs;
        const wobble = Math.sin(holdT * Math.PI * 2) * 0.05;
        em.setValue(expr, peakIntensity + wobble);
      } else if (elapsed < rampUpMs + holdMs + rampDownMs) {
        // Ramp down with ease-in
        const t = (elapsed - rampUpMs - holdMs) / rampDownMs;
        em.setValue(expr, (1 - easeInQuad(t)) * peakIntensity);
      } else {
        // Done
        em.setValue(expr, 0);
        expressionRAF = null;
        return;
      }

      expressionRAF = requestAnimationFrame(animateExpression);
    }

    expressionRAF = requestAnimationFrame(animateExpression);
  }

  // ═══════════════════════════════════════════
  //  Face Tracking — LookAt + Expressions
  // ═══════════════════════════════════════════

  /**
   * Drive VRM model eyes/head to follow user's face.
   * Uses spring-damped lerp for smooth, natural tracking.
   * @param yaw — horizontal angle in degrees (-45 to +45)
   * @param pitch — vertical angle in degrees (-35 to +35)
   */
  function updateLookAt(yaw: number, pitch: number) {
    // Set target — actual movement happens in updateSpringLookAt()
    targetYaw = yaw;
    targetPitch = pitch;
  }

  /**
   * Spring-damped LookAt update — called every frame.
   * Prevents robotic snap-to-target by exponentially decaying toward target.
   */
  function updateSpringLookAt(delta: number) {
    if (!vrm.value?.lookAt) return;

    // Exponential decay spring: 90% toward target per 100ms
    // This creates a smooth, natural "drag" feeling
    const springFactor = 1 - Math.pow(0.001, delta); // ~0.1-0.15 per frame at 60fps

    currentYaw = lerp(currentYaw, targetYaw, springFactor);
    currentPitch = lerp(currentPitch, targetPitch, springFactor);

    const la = vrm.value.lookAt;
    if (la.applier) {
      la.applier.applyYawPitch(currentYaw, currentPitch);
    }
  }

  /**
   * Map face tracking blendshapes → VRM expressions.
   * Provides real-time facial mirroring (user smiles → model smiles).
   */
  function updateExpressions(expressions: FaceExpressions) {
    if (!vrm.value?.expressionManager) return;
    const em = vrm.value.expressionManager;

    // Map face expressions → VRM expression names
    // Smooth factor prevents jittery transitions
    const smooth = 0.3;

    // Happy → VRM 'happy'
    if (expressions.happy > 0.15) {
      const current = em.getValue('happy') ?? 0;
      em.setValue('happy', lerp(current, expressions.happy, smooth));
    } else {
      em.setValue('happy', lerp(em.getValue('happy') ?? 0, 0, smooth));
    }

    // Surprised → VRM 'surprised'
    if (expressions.surprised > 0.2) {
      const current = em.getValue('surprised') ?? 0;
      em.setValue('surprised', lerp(current, expressions.surprised, smooth));
    } else {
      em.setValue('surprised', lerp(em.getValue('surprised') ?? 0, 0, smooth));
    }

    // Angry → VRM 'angry'
    if (expressions.angry > 0.2) {
      const current = em.getValue('angry') ?? 0;
      em.setValue('angry', lerp(current, expressions.angry, smooth));
    } else {
      em.setValue('angry', lerp(em.getValue('angry') ?? 0, 0, smooth));
    }

    // Blink — override auto-blink when face tracking is active
    if (faceTrackingActive) {
      em.setValue('blink', expressions.blink);
    }

    // Mouth open (for talking detection)
    if (expressions.mouthOpen > 0.1) {
      em.setValue('aa', expressions.mouthOpen * 0.8);
    } else {
      em.setValue('aa', lerp(em.getValue('aa') ?? 0, 0, smooth));
    }
  }

  /**
   * Toggle face tracking mode.
   * When active: disables auto-blink (real blinks take over).
   * When inactive: re-enables auto-blink.
   */
  function setFaceTrackingActive(active: boolean) {
    faceTrackingActive = active;
    if (active) {
      // Disable auto-blink — real blinks from camera
      blinkPhase = 'idle';
      isBlinking = false;
    } else {
      // Re-enable auto-blink
      startAutoBlink();
      // Smoothly reset face-driven expressions (don't snap to 0)
      if (vrm.value?.expressionManager) {
        const em = vrm.value.expressionManager;
        em.setValue('happy', 0);
        em.setValue('surprised', 0);
        em.setValue('angry', 0);
        em.setValue('aa', 0);
      }
      // Reset lookAt targets
      targetYaw = 0;
      targetPitch = 0;
    }
  }

  // ═══════════════════════════════════════════
  //  Dispose — Full VRAM Cleanup
  // ═══════════════════════════════════════════
  function dispose() {
    stopRenderLoop();
    stopLipSync();
    stopAudioDrivenLipSync();
    animation.reset();
    frameUpdate = null;
    facingTarget = 0;
    facingCurrent = 0;
    faceTrackingActive = false;
    activeMicroExpr = null;
    if (isBlinking) {
      isBlinking = false;
      blinkPhase = 'idle';
    }
    if (expressionRAF !== null) {
      cancelAnimationFrame(expressionRAF);
      expressionRAF = null;
    }

    // Dispose all models (VRM + FBX)
    disposePreviousModel();

    if (renderer) {
      renderer.dispose();
      renderer.forceContextLoss();
      renderer = null;
    }
    // [Audit C-1] Clean up visibilitychange listener to prevent leak
    if (visibilityHandler && typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', visibilityHandler);
      visibilityHandler = null;
    }
  }

  function setLocomotionState(state: LocomotionState, motionWeight = state === 'idle' ? 0 : 1) {
    animation.setMotionWeight(motionWeight);
    animation.setState(state);
  }

  return {
    vrm,
    currentModelFormat,
    scene,
    camera,
    renderer,
    loadModel,
    loadAnimationClips,
    hasAnimationClip: animation.hasClip,
    initRenderer,
    resize,
    setScreenPosition,
    setScale,
    setFacing,
    setLocomotionState,
    playGesture: animation.playGesture,
    setInspecting: animation.setInspecting,
    setThinking: animation.setThinking,
    lookAtScreenPoint,
    getScreenPosition,
    getScreenBounds,
    setFrameUpdate,
    startRenderLoop,
    stopRenderLoop,
    startAutoBlink,
    startLipSync,
    stopLipSync,
    startAudioDrivenLipSync,
    stopAudioDrivenLipSync,
    triggerMotion,
    updateLookAt,
    updateExpressions,
    setFaceTrackingActive,
    dispose,
  };
}

// Năm hàm thuần (lerp/easeOutQuad/easeInQuad/randomBlinkInterval/weightedRandom)
// nay ở `utils/avatarMath.ts` — xem import ở đầu file. Trước 06/08/2026 chúng có
// một bản sao giống hệt trong một composable mồ côi, và bộ test kiểm bản sao đó
// chứ không kiểm bản này. Chi tiết: mục U25 trong docs/03-danh-gia/05.
