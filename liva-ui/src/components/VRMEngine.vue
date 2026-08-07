<script setup lang="ts">
/**
 * VRMEngine.vue — 3D Avatar Engine (Three.js + @pixiv/three-vrm)
 * ================================================================
 * Used on machines with discrete GPUs (NVIDIA/AMD/Apple M).
 * Lazy-loaded via defineAsyncComponent — 0 bytes when not used.
 *
 * Features:
 * - Transparent WebGL background (alpha: true)
 * - MToon shader lighting (HemisphereLight + DirectionalLight)
 * - Auto-blink, lip-sync, idle breathing
 * - Deep Dispose (VRAM cleanup on unmount/swap)
 * - Face Tracking: webcam → MediaPipe → VRM lookAt + expressions
 */
import { ref, onMounted, onUnmounted, watch, onActivated, onDeactivated } from 'vue';
import { use3DModel } from '../composables/use3DModel';
import { useAvatarLocomotion } from '../composables/useAvatarLocomotion';
import { useFaceTracking } from '../composables/useFaceTracking';
import { logger } from '../utils/logger';

const canvas = ref<HTMLCanvasElement | null>(null);
const webcamVideo = ref<HTMLVideoElement | null>(null);
const isLoaded = ref(false);
const loadError = ref<string | null>(null);
const isCameraOn = ref(false);

/** Config model avatar — các key thay thế nhau tuỳ nguồn cấu hình (widget, settings, default) */
interface ModelConfig {
  filename?: string;
  vrmModel?: string;
  path?: string;
  mainModel?: string;
}

const props = defineProps<{
  modelConfig?: ModelConfig;
  fullScreen?: boolean;
  /** Vị trí nhân vật trên màn hình, chuẩn hoá [0,1]; y tính theo chân. */
  screenPos?: { x: number; y: number };
  /** Cỡ nhân vật so với khung nhìn; 1.0 = cao ~64% chiều cao màn hình. */
  avatarScale?: number;
}>();

// Global Mouse LookAt (eyes follow cursor across screen)
// Runs when face tracking is OFF (face tracking takes priority)
// Uses Web pointer events instead of Electron IPC

const {
  currentModelFormat,
  initRenderer,
  resize,
  setScreenPosition,
  setScale,
  setFacing,
  lookAtScreenPoint,
  setInspecting,
  setThinking,
  setLocomotionState,
  playGesture,
  getScreenBounds,
  loadModel,
  loadAnimationClips,
  setFrameUpdate,
  startRenderLoop,
  startAutoBlink,
  startLipSync,
  stopLipSync,
  startAudioDrivenLipSync,
  stopAudioDrivenLipSync,
  triggerMotion,
  updateLookAt,
  updateExpressions,
  setFaceTrackingActive,
  dispose: disposeVRM,
} = use3DModel();

const { faceData, isTracking, startTracking, stopTracking, captureFrame } = useFaceTracking();

// ═══════════════════════════════════════════════════════
//  Audio-Driven Lip-Sync (Real-time AnalyserNode)
// ═══════════════════════════════════════════════════════

/**
 * Start real-time audio-driven lip-sync.
 * Reads the analyser that the playback composable keeps in its output chain,
 * for per-frame RMS viseme mapping inside the render loop. The engine never
 * touches the audio graph itself — see startAudioDrivenLipSync().
 */
function startAudioLipSync(analyser: AnalyserNode) {
  startAudioDrivenLipSync(analyser);
}

function stopAudioLipSync() {
  stopAudioDrivenLipSync();
}

// ═══════════════════════════════════════════════════════
//  Global Mouse LookAt (Web Pointer Events)
// ═══════════════════════════════════════════════════════
let mouseNormX = 0;
let mouseNormY = 0;

function onPointerMove(e: PointerEvent) {
  // Normalize to -1..1 range across the full viewport
  mouseNormX = (e.clientX / window.innerWidth) * 2 - 1;
  mouseNormY = (e.clientY / window.innerHeight) * 2 - 1;
}

let mouseLookAtInterval: ReturnType<typeof setInterval> | null = null;

function startMouseLookAt() {
  if (mouseLookAtInterval) return;
  window.addEventListener('pointermove', onPointerMove, { passive: true });
  // Poll every 100ms (10fps is enough for smooth eye tracking)
  mouseLookAtInterval = setInterval(() => {
    if (isCameraOn.value) return; // Face tracking takes priority
    // Map normalized -1..1 → VRM yaw/pitch degrees
    const yaw = mouseNormX * 25; // ±25° max
    const pitch = -mouseNormY * 15; // ±15° max (invert Y: cursor up = look up)
    updateLookAt(yaw, pitch);
  }, 100);
}

function stopMouseLookAt() {
  window.removeEventListener('pointermove', onPointerMove);
  if (mouseLookAtInterval) {
    clearInterval(mouseLookAtInterval);
    mouseLookAtInterval = null;
  }
}

// ═══════════════════════════════════════════════════════
//  LLM Emotion Tag → VRM Expression
//  Backend sends [happy], [sad], etc. in AI stream
// ═══════════════════════════════════════════════════════
let currentEmotion: string | null = null;
let emotionDecayTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * Set avatar expression from LLM emotion tag.
 * Auto-decays back to neutral after 4 seconds.
 */
function setExpression(emotion: string) {
  // Clear previous emotion
  if (currentEmotion) {
    triggerMotion(); // Use smooth triggerMotion for crossfade
  }

  const validEmotions = ['happy', 'sad', 'angry', 'surprised', 'neutral', 'relaxed'];
  if (!validEmotions.includes(emotion)) return;

  currentEmotion = emotion;

  // Neutral = reset all
  if (emotion === 'neutral') return;

  // Trigger the emotion (triggerMotion handles smooth ramp)
  triggerMotion();

  // Auto-decay after 4s
  if (emotionDecayTimer) clearTimeout(emotionDecayTimer);
  emotionDecayTimer = setTimeout(() => {
    currentEmotion = null;
  }, 4000);
}

// ═══════════════════════════════════════════════════════
//  Face Tracking → VRM linkage (watch reactive faceData)
// ═══════════════════════════════════════════════════════
let trackingRAF: number | null = null;

function faceTrackingLoop() {
  if (!isTracking.value) return;
  trackingRAF = requestAnimationFrame(faceTrackingLoop);

  const data = faceData.value;
  if (!data.isDetected) return;

  // Drive VRM lookAt (mirror: negate yaw so model looks at user)
  updateLookAt(-data.head.yaw, data.head.pitch);

  // Drive VRM expressions
  updateExpressions(data.expressions);
}

// ═══════════════════════════════════════════════════════
//  Camera Toggle
// ═══════════════════════════════════════════════════════
async function toggleCamera() {
  if (isCameraOn.value) {
    // Turn OFF
    stopTracking();
    setFaceTrackingActive(false);
    isCameraOn.value = false;
    if (trackingRAF !== null) {
      cancelAnimationFrame(trackingRAF);
      trackingRAF = null;
    }
  } else {
    // Turn ON
    if (!webcamVideo.value) return;
    await startTracking(webcamVideo.value);
    setFaceTrackingActive(true);
    isCameraOn.value = true;
    faceTrackingLoop();
  }
}

// ═══════════════════════════════════════════════════════
//  Frame Capture for AI Vision (public)
// ═══════════════════════════════════════════════════════
function captureFrameForAI(): string | null {
  return captureFrame();
}

const toFileUrl = (rawPath: string) => {
  const normalized = rawPath.replace(/\\/g, '/');
  if (/^file:\/\//i.test(normalized)) return normalized;
  if (/^[a-zA-Z]:\//.test(normalized)) {
    return `file:///${normalized}`;
  }
  return normalized.startsWith('/') ? `file://${normalized}` : `file:///${normalized}`;
};

const resolveModelPath = (config: ModelConfig | undefined) => {
  const raw = config?.filename ?? config?.vrmModel ?? config?.path ?? config?.mainModel;

  if (!raw) {
    return { path: null, reason: 'missing model config' };
  }

  if (/^https?:\/\//i.test(raw) || /^file:\/\//i.test(raw)) {
    return { path: raw, reason: 'absolute/url' };
  }

  if (/^[a-zA-Z]:[\\/]/.test(raw)) {
    return { path: toFileUrl(raw), reason: 'windows absolute path' };
  }

  if (raw.startsWith('/')) {
    return { path: raw, reason: 'absolute path' };
  }

  if (raw.startsWith('models/')) {
    return { path: `/${raw}`, reason: 'public asset path' };
  }

  if (raw.includes('/')) {
    return { path: `/${raw}`, reason: 'relative folder path' };
  }

  const path = `/models/vrm/${raw}`;
  return { path, reason: 'config filename' };
};

const loadSelectedModel = async (config: ModelConfig | undefined) => {
  const resolved = resolveModelPath(config);
  const modelPath = resolved.path;

  try {
    logger.info('[VRMEngine]', 'Loading model', {
      source: resolved.reason,
      modelPath,
      config,
    });
    if (!modelPath) {
      throw new Error('No model path provided');
    }
    await loadModel(modelPath);
    if (currentModelFormat.value === 'vrm') {
      const animations = await loadAnimationClips();
      logger.info('[VRMEngine]', 'Mixamo animation set resolved', {
        loaded: animations.loaded,
        missing: Object.keys(animations.failures),
      });
    }
    isLoaded.value = true;
    loadError.value = null;
    logger.info('[VRMEngine]', 'Model loaded successfully', {
      modelPath,
      currentModelFormat: currentModelFormat.value,
    });
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    logger.warn('[VRMEngine]', `Model "${modelPath}" load failed: ${errMsg}`, e);

    loadError.value = `Model load failed: ${errMsg}`;

    isLoaded.value = true;
  }
};

watch(
  () => props.modelConfig,
  async (newConfig: ModelConfig | undefined) => {
    if (newConfig) {
      await loadSelectedModel(newConfig);
    }
  },
  { deep: true }
);

// ═══════════════════════════════════════════════════════
//  Kích thước khung vẽ + vị trí nhân vật
// ═══════════════════════════════════════════════════════

/** Khung vẽ: toàn màn hình theo mặc định, 400×700 khi bị ghim vào góc. */
function currentCanvasSize(): { width: number; height: number } {
  if (props.fullScreen === false) return { width: 400, height: 700 };
  // Cửa sổ có thể chưa có kích thước tại thời điểm mount — Tauri tạo cửa sổ rồi
  // mới hiện, và trình duyệt trả innerWidth = 0 khi khung chưa được dựng. Khi đó
  // canvas ra 0×0: avatar biến mất và getScreenBounds() trả null, nên vùng bắt
  // chuột không bao giờ được đăng ký. Rơi về kích thước container rồi mới tới
  // giá trị mặc định.
  const width = window.innerWidth || canvas.value?.parentElement?.clientWidth || 1280;
  const height = window.innerHeight || canvas.value?.parentElement?.clientHeight || 720;
  return { width, height };
}

function handleResize() {
  const { width, height } = currentCanvasSize();
  resize(width, height);
}

// Sự kiện `resize` của window không phát khi cửa sổ đi từ ẩn sang hiện, nên chỉ
// nghe mỗi nó là chưa đủ — theo dõi thẳng kích thước container.
let containerObserver: ResizeObserver | null = null;

function observeContainer() {
  if (typeof ResizeObserver === 'undefined') return; // jsdom trong test không có
  const element = canvas.value?.parentElement;
  if (!element) return;
  containerObserver = new ResizeObserver(() => handleResize());
  containerObserver.observe(element);
}

function unobserveContainer() {
  containerObserver?.disconnect();
  containerObserver = null;
}

// ═══════════════════════════════════════════════════════
//  Locomotion — LIVA đi lại trên màn hình
//  Lớp locomotion là nguồn sự thật DUY NHẤT về vị trí. Prop screenPos chỉ đặt
//  chỗ đứng ban đầu / dời chỗ tức thì; nếu cả hai cùng ghi mỗi khung hình thì
//  chúng sẽ giẫm lên nhau và nhân vật giật tại chỗ.
// ═══════════════════════════════════════════════════════
const locomotion = useAvatarLocomotion({
  start: { x: props.screenPos?.x ?? 0.85, y: props.screenPos?.y ?? 1 },
});

let inspectionPoint: { x: number; y: number } | null = null;

function inspectScreenPoint(x: number, y: number) {
  inspectionPoint = {
    x: Math.min(Math.max(x, 0), 1),
    y: Math.min(Math.max(y, 0), 1),
  };
  setInspecting(true);
  lookAtScreenPoint(inspectionPoint.x, inspectionPoint.y);
}

function clearInspection() {
  inspectionPoint = null;
  setInspecting(false);
  setFacing(1, false);
  updateLookAt(0, 0);
}

function locomotionLoop(delta: number) {
  const snap = locomotion.update(delta);
  setScreenPosition(snap.x, snap.y);
  setLocomotionState(snap.state, snap.motion);
  if (inspectionPoint && snap.state === 'idle') {
    setInspecting(true);
    lookAtScreenPoint(inspectionPoint.x, inspectionPoint.y);
  } else {
    if (inspectionPoint) setInspecting(false);
    setFacing(snap.facing, snap.state === 'walk' || snap.state === 'run');
  }
}

function startLocomotion() {
  setFrameUpdate(locomotionLoop);
}

function stopLocomotion() {
  setFrameUpdate(null);
}

watch(
  () => props.screenPos,
  (pos) => {
    if (pos) {
      locomotion.teleport(pos.x, pos.y);
      setScreenPosition(pos.x, pos.y);
    }
  },
  { deep: true, immediate: true }
);

watch(
  () => props.avatarScale,
  (scale) => {
    if (typeof scale === 'number') setScale(scale);
  },
  { immediate: true }
);

// ═══════════════════════════════════════════════════════
//  Lifecycle
// ═══════════════════════════════════════════════════════
const initEngine = async () => {
  if (!canvas.value) {
    logger.error('[VRMEngine]', 'Canvas ref is null on mount');
    loadError.value = 'Canvas ref is null';
    return;
  }

  try {
    logger.info('[VRMEngine]', 'Engine initializing...', {
      width: canvas.value.width,
      height: canvas.value.height,
      modelConfig: props.modelConfig,
    });

    // 1. Init renderer with transparent background + lighting
    const { width: canvasWidth, height: canvasHeight } = currentCanvasSize();

    initRenderer(canvas.value, canvasWidth, canvasHeight);
    globalThis.addEventListener('resize', handleResize);
    observeContainer();
    logger.info('[VRMEngine]', 'Renderer initialized', { canvasWidth, canvasHeight });

    canvas.value.style.background = 'transparent';
    canvas.value.style.border = 'none';
    canvas.value.style.width = '100%';
    canvas.value.style.height = '100%';

    // 2. Load 3D model
    await loadSelectedModel(props.modelConfig);

    // 3. Start render loop
    startRenderLoop();

    // 4. Start auto-blink
    startAutoBlink();

    // 5. Start mouse LookAt
    startMouseLookAt();

    // 6. Start locomotion
    startLocomotion();
  } catch (e: unknown) {
    logger.error('[VRMEngine]', 'Init failed:', e instanceof Error ? e.message : String(e), e);
    // Ép kiểu thuần biên dịch (bị xoá khi build) để giữ nguyên hành vi runtime cũ
    loadError.value = (e as Error).message;
  }
};

const cleanupEngine = () => {
  globalThis.removeEventListener('resize', handleResize);
  unobserveContainer();
  stopLocomotion();
  stopMouseLookAt();
  stopAudioLipSync();

  if (isCameraOn.value) {
    stopTracking();
    setFaceTrackingActive(false);
  }
  if (trackingRAF !== null) {
    cancelAnimationFrame(trackingRAF);
    trackingRAF = null;
  }

  if (emotionDecayTimer) {
    clearTimeout(emotionDecayTimer);
    emotionDecayTimer = null;
  }

  // Deep Dispose: giải phóng VRAM hoàn toàn (Bao gồm renderer.forceContextLoss)
  disposeVRM();
};

onMounted(() => {
  initEngine();
});

onActivated(() => {
  // [Zombie RAM Killer] Re-init when returning from KeepAlive
  initEngine();
});

onDeactivated(() => {
  // [Zombie RAM Killer] Deep Dispose when hidden by KeepAlive to release VRAM
  cleanupEngine();
});

onUnmounted(() => {
  cleanupEngine();
});

// ═══════════════════════════════════════════════════════
//  Public API (cho WidgetApp gọi qua ref)
// ═══════════════════════════════════════════════════════
defineExpose({
  triggerMotion,
  startLipSync,
  stopLipSync,
  startAudioLipSync,
  stopAudioLipSync,
  setExpression,
  toggleCamera,
  isCameraOn,
  captureFrameForAI,
  currentModelFormat,
  setScreenPosition,
  setScale,
  getScreenBounds,
  // Locomotion — điều khiển LIVA đi lại
  moveTo: locomotion.moveTo,
  jump: locomotion.jump,
  stopMoving: locomotion.stop,
  setWander: locomotion.setWander,
  locomotionSnapshot: locomotion.snapshot,
  playGesture,
  setThinking,
  inspectScreenPoint,
  clearInspection,
});
</script>

<template>
  <div class="vrm-container" :class="{ 'full-screen': props.fullScreen !== false }">
    <canvas
      ref="canvas"
      width="400"
      height="700"
      style="
        cursor: pointer;
        position: relative;
        z-index: 2;
        width: 100%;
        height: 100%;
        display: block;
      "
    ></canvas>

    <!-- Hidden webcam video (no display, only for MediaPipe) -->
    <video ref="webcamVideo" class="webcam-hidden" playsinline muted></video>

    <!-- Camera toggle button.
         Ẩn ở chế độ toàn màn hình: nút không nằm trong InteractiveZones nên Rust
         luôn cho chuột xuyên qua — nó sẽ chỉ là một chấm không bấm được nổi giữa
         desktop. Ở chế độ này bật/tắt camera đi qua toggleCamera() đã expose. -->
    <button
      v-if="props.fullScreen === false"
      class="camera-toggle"
      :class="{ active: isCameraOn }"
      @click="toggleCamera"
      :title="isCameraOn ? 'Tắt Camera' : 'Bật Camera (Face Tracking)'"
    >
      {{ isCameraOn ? '👁️' : '👁️‍🗨️' }}
    </button>

    <!-- Camera indicator -->
    <div v-if="isCameraOn" class="camera-indicator">
      <span class="cam-dot"></span>
      <span class="cam-text">CAM</span>
    </div>

    <!-- Error indicator -->
    <div v-if="loadError" class="vrm-error">⚠ {{ loadError }}</div>
  </div>
</template>

<style scoped>
.vrm-container {
  position: relative;
  width: 400px;
  height: 700px;
  transform: scale(0.45);
  transform-origin: bottom center;
  overflow: visible;
}

.vrm-container.full-screen {
  width: 100vw;
  height: 100vh;
  transform: none;
  transform-origin: center center;
  overflow: hidden;
}

/* Hidden webcam — NOT displayed, only used by MediaPipe */
.webcam-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
  overflow: hidden;
}

/* Camera toggle button */
.camera-toggle {
  position: absolute;
  top: 12px;
  right: 12px;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.2);
  background: rgba(0, 0, 0, 0.3);
  color: white;
  font-size: 16px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.3s ease;
  pointer-events: auto;
  backdrop-filter: blur(4px);
  z-index: 10;
}

.camera-toggle:hover {
  background: rgba(0, 0, 0, 0.5);
  border-color: rgba(255, 255, 255, 0.4);
  transform: scale(1.1);
}

.camera-toggle.active {
  background: rgba(0, 180, 0, 0.4);
  border-color: rgba(0, 255, 0, 0.6);
  box-shadow: 0 0 12px rgba(0, 255, 0, 0.3);
}

/* Camera indicator */
.camera-indicator {
  position: absolute;
  top: 14px;
  right: 56px;
  display: flex;
  align-items: center;
  gap: 4px;
  pointer-events: none;
  z-index: 10;
}

.cam-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #00ff00;
  box-shadow: 0 0 6px #00ff00;
  animation: camPulse 1.5s infinite;
}

.cam-text {
  font-size: 10px;
  font-weight: 700;
  color: rgba(0, 255, 0, 0.8);
  letter-spacing: 1px;
}

@keyframes camPulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.4;
  }
}

.vrm-error {
  position: absolute;
  bottom: 80px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(255, 50, 50, 0.8);
  color: white;
  padding: 4px 12px;
  border-radius: 8px;
  font-size: 11px;
  white-space: nowrap;
  pointer-events: none;
}
</style>
