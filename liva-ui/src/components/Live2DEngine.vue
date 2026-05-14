<script setup lang="ts">
/**
 * Live2DEngine.vue — 2D Avatar Engine (PIXI.js + Live2D Cubism2)
 * ================================================================
 * Extracted from original App.vue. Used on weak GPUs (Intel UHD, etc.)
 * Lazy-loaded via defineAsyncComponent — 0 bytes when not used.
 */
import { ref, onMounted, onUnmounted } from "vue";

const l2dCanvas = ref<HTMLCanvasElement | null>(null);
let avatarModel: any = null;
let pixiApp: any = null;

const props = defineProps<{
  modelConfig?: any;
}>();

onMounted(async () => {
  // Đợi 1 tick để canvas đã mount
  await new Promise(r => setTimeout(r, 100));

  try {
    // Dynamic import ép vòng đời ưu tiên (tránh Hoisting Error gây trắng màn hình)
    const PIXI = await import("pixi.js");
    (globalThis as any).PIXI = PIXI;
    const { Live2DModel } = await import("pixi-live2d-display/cubism2");

    const app = new PIXI.Application({
      view: l2dCanvas.value!,
      transparent: true,
      width: 500,
      height: 700,
      autoStart: true,
    });

    // Load model Live2D — Bé Phù Thủy Pio
    let live2dUrl = "https://unpkg.com/live2d-widget-model-pio@9.1.2/assets/index.json";
    if (props.modelConfig && props.modelConfig.filename && props.modelConfig.filename.startsWith('http')) {
      live2dUrl = props.modelConfig.filename;
    }
    
    avatarModel = await Live2DModel.from(live2dUrl);
    app.stage.addChild(avatarModel);
    pixiApp = app;

    // Setup tỷ lệ và vị trí
    avatarModel.scale.set(0.35);
    avatarModel.x = 100;
    avatarModel.y = 320;

    // Interaction: chọc tức / vuốt ve
    avatarModel.on("pointertap", () => {
      avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
    });
  } catch (e) {
    console.error("[Live2DEngine] PIXI Model Injection failed:", e);
  }
});

onUnmounted(() => {
  // Hủy tài nguyên WebGL và Texture của PIXI
  if (pixiApp) {
    pixiApp.destroy(true, { children: true, texture: true, baseTexture: true });
    pixiApp = null;
    avatarModel = null;
  }
});

// ═══════════════════════════════════════════════════════
//  Public API (cho WidgetApp gọi qua ref)
// ═══════════════════════════════════════════════════════
function triggerMotion() {
  if (avatarModel) {
    avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
  }
}

function startLipSync() {
  if (avatarModel) {
    avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
  }
}

function stopLipSync() {
  // Live2D motion tự dừng
}

let lipSyncRAF: number | null = null;
let isLipSyncing = false;
let currentLipSyncData: Float32Array | null = null;
let currentAudioStartTime: number = 0;
let currentAudioCtx: AudioContext | null = null;

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

  // Live2D currently only uses startRandomMotion for lip sync
  // In the future, we can map currentLipSyncData[index] to Live2D Parameters like ParamMouthOpenY
  if (Math.random() > 0.95) {
     startLipSync();
  }
}

function stopAudioLipSync() {
  isLipSyncing = false;
  if (lipSyncRAF !== null) {
    cancelAnimationFrame(lipSyncRAF);
    lipSyncRAF = null;
  }
  stopLipSync();
}

defineExpose({ triggerMotion, startLipSync, stopLipSync, playPrecalculatedLipSync, stopAudioLipSync });
</script>

<template>
  <canvas
    ref="l2dCanvas"
    width="500"
    height="800"
    style="mix-blend-mode: multiply; cursor: pointer;"
  ></canvas>
</template>
