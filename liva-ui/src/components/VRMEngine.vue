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
import { ref, onMounted, onUnmounted, watch } from "vue";
import { use3DModel } from "../composables/use3DModel";
import { useFaceTracking } from "../composables/useFaceTracking";

const canvas = ref<HTMLCanvasElement | null>(null);
const webcamVideo = ref<HTMLVideoElement | null>(null);
const isLoaded = ref(false);
const loadError = ref<string | null>(null);
const isCameraOn = ref(false);

const props = defineProps<{
  modelConfig?: any;
}>();

// Electron API for mouse position
const electronAPI = (globalThis as any).electronAPI;

const {
  currentModelFormat,
  initRenderer,
  loadModel,
  startRenderLoop,
  startAutoBlink,
  startLipSync,
  stopLipSync,
  triggerMotion,
  updateLookAt,
  updateExpressions,
  setFaceTrackingActive,
  dispose: disposeVRM,
} = use3DModel();

const {
  faceData,
  isTracking,
  startTracking,
  stopTracking,
  captureFrame,
} = useFaceTracking();

// ═══════════════════════════════════════════════════════
//  Pre-calculated Audio-Driven Lip-Sync (Web Worker)
// ═══════════════════════════════════════════════════════
let lipSyncRAF: number | null = null;
let isLipSyncing = false;
let currentLipSyncData: Float32Array | null = null;
let currentAudioStartTime: number = 0;
let currentAudioCtx: AudioContext | null = null;

/**
 * Play volume-driven lip-sync using precalculated Float32Array from Web Worker.
 */
function playPrecalculatedLipSync(lipSyncData: Float32Array, startTime: number, audioCtx: AudioContext) {
  currentLipSyncData = lipSyncData;
  currentAudioStartTime = startTime;
  currentAudioCtx = audioCtx;

  if (!isLipSyncing) {
    isLipSyncing = true;
    lipSyncLoop();
  }
}

function lipSyncLoop() {
  if (!isLipSyncing || !currentLipSyncData || !currentAudioCtx) return;
  lipSyncRAF = requestAnimationFrame(lipSyncLoop);
  
  const elapsed = currentAudioCtx.currentTime - currentAudioStartTime;
  if (elapsed < 0) return;
  
  const index = Math.floor(elapsed * 60);
  if (index >= currentLipSyncData.length) {
    return;
  }

  // The amplitude is in currentLipSyncData[index], ranging 0-255.
  // For now, we still just call startLipSync() to let use3DModel handle the actual blendshape,
  // but we save the Main Thread from calculating the FFT using AnalyserNode!
  startLipSync();
}

function stopAudioLipSync() {
  isLipSyncing = false;
  if (lipSyncRAF !== null) {
    cancelAnimationFrame(lipSyncRAF);
    lipSyncRAF = null;
  }
  stopLipSync();
}

// ═══════════════════════════════════════════════════════
//  Global Mouse LookAt (eyes follow cursor across desktop)
//  Runs when face tracking is OFF (face tracking takes priority)
// ═══════════════════════════════════════════════════════
let mouseLookAtInterval: ReturnType<typeof setInterval> | null = null;

function startMouseLookAt() {
  if (mouseLookAtInterval) return;
  // Poll every 100ms (10fps is enough for smooth eye tracking)
  mouseLookAtInterval = setInterval(async () => {
    if (isCameraOn.value) return; // Face tracking takes priority
    if (!electronAPI?.getMousePosition) return;
    
    try {
      const pos = await electronAPI.getMousePosition();
      // Map normalized -1..1 → VRM yaw/pitch degrees
      // Clamp to gentle range for natural movement
      const yaw = pos.x * 25;   // ±25° max
      const pitch = -pos.y * 15; // ±15° max (invert Y: cursor up = look up)
      updateLookAt(yaw, pitch);
    } catch {
      // Electron IPC not available (dev mode without Electron)
    }
  }, 100);
}

function stopMouseLookAt() {
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

const loadSelectedModel = async (config: any) => {
  let modelPath = '/models/vrm/chibi/tripo_convert_648e4371-4299-44d8-94d8-e6a63e0e07a3.fbx';
  if (config && config.filename) {
    if (config.filename === 'default.vrm') {
      modelPath = '/models/vrm/chibi/tripo_convert_648e4371-4299-44d8-94d8-e6a63e0e07a3.fbx';
    } else {
      modelPath = config.filename.includes('/') ? config.filename : `/models/vrm/${config.filename}`;
    }
  }

  try {
    await loadModel(modelPath);
    isLoaded.value = true;
    loadError.value = null;
  } catch (e: any) {
    console.warn(`[VRMEngine] Model "${modelPath}" load failed:`, e?.message || e);
    loadError.value = `Model load failed: ${e?.message || modelPath}`;
    isLoaded.value = true;
  }
};

watch(() => props.modelConfig, async (newConfig: any) => {
  if (newConfig) {
    await loadSelectedModel(newConfig);
  }
}, { deep: true });

// ═══════════════════════════════════════════════════════
//  Lifecycle
// ═══════════════════════════════════════════════════════
onMounted(async () => {
  if (!canvas.value) return;

  try {
    // 1. Init renderer with transparent background + lighting
    initRenderer(canvas.value, 400, 700);

    // 2. Load 3D model
    await loadSelectedModel(props.modelConfig);

    // 3. Start render loop
    startRenderLoop();

    // 4. Start auto-blink
    startAutoBlink();

    // 5. Start mouse LookAt (when no face tracking)
    startMouseLookAt();

  } catch (e: any) {
    console.error("[VRMEngine] Init failed:", e);
    loadError.value = e.message;
  }
});

onUnmounted(() => {
  // Stop mouse lookAt
  stopMouseLookAt();
  stopAudioLipSync();

  // Stop face tracking first
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

  // Deep Dispose: giải phóng VRAM hoàn toàn
  disposeVRM();
});

// ═══════════════════════════════════════════════════════
//  Public API (cho WidgetApp gọi qua ref)
// ═══════════════════════════════════════════════════════
defineExpose({
  triggerMotion,
  startLipSync,
  stopLipSync,
  playPrecalculatedLipSync,
  stopAudioLipSync,
  setExpression,
  toggleCamera,
  isCameraOn,
  captureFrameForAI,
  currentModelFormat,
});
</script>

<template>
  <div class="vrm-container">
    <canvas
      ref="canvas"
      width="400"
      height="700"
      style="cursor: pointer;"
    ></canvas>

    <!-- Hidden webcam video (no display, only for MediaPipe) -->
    <video
      ref="webcamVideo"
      class="webcam-hidden"
      playsinline
      muted
    ></video>

    <!-- Camera toggle button -->
    <button
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
    <div v-if="loadError" class="vrm-error">
      ⚠ {{ loadError }}
    </div>
  </div>
</template>

<style scoped>
.vrm-container {
  position: relative;
  width: 400px;
  height: 700px;
  transform: scale(0.6);
  transform-origin: bottom center;
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
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
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
