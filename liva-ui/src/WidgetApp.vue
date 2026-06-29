<script setup lang="ts">
/**
 * WidgetApp.vue — Hybrid Dual-Engine Widget
 * ==========================================
 * Entry component cho cửa sổ widget (transparent overlay).
 * - Auto-detect GPU → lazy load đúng engine (Live2D hoặc VRM)
 * - Zero cross-contamination: engine không dùng = 0 bytes RAM
 * - Phantom Bounding Box Fix (Phương án 1: pointer-events + IPC)
 */
import { ref, shallowRef, triggerRef, defineAsyncComponent, onMounted, onUnmounted, onActivated, onDeactivated, nextTick, watch, inject, computed } from "vue";
import type { IPlatformAdapter } from "./platform/IPlatformAdapter";
import { profileHardware, type EngineMode } from "./utils/HardwareDetector";
import { useVoicePipeline } from "./composables/useVoicePipeline";
import { logger } from "./utils/logger";
import { safeFetch } from "./utils/fetch";

import { useWidgetTheme } from "./composables/useWidgetTheme";
import { useWidgetDrag } from "./composables/useWidgetDrag";
import { useAudioQueue } from "./composables/useAudioQueue";
import { useChat } from "./composables/useChat";
import { useI18n } from "./composables/useI18n";
import { useGateway } from "./composables/useGateway";
import { useWidgetWS } from "./composables/useWidgetWS";
import { DEFAULT_WIDGET_MODEL, resolveEngineFromConfig, normalizeModelConfig } from "./utils/widgetModelResolver";

// Styles
import "./WidgetApp.css";

const platform = inject<IPlatformAdapter>('platform');

// ═══════════════════════════════════════════════════════
//  Lazy Load Engines (defineAsyncComponent = 0 byte khi không dùng)
// ═══════════════════════════════════════════════════════
const Live2DEngine = defineAsyncComponent(() =>
  import("./components/Live2DEngine.vue")
);
const VRMEngine = defineAsyncComponent(() =>
  import("./components/VRMEngine.vue")
);

const activeEngine = shallowRef<any>(null);
const engineMode = ref<EngineMode>('3D');
const activeModelConfig = ref<any>(null);
const hardwareInfo = ref<string>('');
const engineStatus = ref<string>('booting');

const applyWidgetConfig = (config: any, source: string) => {
  const nextEngine = resolveEngineFromConfig(config);
  const nextModelConfig = normalizeModelConfig(config);

  engineMode.value = nextEngine;
  activeModelConfig.value = nextModelConfig;
  activeEngine.value = nextEngine === '3D' ? VRMEngine : Live2DEngine;
  engineStatus.value = `config:${source}:${nextEngine}`;

  logger.info('[Widget]', `${source} → engine=${nextEngine}`, {
    avatarMode: config?.ui?.avatarMode ?? config?.avatarMode,
    activeModel: nextModelConfig,
  });
};

const engineRef = ref<any>(null);
const isSensing = ref(false);
let sensingTimeout: ReturnType<typeof setTimeout> | null = null;
const isCameraActive = ref(false);

const { t } = useI18n();
const gateway = useGateway();

// ═══════════════════════════════════════════════════════
//  Voice Input & Sound Setup
// ═══════════════════════════════════════════════════════
const voice = useVoicePipeline();
const volumeLevel = voice.volumeLevel;
const wakeWordThreshold = voice.wakeWordThreshold;
const setWakeWordThreshold = voice.setWakeWordThreshold;
const diagnosticsPanelRef = voice.diagnosticsPanelRef;
const pipelineError = voice.pipelineError;
const showDiagnostics = ref(false);
const isListening = computed(() => voice.state.value === 'ACTIVE');

// Silence TS unused variable check for diagnosticsPanelRef (used in Vue template ref)
void diagnosticsPanelRef;

// ═══════════════════════════════════════════════════════
//  Forward reference for sendMsg and stopQueuedAudio
// ═══════════════════════════════════════════════════════
const sendMsg = (event: string, payload: any = {}) => {
  widgetWS.sendMsg(event, payload);
};

const stopQueuedAudioForwarder = (clearQueue?: boolean) => {
  stopQueuedAudio(clearQueue);
};

// ═══════════════════════════════════════════════════════
//  Chat Composable
// ═══════════════════════════════════════════════════════
const chat = useChat(t, () => stopQueuedAudioForwarder(true), sendMsg);
const { inputText, isThinking, isCollapsed, messages, chatContainer, startNewChat, sendMessage, renderRichText, scrollToBottom } = chat;

const handleSendMessage = () => {
  sendMessage(widgetWS.ws.value);
};

// ═══════════════════════════════════════════════════════
//  Audio Queue Composable
// ═══════════════════════════════════════════════════════
const {
  isPlayingAudio,
  activeAudioSources,
  stopQueuedAudio,
  allowIncomingChunks,
  playWakeWordSound,
  handleBinaryAudioChunk,
  handleBase64AudioChunk,
  duckAudio,
  cleanup: cleanupAudio,
} = useAudioQueue(engineRef, computed(() => chat.isThinking.value), voice, sendMsg);

// ═══════════════════════════════════════════════════════
//  WebSocket Composable
// ═══════════════════════════════════════════════════════
const widgetWS = useWidgetWS({
  engineStatus,
  voice,
  gateway,
  isThinking,
  messages,
  engineRef,
  t,
  handleBinaryAudioChunk,
  handleBase64AudioChunk,
  stopQueuedAudio,
  allowIncomingChunks,
  isPlayingAudio,
  activeAudioSources,
  duckAudio,
  applyWidgetConfig,
  scrollToBottom,
});

// ═══════════════════════════════════════════════════════
//  Theme Composable
// ═══════════════════════════════════════════════════════
const { isLightMode, toggleTheme, initTheme } = useWidgetTheme();

// ═══════════════════════════════════════════════════════
//  Drag Composable
// ═══════════════════════════════════════════════════════
const { dragOffset, isDragging, hasMoved, snapPosition, verticalSnapPosition, onDragStart } = useWidgetDrag(isCollapsed);

const onDragEndClick = () => {
  if (!hasMoved.value) {
    toggleCollapse();
  }
};

// ═══════════════════════════════════════════════════════
//  Sensory Capture (Ctrl+Shift+S)
// ═══════════════════════════════════════════════════════
const handleKeydown = async (e: KeyboardEvent) => {
  if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "s") {
    isSensing.value = true;
    try {
      await safeFetch("http://127.0.0.1:3000/api/sensory-capture", { method: "POST" });
    } catch {
      // ignore
    }
    sensingTimeout = setTimeout(() => { isSensing.value = false; }, 30000);
  }
};

// ═══════════════════════════════════════════════════════
//  Wake Word Detection
// ═══════════════════════════════════════════════════════
const handleWakeWordDetection = () => {
  logger.info('[Widget]', 'Wake Word detected!');
  playWakeWordSound();
  messages.value = [...messages.value, { role: "assistant", text: t('wg_wake_word_ack') }];
  triggerRef(messages);
  scrollToBottom();
};

voice.onWakeWordDetected(handleWakeWordDetection);

const forceTriggerWakeWord = async () => {
  if (voice.state.value === 'OFF') {
    if (widgetWS.ws.value && widgetWS.ws.value.readyState === WebSocket.OPEN) {
      try {
        await voice.startPipeline(widgetWS.ws.value);
      } catch (e) {
        logger.warn('[Widget]', 'Failed to start voice pipeline on force trigger:', e);
        return;
      }
    }
  }
  handleWakeWordDetection();
  if (voice.state.value === 'PASSIVE') {
    voice.state.value = 'ACTIVE';
    sendMsg("wake_word_triggered");
  }
};

// Camera frame capture interval
let frameCaptureInterval: ReturnType<typeof setInterval> | null = null;

// ═══════════════════════════════════════════════════════
//  Voice Toggle (Push-to-talk)
// ═══════════════════════════════════════════════════════
const toggleVoice = () => {
  if (voice.state.value === 'OFF') {
    if (widgetWS.ws.value && widgetWS.ws.value.readyState === WebSocket.OPEN) {
      logger.info('[Widget]', 'Manually starting voice pipeline...');
      voice.startPipeline(widgetWS.ws.value).then(() => {
        if (voice.state.value === 'PASSIVE') {
          voice.toggleVoice();
        }
      }).catch((e: unknown) => {
        logger.warn('[Widget]', 'Failed to start voice pipeline on toggle:', e);
      });
    } else {
      logger.warn('[Widget]', 'Cannot start voice pipeline: WebSocket not ready');
    }
  } else {
    voice.toggleVoice();
  }
};

const interruptLIVA = () => {
  stopQueuedAudio(true);
  if (widgetWS.ws.value && widgetWS.ws.value.readyState === WebSocket.OPEN) {
    widgetWS.ws.value.send('[INTERRUPT]');
  }
};

const openDashboard = () => {
  if (platform) platform.invokeBackend('open_dashboard');
};

// ═══════════════════════════════════════════════════════
//  Phantom Bounding Box Fix — Rust Cursor Hit-Test System (Event-Driven Observers)
// ═══════════════════════════════════════════════════════
const chatUIRef = ref<HTMLElement | null>(null);
const miniIconsRef = ref<HTMLElement | null>(null);

let resizeObserver: ResizeObserver | null = null;
let mutationObserver: MutationObserver | null = null;

const updateInteractiveZones = () => {
  if (!platform) return;
  const zones: Array<{ x: number; y: number; width: number; height: number }> = [];
  const PAD = 12; // Extra padding for small targets (collapsed capsule)

  if (chatUIRef.value) {
    const rect = chatUIRef.value.getBoundingClientRect();
    zones.push({
      x: Math.max(0, rect.left - PAD),
      y: Math.max(0, rect.top - PAD),
      width: rect.width + PAD * 2,
      height: rect.height + PAD * 2,
    });
  }

  if (!isCollapsed.value && chatContainer.value) {
    const rect = chatContainer.value.getBoundingClientRect();
    zones.push({
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
    });
  }

  if (miniIconsRef.value) {
    const rect = miniIconsRef.value.getBoundingClientRect();
    zones.push({
      x: Math.max(0, rect.left - PAD),
      y: Math.max(0, rect.top - PAD),
      width: rect.width + PAD * 2,
      height: rect.height + PAD * 2,
    });
  }

  logger.info('[Widget] Interactive zones updated:', JSON.stringify(zones.map(z => ({x: Math.round(z.x), y: Math.round(z.y), w: Math.round(z.width), h: Math.round(z.height)}))), 'collapsed:', isCollapsed.value);

  platform.invokeBackend("update_interactive_zones", { zones }).catch((err) => {
    logger.warn("[Widget] Failed to update interactive zones:", err);
  });
};

const observeElements = () => {
  if (resizeObserver) {
    resizeObserver.disconnect();
  } else {
    resizeObserver = new ResizeObserver(() => {
      updateInteractiveZones();
    });
  }

  if (mutationObserver) {
    mutationObserver.disconnect();
  } else {
    mutationObserver = new MutationObserver(() => {
      updateInteractiveZones();
    });
  }

  const elementsToObserve = [
    chatUIRef.value,
    miniIconsRef.value,
    chatContainer.value
  ].filter((el): el is HTMLElement => !!el);

  for (const el of elementsToObserve) {
    resizeObserver.observe(el);
    mutationObserver.observe(el, {
      attributes: true,
      childList: true,
      subtree: true
    });
  }
};

watch([chatUIRef, miniIconsRef, chatContainer], () => {
  observeElements();
  updateInteractiveZones();
}, { immediate: true });

watch([isCollapsed, isDragging, () => messages.value.length], () => {
  nextTick(() => {
    updateInteractiveZones();
  });
}, { deep: true });

const toggleCollapse = () => {
  isCollapsed.value = !isCollapsed.value;
  const currentAbsoluteY = window.innerHeight - 60 + dragOffset.value.y;
  verticalSnapPosition.value = currentAbsoluteY < window.innerHeight / 2 ? 'top' : 'bottom';
};

watch(isThinking, (val) => {
  if (val && engineRef.value?.triggerMotion) {
    engineRef.value.triggerMotion();
  }
});

watch(() => engineRef.value?.isCameraOn?.value, (val) => {
  isCameraActive.value = !!val;
  if (val) {
    startFrameCapture();
  } else {
    stopFrameCapture();
  }
});

function startFrameCapture() {
  if (frameCaptureInterval) return;
  frameCaptureInterval = setInterval(() => {
    if (!widgetWS.ws.value || widgetWS.ws.value.readyState !== WebSocket.OPEN) return;
    if (!engineRef.value?.captureFrameForAI) return;

    const frame = engineRef.value.captureFrameForAI();
    if (frame) {
      sendMsg("camera_frame", { image: frame, timestamp: Date.now() });
    }
  }, 10000);
}

function stopFrameCapture() {
  if (frameCaptureInterval) {
    clearInterval(frameCaptureInterval);
    frameCaptureInterval = null;
  }
}

onMounted(() => {
  globalThis.addEventListener("keydown", handleKeydown);
  initTheme();

  const hw = profileHardware();
  hardwareInfo.value = `GPU=${hw.gpu}; RAM=${hw.ram}GB; Cores=${hw.cores}; WebGL=${hw.webglVersion}; MaxTex=${hw.maxTextureSize}; Recommended=${hw.recommendedEngine}`;
  logger.info('[Widget]', 'Hardware profile detected', hw);

  engineMode.value = '3D';
  activeModelConfig.value = DEFAULT_WIDGET_MODEL;
  activeEngine.value = VRMEngine;
  engineStatus.value = 'forced-3d-bootstrap';
  logger.info('[Widget]', 'Initial engine forced to 3D for diagnostics');

  nextTick(() => {
    updateInteractiveZones();
  });

  (window as any).sendLIVAMessage = (text: string) => {
    if (widgetWS.ws.value && widgetWS.ws.value.readyState === WebSocket.OPEN) {
      stopQueuedAudio(true);
      messages.value = [...messages.value, { role: "user", text }];
      triggerRef(messages);
      sendMsg("user_voice_command", { text });
      scrollToBottom();
    }
  };
});

onUnmounted(() => {
  globalThis.removeEventListener("keydown", handleKeydown);
  cleanupAudio();
  chat.cleanup();
  stopFrameCapture();
  voice.stopPipeline();
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
  if (mutationObserver) {
    mutationObserver.disconnect();
    mutationObserver = null;
  }
  if (sensingTimeout) { clearTimeout(sensingTimeout); sensingTimeout = null; }
});

onActivated(() => {
  if (isCameraActive.value) {
    startFrameCapture();
  }
});

onDeactivated(() => {
  stopFrameCapture();
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
  if (mutationObserver) {
    mutationObserver.disconnect();
    mutationObserver = null;
  }
});
</script>

<template>
  <div class="h-screen w-screen flex flex-col items-end justify-end bg-transparent font-sans relative overflow-hidden">
    <!-- 3D/2D Engine (pointer-events: none → click xuyên qua) -->
    <component
      :is="activeEngine"
      ref="engineRef"
      :modelConfig="activeModelConfig"
      :fullScreen="false"
      style="pointer-events: none; position: fixed; right: 0; bottom: 0; z-index: 0; width: 400px; height: 700px; transform-origin: bottom right; transform: scale(0.45);"
    />
    <!-- Debug info hidden from UI -->
    <div v-if="false" class="hardware-badge">
      {{ hardwareInfo }}
    </div>

    <div v-if="false" class="engine-badge">
      Engine: {{ engineMode }} · {{ engineStatus }}
    </div>

    <!-- Chat UI Layer (pointer-events: auto → bắt click) -->
    <div
      ref="chatUIRef"
      :class="[
        'flex flex-col relative z-10 animate-fade-in-up mb-[60px] mr-4',
        isDragging ? '' : 'transition-all duration-300 ease-out',
        !isCollapsed ? 'w-full max-w-[400px]' : 'w-auto'
      ]"
      :style="{
        'pointer-events': 'auto',
        left: dragOffset.x + 'px',
        top: dragOffset.y + 'px'
      }"
    >
      <!-- Floating Mini-Icons -->
      <div 
        ref="miniIconsRef"
        class="absolute flex gap-2.5 no-drag-region transition-all duration-300"
        :class="[
          snapPosition === 'left' ? 'left-0 flex-row-reverse' : 'right-2 flex-row',
          verticalSnapPosition === 'top' ? '-top-[44px]' : '-bottom-[44px]'
        ]"
      >
        <button class="floating-mini-icon w-8 h-8 flex items-center justify-center transition-all hover:scale-105" :title="t('wg_new_chat')" @click="startNewChat">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 3v5h-5" />
          </svg>
        </button>
        <button class="floating-mini-icon w-8 h-8 flex items-center justify-center transition-all hover:scale-105" title="Memory Knot">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
            <path stroke-linecap="round" stroke-linejoin="round" d="M13.19 8.688a4.5 4.5 0 0 1 1.242 7.244l-4.5 4.5a4.5 4.5 0 0 1-6.364-6.364l1.757-1.757m13.35-.622 1.757-1.757a4.5 4.5 0 0 0-6.364-6.364l-4.5 4.5a4.5 4.5 0 0 0 1.242 7.244" />
          </svg>
        </button>
        <button class="floating-mini-icon w-8 h-8 flex items-center justify-center transition-all hover:scale-105 relative" title="Shadow Digest">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.042A8.967 8.967 0 0 0 6 3.75c-1.052 0-2.062.18-3 .512v14.25A8.987 8.987 0 0 1 6 18c2.305 0 4.408.867 6 2.292m0-14.25a8.966 8.966 0 0 1 6-2.292c1.052 0 2.062.18 3 .512v14.25A8.987 8.987 0 0 0 18 18a8.967 8.967 0 0 0-6 2.292m0-14.25v14.25" />
          </svg>
        </button>
      </div>
      <!-- Messages (scrollable) -->
      <div
        ref="chatContainer"
        v-if="!isCollapsed && (messages.length > 1 || isThinking)"
        :class="[
          'absolute w-full scrollbar-hide max-h-[300px] overflow-y-auto flex flex-col gap-2 px-2 py-1',
          verticalSnapPosition === 'top' ? 'top-full mt-4' : 'bottom-full mb-4'
        ]"
      >
        <template v-for="(msg, idx) in messages.slice(-15)" :key="idx">
          <div
            v-if="msg.text?.trim() || msg.thinking?.trim()"
            :class="[
              'px-4 py-2.5 rounded-[22px] text-sm max-w-[85%] leading-relaxed flex flex-col gap-1 msg-enter',
              msg.role === 'user'
                ? 'self-end bg-gradient-to-r from-purple-600 to-blue-500 text-white rounded-br-[6px] chat-bubble-user'
                : 'self-start chat-bubble-ai rounded-bl-[6px]'
            ]"
          >
            <details v-if="msg.thinking" :open="!msg.text || msg.text.length === 0" class="thinking-details mb-2 select-none opacity-80 w-full" style="outline: none;">
              <summary class="text-xs text-purple-400 hover:text-purple-300 font-semibold focus:outline-none cursor-pointer flex items-center gap-1">💭 {{ t('thinking_details') }}</summary>
              <div class="mt-1 pl-2 border-l border-purple-500/30 text-xs text-gray-400/80 leading-relaxed whitespace-pre-line">{{ msg.thinking }}</div>
            </details>
            <div v-if="msg.text" v-html="renderRichText(msg.text)" class="w-full"></div>
          </div>
        </template>
        <!-- Thinking indicator -->
        <div v-if="isThinking" class="self-start chat-bubble-ai px-4 py-2.5 rounded-[22px] rounded-bl-[6px] text-sm flex items-center gap-2 msg-enter">
          <span class="thinking-dot text-purple-400" style="animation-delay: 0s">●</span>
          <span class="thinking-dot text-purple-400" style="animation-delay: 0.2s">●</span>
          <span class="thinking-dot text-purple-400" style="animation-delay: 0.4s">●</span>
        </div>
      </div>

      <!-- Developer Diagnostics Panel -->
      <div
        v-if="!isCollapsed && showDiagnostics"
        ref="diagnosticsPanelRef"
        class="diagnostics-box w-full mb-3 p-4 rounded-[22px] text-xs flex flex-col gap-3 glass-diagnostics msg-enter border border-purple-500/20 shadow-2xl relative"
        style="pointer-events: auto; backdrop-filter: blur(20px);"
      >
        <div class="flex justify-between items-center border-b border-white/10 pb-2">
          <strong class="text-purple-400 font-semibold tracking-wide flex items-center gap-1.5">
            <span>🛠️</span> LIVA WAKE DIAGNOSTICS
          </strong>
          <span class="px-2 py-0.5 rounded-full text-[10px] uppercase font-bold tracking-wider" :class="{
            'bg-slate-800 text-slate-400 border border-slate-700': voice.state.value === 'OFF',
            'bg-blue-500/20 text-blue-400 border border-blue-500/30': voice.state.value === 'PASSIVE',
            'bg-green-500/20 text-green-400 border border-green-500/30': voice.state.value === 'ACTIVE',
            'bg-purple-500/20 text-purple-400 border border-purple-500/30': voice.state.value === 'PROCESSING'
          }">
            {{ voice.state.value }}
          </span>
        </div>

        <!-- Error Alert Banner -->
        <div v-if="pipelineError" class="p-2.5 bg-red-500/10 border border-red-500/25 text-red-200 rounded-xl text-[10px] leading-relaxed">
          <strong>⚠️ Lỗi thiết bị âm thanh:</strong> {{ pipelineError }}
          <div class="mt-1 opacity-80 text-[9px]">
            Giải pháp: Vui lòng kiểm tra và cấp quyền truy cập Microphone trong Cài đặt Hệ thống (Windows Settings -> Privacy -> Microphone) hoặc trình duyệt.
          </div>
        </div>

        <!-- Mic Volume Meter -->
        <div class="flex flex-col gap-1.5">
          <div class="flex justify-between text-slate-400">
            <span>Microphone Level (RMS)</span>
            <span class="font-mono text-[10px] text-blue-400">Live 60 FPS</span>
          </div>
          <div class="h-2 w-full bg-black/40 rounded-full overflow-hidden border border-white/5 relative">
            <div class="h-full bg-gradient-to-r from-blue-500 to-indigo-500 rounded-full transition-all duration-75" style="width: var(--rms-level, 0%)"></div>
          </div>
        </div>

        <!-- Classifier Confidence Score -->
        <div class="flex flex-col gap-1.5">
          <div class="flex justify-between text-slate-400">
            <span>Wake Word Confidence</span>
            <span class="font-mono text-[10px] text-purple-400">Target: {{ wakeWordThreshold.toFixed(2) }}</span>
          </div>
          <div class="h-2 w-full bg-black/40 rounded-full overflow-hidden border border-white/5 relative">
            <div class="h-full bg-gradient-to-r from-purple-500 to-pink-500 rounded-full transition-all duration-75" style="width: var(--confidence-level, 0%)"></div>
            <!-- Threshold indicator mark -->
            <div class="absolute top-0 bottom-0 w-[2px] bg-red-500/80 shadow-[0_0_4px_#ef4444]" :style="{ left: `${wakeWordThreshold * 100}%` }" title="Detection Threshold"></div>
          </div>
        </div>

        <!-- Sensitivity Threshold Slider -->
        <div class="flex flex-col gap-1.5 bg-white/5 p-2.5 rounded-xl border border-white/5">
          <div class="flex justify-between items-center">
            <span class="text-slate-300">Sensitivity (Ngưỡng nhạy)</span>
            <span class="font-mono font-bold text-purple-300">{{ wakeWordThreshold.toFixed(3) }}</span>
          </div>
          <input
            type="range"
            min="0.02"
            max="0.99"
            step="0.01"
            :value="wakeWordThreshold"
            @input="setWakeWordThreshold(parseFloat(($event.target as HTMLInputElement).value))"
            class="w-full accent-purple-500 cursor-pointer h-1.5 bg-black/30 rounded-lg appearance-none"
          />
          <p class="text-[10px] text-slate-400 leading-normal mt-0.5">
            Mẹo: Hạ thấp ngưỡng nhạy (ví dụ 0.08) nếu mic yếu. Nâng cao nếu phòng ồn để tránh tự kích hoạt nhầm.
          </p>
        </div>

        <!-- Manual Actions -->
        <div class="flex gap-2">
          <button
            @click="forceTriggerWakeWord"
            class="flex-1 py-1.5 px-3 rounded-xl bg-purple-600/30 hover:bg-purple-600/50 border border-purple-500/30 text-[11px] font-semibold text-purple-200 transition-all active:scale-95 flex items-center justify-center gap-1.5"
          >
            <span>⚡</span> Force Trigger
          </button>
        </div>

        <!-- Architecture info -->
        <div class="text-[10px] text-slate-500 leading-relaxed border-t border-white/5 pt-2">
          💡 Model chạy local offline 100% trong Browser/Tauri WebWorker (không gửi âm thanh lên Gateway/Cloud để bảo mật).
        </div>
      </div>

      <!-- Full Chat Bar State -->
      <div v-if="!isCollapsed" class="chat-capsule w-full flex items-center p-[6px]" :class="snapPosition === 'left' ? 'flex-row-reverse' : ''">
        <!-- Drag Handle (Grip) -->
        <div 
          class="w-6 h-8 flex items-center justify-center cursor-move transition-colors" 
          :class="isLightMode ? 'text-slate-400 hover:text-slate-600' : 'text-white/30 hover:text-white/60'"
          :title="t('wg_drag')"
          @mousedown="onDragStart"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" fill="currentColor" viewBox="0 0 16 16">
            <path d="M7 2a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 5a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 8a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 14a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0z"/>
          </svg>
        </div>
        <input
          v-model="inputText"
          @keyup.enter="handleSendMessage"
          type="text"
          :placeholder="t('wg_placeholder')"
          class="chat-input flex-1 bg-transparent border-none pl-1 pr-2 focus:outline-none w-full"
        />
        <!-- Send Button (visible when input has text) -->
        <button
          v-if="inputText.trim()"
          @click="handleSendMessage"
          class="send-btn"
          :title="t('wg_send') || 'Send'"
        >
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 12 3.269 3.126A59.769 59.769 0 0 1 21.485 12 59.768 59.768 0 0 1 3.27 20.876L5.999 12Zm0 0h7.5" />
          </svg>
        </button>
        <div class="flex items-center gap-1.5" :class="snapPosition === 'left' ? 'flex-row-reverse pl-1' : 'pr-1'">
          <!-- Toggle Collapse Button -->
          <button
            @click="toggleCollapse"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center"
            :title="t('wg_collapse')"
          >
            <svg v-if="snapPosition === 'left'" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
              <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 19.5L8.25 12l7.5-7.5" />
            </svg>
            <svg v-else xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
              <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
            </svg>
          </button>

          <!-- Theme toggle button -->
          <button
            @click="toggleTheme"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center"
            :title="t('wg_theme')"
          >
            <svg v-if="isLightMode" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4 text-yellow-400 drop-shadow-[0_0_8px_rgba(250,204,21,0.6)]">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v2.25m6.364.386-1.591 1.591M21 12h-2.25m-.386 6.364-1.591-1.591M12 18.75V21m-4.773-2.227-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0Z" />
            </svg>
            <svg v-else xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4 text-blue-100 drop-shadow-[0_0_6px_rgba(219,234,254,0.4)]">
              <path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.72 9.72 0 0 1 18 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 0 0 3 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 0 0 9.002-5.998Z" />
            </svg>
          </button>

          <!-- Diagnostics toggle button -->
          <button
            @click="showDiagnostics = !showDiagnostics"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center transition-all"
            :class="showDiagnostics ? 'text-purple-400 bg-purple-500/10' : 'text-slate-400 hover:text-slate-200'"
            title="Diagnostics"
          >
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 9V4.5M12 9V3M15 9V5.25M6 9V7.5M3 9v7.5m3-3V21m3-6.75V18m3-6V19.5m3-8.25V18m3-4.5V21" />
            </svg>
          </button>
          
          <!-- Camera indicator -->
          <div
            v-if="isCameraActive"
            class="w-8 h-8 rounded-full bg-green-500/20 text-green-400 flex justify-center items-center text-xs"
            :title="t('wg_cam_on')"
          >
            👁️
          </div>
          
          <button
            @click="openDashboard"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center"
            :title="t('wg_settings')"
          >
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.99l1.005.828c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z" />
              <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
            </svg>
          </button>

          <!-- Voice button (mic toggle) - Focal Point -->
          <button
            @click="isThinking ? interruptLIVA() : toggleVoice()"
            :class="[
              'w-9 h-9 rounded-full flex justify-center items-center transition-all relative',
              snapPosition === 'left' ? 'mr-1' : 'ml-1',
              isThinking ? 'animate-pulse' : '',
              isListening 
                ? (isLightMode 
                    ? 'bg-indigo-400/60 text-white shadow-[0_0_15px_rgba(129,140,248,0.5)]' 
                    : 'bg-[#43528F]/30 text-[#0f1225] shadow-[inset_0_0_15px_rgba(99,102,241,0.2)]') 
                : 'mic-btn'
            ]"
            :title="isThinking ? t('wg_interrupt') : (isListening ? t('wg_stop_mic') : t('wg_start_mic'))"
          >
            <!-- Volume ring (when recording) -->
            <svg v-if="isListening" class="voice-ring" viewBox="0 0 36 36">
              <circle cx="18" cy="18" r="16" fill="none" :stroke="isLightMode ? 'rgba(255,255,255,0.4)' : 'rgba(0,0,0,0.5)'" stroke-width="2" />
              <circle
                cx="18" cy="18" r="16" fill="none" :stroke="isLightMode ? '#ffffff' : '#7C93F5'" stroke-width="2.5"
                :stroke-dasharray="`${volumeLevel * 100} ${100 - volumeLevel * 100}`"
                stroke-linecap="round"
                transform="rotate(-90 18 18)"
              />
            </svg>
            <!-- Icon -->
            <svg v-if="isThinking" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M10.05 4.575a1.575 1.575 0 1 0-3.15 0v3m3.15-3v-1.5a1.575 1.575 0 0 1 3.15 0v1.5m-3.15 0 .075 5.925m3.075.75V4.575m0 0a1.575 1.575 0 0 1 3.15 0V15M6.9 7.575a1.575 1.575 0 1 0-3.15 0v8.175a6.75 6.75 0 0 0 6.75 6.75h2.018a5.25 5.25 0 0 0 3.712-1.538l1.732-1.732a5.25 5.25 0 0 0 1.538-3.712l.003-2.024a.668.668 0 0 1 .198-.471 1.575 1.575 0 1 0-2.228-2.228 3.818 3.818 0 0 0-1.12 2.687M6.9 7.575V12m6.27 4.318A4.49 4.49 0 0 1 16.35 15m.002 0h-.002" />
            </svg>
            <svg v-else-if="isListening" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 18.75a6 6 0 0 0 6-6v-1.5m-6 7.5a6 6 0 0 1-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 0 1-3-3V4.5a3 3 0 1 1 6 0v8.25a3 3 0 0 1-3 3Z" />
            </svg>
            <svg v-else xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 18.75a6 6 0 0 0 6-6v-1.5m-6 7.5a6 6 0 0 1-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 0 1-3-3V4.5a3 3 0 1 1 6 0v8.25a3 3 0 0 1-3 3Z" />
            </svg>
          </button>
        </div>
      </div>

      <!-- Compact Collapsed State — LIVA Branded Icon -->
      <div 
        v-else 
        class="chat-capsule collapsed-capsule w-12 h-12 flex items-center justify-center relative rounded-full shadow-lg ml-auto cursor-pointer"
        @mousedown.stop="onDragStart"
        @mouseup.stop="onDragEndClick"
        :title="t('wg_collapse')"
      >
        <!-- LIVA Sparkle Icon (Non-button representation to avoid nested click boundaries) -->
        <div class="pointer-events-none flex justify-center items-center w-9 h-9">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5" :class="isLightMode ? 'text-indigo-500' : 'text-purple-300'">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904 9 18.75l-.813-2.846a4.5 4.5 0 0 0-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 0 0 3.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 0 0 3.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 0 0-3.09 3.09ZM18.259 8.715 18 9.75l-.259-1.035a3.375 3.375 0 0 0-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 0 0 2.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 0 0 2.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 0 0-2.455 2.456ZM16.894 20.567 16.5 21.75l-.394-1.183a2.25 2.25 0 0 0-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 0 0 1.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 0 0 1.423 1.423l1.183.394-1.183.394a2.25 2.25 0 0 0-1.423 1.423Z" />
          </svg>
        </div>
      </div>
    </div>
  </div>
</template>
