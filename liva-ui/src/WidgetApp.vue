<script setup lang="ts">
/**
 * WidgetApp.vue — Hybrid Dual-Engine Widget
 * ==========================================
 * Entry component cho cửa sổ widget (transparent overlay).
 * - Auto-detect GPU → lazy load đúng engine (Live2D hoặc VRM)
 * - Zero cross-contamination: engine không dùng = 0 bytes RAM
 * - Phantom Bounding Box Fix (Phương án 1: pointer-events + IPC)
 */
import { ref, shallowRef, triggerRef, defineAsyncComponent, onMounted, onUnmounted, onActivated, onDeactivated, nextTick, watch, inject } from "vue";
import type { IPlatformAdapter } from "./platform/IPlatformAdapter";
import { profileHardware, type EngineMode } from "./utils/HardwareDetector";
import { computed } from "vue";
import { useVoicePipeline } from "./composables/useVoicePipeline";
import { logger } from "./utils/logger";
import { safeFetch } from "./utils/fetch";

const platform = inject<IPlatformAdapter>('platform');

const DEFAULT_WIDGET_MODEL = {
  filename: "models/vrm/default_avatar/tripo_convert_648e4371-4299-44d8-94d8-e6a63e0e07a3.fbx",
  type: "3d",
  format: "fbx",
};

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

const resolveEngineFromConfig = (config: any) => {
  const avatarMode = config?.ui?.avatarMode ?? config?.avatarMode ?? config?.avatar?.engineMode;
  const activeModel = config?.ui?.activeModel ?? config?.activeModel ?? config?.avatar?.activeModel;

  if (avatarMode === '2D' || avatarMode === '3D') {
    return avatarMode;
  }

  if (activeModel?.type === '3d' || activeModel?.format === 'vrm' || activeModel?.format === 'fbx') return '3D';
  if (activeModel?.type === '2d') return '2D';

  return '3D';
};

const normalizeModelConfig = (config: any) => {
  const activeModel = config?.ui?.activeModel ?? config?.activeModel ?? config?.avatar?.activeModel;
  const avatar = config?.avatar ?? {};

  if (activeModel?.filename) return activeModel;

  const candidate = avatar.vrmModel || avatar.live2dModel;
  if (candidate) {
    const lower = String(candidate).toLowerCase();
    return {
      filename: candidate,
      type: lower.includes('/live2d/') ? '2d' : '3d',
      format: lower.endsWith('.fbx') ? 'fbx' : lower.endsWith('.vrm') ? 'vrm' : 'json',
    };
  }

  return DEFAULT_WIDGET_MODEL;
};

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

// ═══════════════════════════════════════════════════════
//  Chat State
// ═══════════════════════════════════════════════════════
const isThinking = ref(false);
const inputText = ref("");
const isCollapsed = ref(true);

// Theme Toggle
const isLightMode = ref(globalThis.localStorage?.getItem("theme") === "light");
const toggleTheme = () => {
  isLightMode.value = !isLightMode.value;
  const newTheme = isLightMode.value ? "light" : "dark";
  globalThis.document?.documentElement.setAttribute("data-theme", newTheme);
  globalThis.document?.body.setAttribute("data-theme", newTheme);
  globalThis.localStorage?.setItem("theme", newTheme);
};

import { useI18n } from "./composables/useI18n";
import { useGateway } from "./composables/useGateway";
const { t } = useI18n();
const gateway = useGateway();

const messages = shallowRef<{ role: "user" | "assistant"; text: string; thinking?: string }[]>([
  {
    role: "assistant",
    text: t('welcome_liva'),
  },
]);
const chatContainer = ref<HTMLElement | null>(null);

const startNewChat = () => {
  messages.value = [
    {
      role: "assistant",
      text: t('welcome_liva'),
    },
  ];
  triggerRef(messages);
  stopQueuedAudio(true);
  if (isCollapsed.value) {
    toggleCollapse();
  }
};
const isSensing = ref(false);
const isCameraActive = ref(false);

// ═══════════════════════════════════════════════════════
//  Chat UI Dragging Logic
// ═══════════════════════════════════════════════════════
const dragOffset = ref({ x: 0, y: 0 });
const isDragging = ref(false);
let startMousePos = { x: 0, y: 0 };
let startDragOffset = { x: 0, y: 0 };

const onDragMove = (e: MouseEvent) => {
  if (!isDragging.value) return;
  const nextX = startDragOffset.x + (e.clientX - startMousePos.x);
  const nextY = startDragOffset.y + (e.clientY - startMousePos.y);
  const maxX = Math.max(window.innerWidth - 120, 0);
  const maxY = Math.max(window.innerHeight - 120, 0);
  dragOffset.value = {
    x: Math.min(Math.max(nextX, -maxX), maxX),
    y: Math.min(Math.max(nextY, -maxY), maxY),
  };
};

const onDragEnd = () => {
  isDragging.value = false;
  globalThis.document.removeEventListener('mousemove', onDragMove);
  globalThis.document.removeEventListener('mouseup', onDragEnd);

  const currentWidth = isCollapsed.value ? 48 : 400;
  const naturalLeft = window.innerWidth - 16 - currentWidth;
  const currentCenterX = naturalLeft + dragOffset.value.x + currentWidth / 2;
  snapPosition.value = currentCenterX < window.innerWidth / 2 ? 'left' : 'right';

  const currentAbsoluteY = window.innerHeight - 60 + dragOffset.value.y;
  verticalSnapPosition.value = currentAbsoluteY < window.innerHeight / 2 ? 'top' : 'bottom';

  if (isCollapsed.value) {
    snapToEdge();
  }
};

const onDragStart = (e: MouseEvent) => {
  isDragging.value = true;
  startMousePos = { x: e.clientX, y: e.clientY };
  startDragOffset = { ...dragOffset.value };
  globalThis.document.addEventListener('mousemove', onDragMove);
  globalThis.document.addEventListener('mouseup', onDragEnd);
};

// ═══════════════════════════════════════════════════════
//  Voice Input (Microphone → STT)
// ═══════════════════════════════════════════════════════
const voice = useVoicePipeline();
const volumeLevel = voice.volumeLevel;
const isListening = computed(() => voice.state.value === 'ACTIVE');

// ═══════════════════════════════════════════════════════
//  Wake Word Detection Sound (Web Audio API)
// ═══════════════════════════════════════════════════════
let wakeWordAudioCtx: AudioContext | null = null;

function playWakeWordSound() {
  try {
    if (!wakeWordAudioCtx) {
      const AudioContextCls = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
      wakeWordAudioCtx = new AudioContextCls();
    }
    if (wakeWordAudioCtx.state === 'suspended') {
      wakeWordAudioCtx.resume();
    }

    const playTone = (freq: number, startTime: number, duration: number) => {
      const oscillator = wakeWordAudioCtx!.createOscillator();
      const gainNode = wakeWordAudioCtx!.createGain();

      oscillator.connect(gainNode);
      gainNode.connect(wakeWordAudioCtx!.destination);

      oscillator.type = 'sine';
      oscillator.frequency.value = freq;

      gainNode.gain.setValueAtTime(0, startTime);
      gainNode.gain.linearRampToValueAtTime(0.3, startTime + 0.02);
      gainNode.gain.exponentialRampToValueAtTime(0.001, startTime + duration);

      oscillator.start(startTime);
      oscillator.stop(startTime + duration);
    };

    const now = wakeWordAudioCtx.currentTime;
    // Siri-like double chime
    playTone(415.30, now, 0.15);       // G#4
    playTone(554.37, now + 0.15, 0.2); // C#5

  } catch (err) {
    logger.warn('[Widget]', 'Could not play wake word sound:', err);
  }
}

// ═══════════════════════════════════════════════════════
//  Wake Word Detection ("Hey Liva" → auto-activate voice)
//  [v25 Pillar 4] Using ONNX WASM for local inference
// ═══════════════════════════════════════════════════════
voice.onWakeWordDetected(() => {
  logger.info('[Widget]', 'Wake Word detected!');

  // Play acknowledgment sound (Siri double-chime)
  playWakeWordSound();

  // Add visual feedback
  messages.value = [...messages.value, { role: "assistant", text: t('wg_wake_word_ack') }];
  triggerRef(messages);
  scrollToBottom();
});

// Camera frame capture interval (send to AI every 10s)
let frameCaptureInterval: ReturnType<typeof setInterval> | null = null;

// ═══════════════════════════════════════════════════════
//  WebSocket
// ═══════════════════════════════════════════════════════
let ws: WebSocket | null = null;

// ═══════════════════════════════════════════════════════
//  Audio Queue
// ═══════════════════════════════════════════════════════
let audioCtx: AudioContext | null = null;
let nextAudioTime = 0;
let activeAudioSources: AudioBufferSourceNode[] = [];
let audioQueueEpoch = 0;
let isAudioPlaybackBlocked = false;

const removeAudioSource = (source: AudioBufferSourceNode) => {
  activeAudioSources = activeAudioSources.filter((item) => item !== source);
};

const stopQueuedAudio = (blockIncomingChunks = true) => {
  if (blockIncomingChunks) {
    isAudioPlaybackBlocked = true;
  }

  audioQueueEpoch++;
  const sources = activeAudioSources;
  activeAudioSources = [];

  for (const source of sources) {
    try {
      source.stop();
    } catch {
      // Source may already have ended or may not have reached its scheduled start.
    }
  }

  if (engineRef.value?.stopAudioLipSync) {
    engineRef.value.stopAudioLipSync();
  }

  nextAudioTime = audioCtx ? audioCtx.currentTime : 0;
};

// ═══════════════════════════════════════════════════════
//  Engine ref for triggering motions
// ═══════════════════════════════════════════════════════
const engineRef = ref<any>(null);

// ═══════════════════════════════════════════════════════
//  Platform Bridge (Agnostic IPC)
// ═══════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════
//  Phantom Bounding Box Fix — Rust Cursor Hit-Test System
//  Rust polls cursor position every 30ms and toggles ghost mode
//  based on whether cursor is inside interactive zones.
//  We report the bounding rects of interactive elements to Rust.
// ═══════════════════════════════════════════════════════
const chatUIRef = ref<HTMLElement | null>(null);
const miniIconsRef = ref<HTMLElement | null>(null);
let zonesInterval: ReturnType<typeof setInterval> | null = null;

const updateInteractiveZones = () => {
  if (!platform) return;
  const zones: Array<{ x: number; y: number; width: number; height: number }> = [];

  // 1. Measure chat capsule/bar
  if (chatUIRef.value) {
    const rect = chatUIRef.value.getBoundingClientRect();
    zones.push({
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
    });
  }

  // 2. Measure messages container if visible
  if (!isCollapsed.value && chatContainer.value) {
    const rect = chatContainer.value.getBoundingClientRect();
    zones.push({
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
    });
  }

  // 3. Measure mini icons container if visible
  if (miniIconsRef.value) {
    const rect = miniIconsRef.value.getBoundingClientRect();
    zones.push({
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
    });
  }

  platform.invokeBackend("update_interactive_zones", { zones }).catch((err) => {
    logger.warn("[Widget] Failed to update interactive zones:", err);
  });
};

watch([isCollapsed, isDragging, () => messages.value.length], () => {
  nextTick(() => {
    updateInteractiveZones();
  });
}, { deep: true });


// ═══════════════════════════════════════════════════════
//  Collapse & Snap Logic
// ═══════════════════════════════════════════════════════
const snapPosition = ref('right');
const verticalSnapPosition = ref('bottom');

const snapToEdge = () => {
  const collapsedWidth = 48; // w-12 is 48px
  const naturalLeft = window.innerWidth - 16 - collapsedWidth; 
  const currentCenterX = naturalLeft + dragOffset.value.x + collapsedWidth / 2;
  
  if (currentCenterX < window.innerWidth / 2) {
    snapPosition.value = 'left';
    dragOffset.value.x = 16 - naturalLeft;
  } else {
    snapPosition.value = 'right';
    dragOffset.value.x = 0;
  }
};

const toggleCollapse = () => {
  isCollapsed.value = !isCollapsed.value;
  const currentAbsoluteY = window.innerHeight - 60 + dragOffset.value.y;
  verticalSnapPosition.value = currentAbsoluteY < window.innerHeight / 2 ? 'top' : 'bottom';
};

// ═══════════════════════════════════════════════════════
//  Thinking → trigger avatar motion
// ═══════════════════════════════════════════════════════
watch(isThinking, (val) => {
  if (val && engineRef.value?.triggerMotion) {
    engineRef.value.triggerMotion();
  }
});

// Watch camera state from engine
watch(() => engineRef.value?.isCameraOn?.value, (val) => {
  isCameraActive.value = !!val;
  if (val) {
    startFrameCapture();
  } else {
    stopFrameCapture();
  }
});

// ═══════════════════════════════════════════════════════
//  Camera Frame Capture → AI Vision
// ═══════════════════════════════════════════════════════

/** Send webcam frame to Gateway every 10s for AI multimodal processing */
function startFrameCapture() {
  if (frameCaptureInterval) return;
  frameCaptureInterval = setInterval(() => {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    if (!engineRef.value?.captureFrameForAI) return;

    const frame = engineRef.value.captureFrameForAI();
    if (frame) {
      ws.send(JSON.stringify({
        event: "camera_frame",
        payload: { image: frame, timestamp: Date.now() },
      }));
    }
  }, 10000); // Every 10 seconds
}

function stopFrameCapture() {
  if (frameCaptureInterval) {
    clearInterval(frameCaptureInterval);
    frameCaptureInterval = null;
  }
}

const scrollToBottom = async () => {
  await nextTick();
  if (chatContainer.value) {
    chatContainer.value.scrollTop = chatContainer.value.scrollHeight;
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
    setTimeout(() => { isSensing.value = false; }, 30000);
  }
};

// ═══════════════════════════════════════════════════════
//  Voice Toggle (Push-to-talk)
//  When PTT starts → pause wake word (audio goes to full STT)
//  When PTT stops → restart wake word ("Hey Liva" listens again)
// ═══════════════════════════════════════════════════════
const toggleVoice = () => {
  voice.toggleVoice();
};

// Interrupt: if user clicks mic while LIVA is speaking
const interruptLIVA = () => {
  stopQueuedAudio();

  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send('[INTERRUPT]');
  }
};

// ═══════════════════════════════════════════════════════
//  Send Message
// ═══════════════════════════════════════════════════════
const sendMessage = () => {
  if (!inputText.value.trim() || !ws || ws.readyState !== WebSocket.OPEN) return;

  stopQueuedAudio();

  const text = inputText.value.trim();
  messages.value = [...messages.value, { role: "user", text }];
  triggerRef(messages);

  ws.send(JSON.stringify({
    event: "user_voice_command",
    payload: { text },
  }));

  inputText.value = "";
  scrollToBottom();
};

// ═══════════════════════════════════════════════════════
//  Open Dashboard
// ═══════════════════════════════════════════════════════
const openDashboard = () => {
  if (platform) platform.invokeBackend('open_dashboard');
};

// ═══════════════════════════════════════════════════════
//  Lifecycle
// ═══════════════════════════════════════════════════════
onMounted(() => {
  globalThis.addEventListener("keydown", handleKeydown);

  // Initialize theme properly on mount so the first click doesn't bug out
  const initialTheme = isLightMode.value ? "light" : "dark";
  globalThis.document?.documentElement.setAttribute("data-theme", initialTheme);
  globalThis.document?.body.setAttribute("data-theme", initialTheme);

  const hw = profileHardware();
  hardwareInfo.value = `GPU=${hw.gpu}; RAM=${hw.ram}GB; Cores=${hw.cores}; WebGL=${hw.webglVersion}; MaxTex=${hw.maxTextureSize}; Recommended=${hw.recommendedEngine}`;
  logger.info('[Widget]', 'Hardware profile detected', hw);

  // 1. Auto-detect engine và lazy load
  // Ưu tiên cấu hình người dùng từ Dashboard nếu có, fallback theo hardware
  engineMode.value = '3D';
  activeModelConfig.value = DEFAULT_WIDGET_MODEL;
  activeEngine.value = VRMEngine;
  engineStatus.value = 'forced-3d-bootstrap';
  logger.info('[Widget]', 'Initial engine forced to 3D for diagnostics');

  // 2. Mặc định xuyên chuột (Ghost Mode) - Rust will handle this dynamically.
  // We trigger the initial update and start a 150ms periodic check to sync coords.
  nextTick(() => {
    updateInteractiveZones();
  });
  zonesInterval = setInterval(updateInteractiveZones, 150);

  // Expose global helper for clickable bubble buttons
  (window as any).sendLIVAMessage = (text: string) => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      stopQueuedAudio();
      messages.value = [...messages.value, { role: "user", text }];
      triggerRef(messages);
      ws.send(JSON.stringify({
        event: "user_voice_command",
        payload: { text },
      }));
      scrollToBottom();
    }
  };

  // 3. Connect WebSocket
  // Connect directly because the Tauri event might fire before this component mounts.
  const port = 8082;
  const wsUrl = `ws://127.0.0.1:${port}`;
  ws = new WebSocket(wsUrl);
  
  ws.onopen = () => {
    logger.info('[Widget]', `WSS Connected to Gateway on port ${port}`);
    engineStatus.value = 'websocket-open';
    ws?.send(JSON.stringify({ event: "get_config" }));
    ws?.send(JSON.stringify({ event: "get_avatar_models" }));
    ws?.send(JSON.stringify({ event: "get_user_profile" }));
    if (ws) {
      voice.startPipeline(ws).catch((e: unknown) =>
        logger.warn('[Widget]', 'Voice pipeline start failed:', e instanceof Error ? e.message : String(e))
      );
    }
  };

  ws.onmessage = async (event) => {
        try {
          if (typeof event.data === "string" && event.data.trim() === "[INTERRUPT]") {
            stopQueuedAudio();
            return;
          }

          const data = JSON.parse(event.data);

          if (data.event === "config_data" || data.event === "config_updated") {
            const conf = data.payload || data;
            applyWidgetConfig(conf, data.event);
          } else if (data.event === "user_profile" || data.event === "profile_updated_success") {
            // Sync user profile (language, tone, etc.) to shared Gateway state
            // so useI18n reactive computed picks up the language change instantly
            if (data.payload) {
              gateway.userProfile.value = data.payload;
            }
          } else if (data.event === "eco_mode_changed") {
            const enabled = !!data.payload?.enabled;
            (window as any).LIVA_ECO_MODE = enabled;
            logger.info('[Widget]', `Eco Mode status changed: ${enabled}. Throttling avatar renderer.`);
          } else if (data.event === "debug_log") {
            logger.info('[Widget]', 'Gateway debug', data.payload ?? data);
          } else if (data.event === "ai_thinking_start") {
            isThinking.value = true;
            stopQueuedAudio();
            scrollToBottom();
            if (voice.state.value === 'ACTIVE') voice.state.value = 'PROCESSING';
          } else if (data.event === "ai_thinking_end") {
            isThinking.value = false;
          } else if (data.event === "ai_stream_start") {
            isAudioPlaybackBlocked = false;
            isThinking.value = false;
            
            // 1. Find and filter out any existing assistant message containing thinking/skills content
            let thinkingText = "";
            const filteredMsgs = messages.value.filter(msg => {
                const isThinkingMsg = msg.role === "assistant" && (
                    msg.text.includes("sys-thinking-flag") || 
                    msg.text.includes("sys-skill-flag") ||
                    msg.text.includes("LIVA đang") || 
                    msg.text.includes("Identify Tool") || 
                    msg.text.includes("Determine Parameters") ||
                    msg.text.includes("Execute Tool Call") ||
                    msg.thinking
                );
                if (isThinkingMsg) {
                    thinkingText = msg.thinking || msg.text;
                    return false; // Remove this intermediate thinking bubble from history
                }
                return true;
            });

            // 2. Extract clean thinking text to store in the structured field
            let cleanThinking = "";
            if (thinkingText) {
                cleanThinking = thinkingText
                    .replace(/<br\s*\/?>/gi, "\n")
                    .replace(/<[^>]+>/g, "") // strip HTML tags
                    .trim();
            }

            messages.value = [...filteredMsgs, { role: "assistant", text: "", thinking: cleanThinking || undefined }];
            triggerRef(messages);
            scrollToBottom();
          } else if (data.event === "ai_stream_chunk") {
            if (messages.value.length > 0) {
              let chunk = data.payload.textChunk as string;
              chunk = chunk.replace(/\[\[SYS_THINKING\]\]/g, t('sys_thinking'));
              chunk = chunk.replace(/\[\[SYS_USING_SKILL\]\]/g, t('sys_using_skill'));
              
              const emotionMatch = chunk.match(/^\[(happy|sad|angry|surprised|neutral|relaxed)\]/);
              if (emotionMatch) {
                const emotion = emotionMatch[1];
                chunk = chunk.replace(/^\[(.*?)\]/, '');
                if (engineRef.value?.setExpression) {
                  engineRef.value.setExpression(emotion);
                }
              }
              chunk = chunk.replace(/\n/g, "<br/>");
              messages.value[messages.value.length - 1].text += chunk;
              triggerRef(messages);
              scrollToBottom();
            }
          } else if (data.event === "ai_spoken_response") {
            isAudioPlaybackBlocked = false;
            isThinking.value = false;
            if (activeAudioSources.length === 0 && voice.state.value === 'PROCESSING') voice.state.value = 'PASSIVE';
            
            let finalReply = data.payload.text.replace(/\n/g, "<br/>");
            
            // Clean up any remaining thinking bubbles if any got past the stream_start phase
            let thinkingText = "";
            const filteredMsgs = messages.value.filter(msg => {
                const isThinkingMsg = msg.role === "assistant" && (
                    msg.text.includes("sys-thinking-flag") || 
                    msg.text.includes("sys-skill-flag") ||
                    msg.text.includes("LIVA đang") || 
                    msg.text.includes("Identify Tool") || 
                    msg.text.includes("Determine Parameters") ||
                    msg.text.includes("Execute Tool Call") ||
                    msg.thinking
                );
                if (isThinkingMsg && !msg.thinking) {
                    thinkingText = msg.text;
                    return false;
                }
                return true;
            });

            const lastMsg = filteredMsgs[filteredMsgs.length - 1];
            if (lastMsg && lastMsg.role === "assistant") {
                lastMsg.text = finalReply;
                if (thinkingText) {
                    lastMsg.thinking = thinkingText
                        .replace(/<br\s*\/?>/gi, "\n")
                        .replace(/<[^>]+>/g, "")
                        .trim();
                }
                messages.value = [...filteredMsgs];
            } else {
                let cleanThinking = "";
                if (thinkingText) {
                    cleanThinking = thinkingText
                        .replace(/<br\s*\/?>/gi, "\n")
                        .replace(/<[^>]+>/g, "")
                        .trim();
                }
                messages.value = [...filteredMsgs, { role: "assistant", text: finalReply, thinking: cleanThinking || undefined }];
            }
            triggerRef(messages);
            scrollToBottom();
          } else if (data.event === "ai_audio_chunk") {
            if (isAudioPlaybackBlocked) return;
            try {
              if (!audioCtx) {
                const AudioContextCls = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
                audioCtx = new AudioContextCls();
              }
              if (audioCtx.state === 'suspended') await audioCtx.resume();

              const queueEpoch = audioQueueEpoch;
              const binaryStr = atob(data.payload.audio);
              const bytes = new Uint8Array(binaryStr.length);
              for (let i = 0; i < binaryStr.length; i++) bytes[i] = binaryStr.codePointAt(i) as number;

              const audioBuffer = await audioCtx.decodeAudioData(bytes.buffer);
              if (queueEpoch !== audioQueueEpoch || isAudioPlaybackBlocked) return;

              const source = audioCtx.createBufferSource();
              source.buffer = audioBuffer;
              source.connect(audioCtx.destination);
              source.onended = () => {
                removeAudioSource(source);
                if (activeAudioSources.length === 0 && engineRef.value?.stopAudioLipSync) {
                  engineRef.value.stopAudioLipSync();
                }
                if (activeAudioSources.length === 0 && !isThinking.value && voice.state.value === 'PROCESSING') {
                  voice.state.value = 'PASSIVE';
                }
              };

              const overlap = 0.1;
              const currentTime = audioCtx.currentTime;
              if (nextAudioTime < currentTime) nextAudioTime = currentTime;
              activeAudioSources.push(source);
              source.start(nextAudioTime);
              nextAudioTime += (audioBuffer.duration - overlap);

              if (engineRef.value?.startAudioLipSync && audioCtx) {
                engineRef.value.startAudioLipSync(audioCtx, source);
              }
            } catch (audioErr: unknown) {
              logger.warn('[Widget]', 'Audio decode/playback error:', audioErr instanceof Error ? audioErr.message : String(audioErr));
            }
          }
        } catch (parseErr: unknown) {
          logger.warn('[Widget]', 'WebSocket message parse error:', parseErr instanceof Error ? parseErr.message : String(parseErr));
        }
      };

  // 4. Listen for avatar/config hot-swap from Dashboard (Handled via WebSocket instead of IPC)

  if (ws) {
    engineStatus.value = 'websocket-connecting';
  }


});

onUnmounted(() => {
  globalThis.removeEventListener("keydown", handleKeydown);
  if (ws) { ws.close(); ws = null; }
  stopQueuedAudio();
  if (audioCtx) { audioCtx.close(); audioCtx = null; }
  stopFrameCapture();
  voice.stopPipeline();
  if (zonesInterval) {
    clearInterval(zonesInterval);
    zonesInterval = null;
  }
});

onActivated(() => {
  // Widget became visible again — restart frame capture if camera was active
  if (isCameraActive.value) {
    startFrameCapture();
  }
});

onDeactivated(() => {
  // Widget hidden by KeepAlive — pause frame capture to save CPU
  stopFrameCapture();
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
              'px-4 py-2.5 rounded-[22px] text-sm max-w-[85%] leading-relaxed flex flex-col gap-1 shadow-sm',
              msg.role === 'user'
                ? 'self-end bg-gradient-to-r from-purple-600 to-blue-500 text-white rounded-br-[6px]'
                : 'self-start chat-bubble-ai rounded-bl-[6px]'
            ]"
          >
            <details v-if="msg.thinking" class="thinking-details mb-2 select-none opacity-80 w-full" style="outline: none;">
              <summary class="text-xs text-purple-400 hover:text-purple-300 font-semibold focus:outline-none cursor-pointer flex items-center gap-1">💭 {{ t('thinking_details') }}</summary>
              <div class="mt-1 pl-2 border-l border-purple-500/30 text-xs text-gray-400/80 leading-relaxed whitespace-pre-line">{{ msg.thinking }}</div>
            </details>
            <div v-if="msg.text" v-html="msg.text" class="w-full"></div>
          </div>
        </template>
        <!-- Thinking indicator -->
        <div v-if="isThinking" class="self-start chat-bubble-ai px-4 py-2.5 rounded-[22px] rounded-bl-[6px] text-sm flex items-center gap-1">
          <span class="thinking-dot" style="animation-delay: 0s">●</span>
          <span class="thinking-dot" style="animation-delay: 0.2s">●</span>
          <span class="thinking-dot" style="animation-delay: 0.4s">●</span>
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
          @keyup.enter="sendMessage"
          type="text"
          :placeholder="t('wg_placeholder')"
          class="chat-input flex-1 bg-transparent border-none pl-1 pr-4 focus:outline-none w-full"
        />
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
              <path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.99l1.005.828c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z" />
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

      <!-- Compact Collapsed State -->
      <div v-else class="chat-capsule w-12 h-12 flex items-center justify-center relative rounded-full shadow-lg ml-auto">
        <!-- Outer Drag Ring -->
        <div 
          class="absolute inset-0 rounded-full border-[2px] border-white/20 hover:border-white/50 cursor-move transition-colors z-10"
          @mousedown.stop="onDragStart"
          :title="t('wg_drag')"
        ></div>
        
        <!-- Expand Button -->
        <button
          @mousedown.stop
          @click="toggleCollapse"
          class="chat-icon-btn bg-transparent border-none outline-none w-9 h-9 rounded-full flex justify-center items-center hover:bg-white/10 transition-colors z-20"
          :title="t('wg_collapse')"
        >
          <svg v-if="snapPosition === 'left'" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2.5" stroke="currentColor" class="w-5 h-5 ml-0.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
          </svg>
          <svg v-else xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2.5" stroke="currentColor" class="w-5 h-5 mr-0.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 19.5L8.25 12l7.5-7.5" />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<style>
@keyframes blob {
  0% { transform: translate(0px, 0px) scale(1); }
  33% { transform: translate(30px, -50px) scale(1.1); }
  66% { transform: translate(-20px, 20px) scale(0.9); }
  100% { transform: translate(0px, 0px) scale(1); }
}
.animate-blob { animation: blob 7s infinite; }
.animation-delay-2000 { animation-delay: 2s; }

@keyframes fadeInUp {
  from { opacity: 0; transform: translateY(20px); }
  to { opacity: 1; transform: translateY(0); }
}
.animate-fade-in-up { animation: fadeInUp 0.6s ease-out forwards; }

.scrollbar-hide::-webkit-scrollbar { display: none; }
.scrollbar-hide { -ms-overflow-style: none; scrollbar-width: none; }

/* Thinking dots animation */
@keyframes thinkingPulse {
  0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
  40% { opacity: 1; transform: scale(1.2); }
}
.thinking-dot {
  animation: thinkingPulse 1.4s infinite ease-in-out;
  font-size: 8px;
}

/* Voice button */
.voice-btn {
  position: relative;
  z-index: 1;
}
.voice-ring {
  position: absolute;
  inset: -2px;
  width: calc(100% + 4px);
  height: calc(100% + 4px);
  pointer-events: none;
  transition: stroke-dasharray 0.1s ease;
}

/* Recording pulse */
@keyframes recPulse {
  0%, 100% { box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.4); }
  50% { box-shadow: 0 0 0 6px rgba(239, 68, 68, 0); }
}
.voice-btn.bg-red-500\/30 {
  animation: recPulse 1.5s infinite;
}

.hardware-badge {
  position: absolute;
  left: 16px;
  bottom: 16px;
  max-width: 420px;
  padding: 8px 12px;
  border-radius: 12px;
  background: rgba(0, 0, 0, 0.45);
  color: rgba(255, 255, 255, 0.9);
  font-size: 11px;
  line-height: 1.4;
  pointer-events: none;
  backdrop-filter: blur(8px);
  z-index: 20;
}

.engine-badge {
  position: absolute;
  left: 16px;
  bottom: 66px;
  padding: 6px 10px;
  border-radius: 10px;
  background: rgba(21, 128, 61, 0.45);
  color: rgba(240, 253, 244, 0.95);
  font-size: 11px;
  pointer-events: none;
  backdrop-filter: blur(8px);
  z-index: 20;
}

/* Premium HITL Action Buttons */
.hitl-container {
  display: flex;
  gap: 10px;
  margin-top: 12px;
  width: 100%;
}
.hitl-btn {
  flex: 1;
  padding: 8px 16px;
  border-radius: 12px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}
.hitl-btn-approve {
  background: linear-gradient(135deg, #a855f7 0%, #3b82f6 100%);
  color: white;
}
.hitl-btn-approve:hover {
  transform: translateY(-1px);
  box-shadow: 0 6px 16px rgba(168, 85, 247, 0.4);
}
.hitl-btn-approve:active {
  transform: translateY(1px);
}
.hitl-btn-reject {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(239, 68, 68, 0.4);
  color: #ef4444;
}
.hitl-btn-reject:hover {
  background: rgba(239, 68, 68, 0.15);
  transform: translateY(-1px);
  box-shadow: 0 6px 16px rgba(239, 68, 68, 0.25);
}
.hitl-btn-reject:active {
  transform: translateY(1px);
}
</style>
