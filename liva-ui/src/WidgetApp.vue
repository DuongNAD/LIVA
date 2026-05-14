<script setup lang="ts">
/**
 * WidgetApp.vue — Hybrid Dual-Engine Widget
 * ==========================================
 * Entry component cho cửa sổ widget (transparent overlay).
 * - Auto-detect GPU → lazy load đúng engine (Live2D hoặc VRM)
 * - Zero cross-contamination: engine không dùng = 0 bytes RAM
 * - Phantom Bounding Box Fix (Phương án 1: pointer-events + IPC)
 */
import { ref, shallowRef, triggerRef, defineAsyncComponent, onMounted, onUnmounted, nextTick, watch } from "vue";
import { detectOptimalEngine, type EngineMode } from "./utils/HardwareDetector";
import { useMicrophone } from "./composables/useMicrophone";
import { useWakeWord } from "./composables/useWakeWord";

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
const engineMode = ref<EngineMode>('2D');
const activeModelConfig = ref<any>(null);

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
const messages = shallowRef<{ role: "user" | "assistant"; text: string }[]>([
  {
    role: "assistant",
    text: "Xin chào! Mình là LIVA. Hệ thống đã sẵn sàng phục vụ bạn!",
  },
]);
const chatContainer = ref<HTMLElement | null>(null);

const startNewChat = () => {
  messages.value = [
    {
      role: "assistant",
      text: "Xin chào! Mình là LIVA. Hệ thống đã sẵn sàng phục vụ bạn!",
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
let isHovered = false;
let startMousePos = { x: 0, y: 0 };
let startDragOffset = { x: 0, y: 0 };

const onDragMove = (e: MouseEvent) => {
  if (!isDragging.value) return;
  dragOffset.value = {
    x: startDragOffset.x + (e.clientX - startMousePos.x),
    y: startDragOffset.y + (e.clientY - startMousePos.y),
  };
};

const onDragEnd = () => {
  isDragging.value = false;
  globalThis.document.removeEventListener('mousemove', onDragMove);
  globalThis.document.removeEventListener('mouseup', onDragEnd);
  if (!isHovered && electronAPI) {
    electronAPI.setIgnoreMouse(true);
  }
  
  // Dynamically determine side of screen
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
const { isListening, volumeLevel, startListening, stopListening } = useMicrophone();

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

    // Play a short "ding" sound (sine wave at 880Hz for 100ms)
    const oscillator = wakeWordAudioCtx.createOscillator();
    const gainNode = wakeWordAudioCtx.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(wakeWordAudioCtx.destination);

    oscillator.type = 'sine';
    oscillator.frequency.value = 880; // A5 note

    // Envelope: quick attack, quick decay
    gainNode.gain.setValueAtTime(0, wakeWordAudioCtx.currentTime);
    gainNode.gain.linearRampToValueAtTime(0.3, wakeWordAudioCtx.currentTime + 0.01);
    gainNode.gain.exponentialRampToValueAtTime(0.001, wakeWordAudioCtx.currentTime + 0.15);

    oscillator.start(wakeWordAudioCtx.currentTime);
    oscillator.stop(wakeWordAudioCtx.currentTime + 0.15);
  } catch (err) {
    console.warn('[Widget] Could not play wake word sound:', err);
  }
}

// ═══════════════════════════════════════════════════════
//  Wake Word Detection ("Hey Liva" → auto-activate voice)
//  [v25 Pillar 4] Using ONNX WASM for local inference
// ═══════════════════════════════════════════════════════
const wakeWord = useWakeWord();

// Wake word detection callback
wakeWord.onWakeWordDetected(async (_trailingText: string) => {
  console.log(`[Widget] Wake Word detected!`);

  // Play acknowledgment sound
  playWakeWordSound();

  // Stop wake word mic → switch to full push-to-talk voice mode
  await wakeWord.stopWakeWord();

  // Activate voice mode so user can speak
  if (ws && ws.readyState === WebSocket.OPEN) {
    await startListening(ws);
  }
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
//  Electron API (via preload.cjs contextBridge)
// ═══════════════════════════════════════════════════════
const electronAPI = (globalThis as any).electronAPI;

// ═══════════════════════════════════════════════════════
//  Phantom Bounding Box Fix (Phương án 1: Ghost Mode)
// ═══════════════════════════════════════════════════════
const enableMouse = () => {
  isHovered = true;
  if (electronAPI) electronAPI.setIgnoreMouse(false);
};
const disableMouse = () => {
  isHovered = false;
  if (isDragging.value) return; // Prevent losing mouse capture during drag
  if (electronAPI) electronAPI.setIgnoreMouse(true);
};

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
  if (!isCollapsed.value) {
    isCollapsed.value = true;
    
    const collapsedWidth = 48;
    const fullWidth = 400; // max-w
    const naturalLeftCollapsed = window.innerWidth - 16 - collapsedWidth;
    const currentAbsoluteLeft = (window.innerWidth - 16 - fullWidth) + dragOffset.value.x;
    const currentCenterX = currentAbsoluteLeft + fullWidth / 2;
    
    if (currentCenterX < window.innerWidth / 2) {
      snapPosition.value = 'left';
      dragOffset.value.x = 16 - naturalLeftCollapsed;
    } else {
      snapPosition.value = 'right';
      dragOffset.value.x = 0;
    }
  } else {
    isCollapsed.value = false;
    if (snapPosition.value === 'left') {
      const fullWidth = 400;
      const naturalLeftFull = window.innerWidth - 16 - fullWidth;
      dragOffset.value.x = 16 - naturalLeftFull;
    } else {
      dragOffset.value.x = 0;
    }
  }
  
  // Re-evaluate vertical position after toggle
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
      await fetch("http://127.0.0.1:3000/api/sensory-capture", { method: "POST" });
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
const toggleVoice = async () => {
  if (isListening.value) {
    stopListening();
    // Resume wake word detection after PTT ends
    if (ws && ws.readyState === WebSocket.OPEN) {
      wakeWord.setWebSocket(ws);
      wakeWord.startWakeWord().catch(() => {});
    }
  } else {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    // Stop wake word while using full PTT mode
    await wakeWord.stopWakeWord();
    await startListening(ws);
  }
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
  if (electronAPI) electronAPI.openDashboard();
};

// ═══════════════════════════════════════════════════════
//  Lifecycle
// ═══════════════════════════════════════════════════════
onMounted(() => {
  globalThis.addEventListener("keydown", handleKeydown);

  // 1. Auto-detect engine và lazy load
  // TODO: đọc preference từ config qua WebSocket, mặc định auto
  engineMode.value = detectOptimalEngine('auto');
  activeEngine.value = engineMode.value === '3D' ? VRMEngine : Live2DEngine;

  // 2. Mặc định xuyên chuột (Ghost Mode)
  disableMouse();

  // 3. Connect WebSocket
  ws = new WebSocket("ws://127.0.0.1:8082");
  ws.onopen = () => {
    console.log("[Widget] WSS Connected to Gateway");
    ws?.send(JSON.stringify({ event: "get_config" }));
    // Set WebSocket reference for Wake Word Worker
    if (ws) {
      wakeWord.setWebSocket(ws);
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
        if (conf.ui && conf.ui.avatarMode) {
          const mode = conf.ui.avatarMode;
          if (mode !== 'auto') {
            engineMode.value = mode;
          } else {
            if (conf.ui.activeModel && conf.ui.activeModel.type) {
              engineMode.value = conf.ui.activeModel.type === '3d' ? '3D' : '2D';
            } else {
              engineMode.value = detectOptimalEngine('auto');
            }
          }
          if (conf.ui.activeModel) {
            activeModelConfig.value = conf.ui.activeModel;
          }
          activeEngine.value = engineMode.value === '3D' ? VRMEngine : Live2DEngine;
        }
      } else if (data.event === "ai_thinking_start") {
        isThinking.value = true;
        stopQueuedAudio();
        scrollToBottom();
      } else if (data.event === "ai_thinking_end") {
        isThinking.value = false;
      } else if (data.event === "ai_stream_start") {
        isAudioPlaybackBlocked = false;
        isThinking.value = false;
        messages.value = [...messages.value, { role: "assistant", text: "" }];
        triggerRef(messages);
        scrollToBottom();
      } else if (data.event === "ai_stream_chunk") {
        if (messages.value.length > 0) {
          let chunk = data.payload.textChunk as string;

          // LLM Emotion Tag Parsing: [happy], [sad], [angry], [surprised], [neutral]
          const emotionMatch = chunk.match(/^\[(happy|sad|angry|surprised|neutral|relaxed)\]/);
          if (emotionMatch) {
            const emotion = emotionMatch[1];
            chunk = chunk.replace(/^\[(.*?)\]/, ''); // Strip tag from display
            if (engineRef.value?.setExpression) {
              engineRef.value.setExpression(emotion);
            }
          }

          messages.value[messages.value.length - 1].text += chunk;
          triggerRef(messages); // Only trigger update, don't reallocate array
          scrollToBottom();
        }
      } else if (data.event === "ai_spoken_response") {
        isAudioPlaybackBlocked = false;
        isThinking.value = false;
        const lastMsg = messages.value[messages.value.length - 1];
        if (lastMsg && lastMsg.role === "assistant" && lastMsg.text.length > 0
            && data.payload.text.includes(lastMsg.text.trim())) {
          lastMsg.text = data.payload.text;
          triggerRef(messages);
        } else if (!lastMsg || lastMsg.role === "user") {
          messages.value = [...messages.value, { role: "assistant", text: data.payload.text }];
          triggerRef(messages);
        }
        scrollToBottom();
      } else if (data.event === "ai_audio_chunk") {
        if (isAudioPlaybackBlocked) return;

        // Audio playback (base64 MP3 from voice_engine)
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
          };

          const overlap = 0.1;
          const currentTime = audioCtx.currentTime;
          if (nextAudioTime < currentTime) nextAudioTime = currentTime;
          activeAudioSources.push(source);
          source.start(nextAudioTime);
          nextAudioTime += (audioBuffer.duration - overlap);

          // Audio-driven lip-sync via AnalyserNode
          if (engineRef.value?.startAudioLipSync && audioCtx) {
            engineRef.value.startAudioLipSync(audioCtx, source);
          }
        } catch {
          // ignore audio errors
        }
      }
      // NOTE: wake_word_detected from Gateway is deprecated (v25)
      // Wake word is now handled entirely on frontend via ONNX WASM
    } catch {
      // ignore parse errors
    }
  };

  // 4. Listen for avatar hot-swap from Dashboard
  if (electronAPI) {
    electronAPI.onAvatarChanged((config: any) => {
      console.log('[Widget] Avatar config changed, reloading...', config);
      if (config.activeModel) {
        activeModelConfig.value = config.activeModel;
      }

      // Re-detect or force engine mode
      if (config.engineMode && config.engineMode !== 'auto') {
        engineMode.value = config.engineMode;
      } else {
        if (config.activeModel && config.activeModel.type) {
          engineMode.value = config.activeModel.type === '3d' ? '3D' : '2D';
        } else {
          engineMode.value = detectOptimalEngine('auto');
        }
      }
      activeEngine.value = engineMode.value === '3D' ? VRMEngine : Live2DEngine;
    });
  }

  // 5. Start Wake Word detection (always-on "Hey Liva" listener)
  //    Wait a bit for WebSocket to stabilize before starting mic
  setTimeout(() => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      wakeWord.startWakeWord().catch((e: any) =>
        console.warn('[Widget] Wake word start failed:', e?.message)
      );
    }
  }, 3000);
});

onUnmounted(() => {
  globalThis.removeEventListener("keydown", handleKeydown);
  if (ws) { ws.close(); ws = null; }
  stopQueuedAudio();
  if (audioCtx) { audioCtx.close(); audioCtx = null; }
  stopFrameCapture();
  stopListening();
  wakeWord.stopWakeWord();
  if (electronAPI) {
    electronAPI.removeAllListeners('avatar-changed');
    electronAPI.removeAllListeners('config-updated');
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
      style="pointer-events: none; position: fixed; right: 0; bottom: 40px; z-index: 0;"
    />

    <!-- Chat UI Layer (pointer-events: auto → bắt click) -->
    <div
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
      @mouseenter="enableMouse"
      @mouseleave="disableMouse"
    >
      <!-- Floating Mini-Icons -->
      <div 
        class="absolute flex gap-2.5 no-drag-region transition-all duration-300"
        :class="[
          snapPosition === 'left' ? 'left-0 flex-row-reverse' : 'right-2 flex-row',
          verticalSnapPosition === 'top' ? '-top-[44px]' : '-bottom-[44px]'
        ]"
      >
        <button class="floating-mini-icon w-8 h-8 flex items-center justify-center transition-all hover:scale-105" title="Cuộc trò chuyện mới" @click="startNewChat">
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
        <div
          v-for="(msg, idx) in messages"
          :key="idx"
          :class="[
            'px-3 py-2 rounded-2xl text-sm max-w-[85%] leading-relaxed',
            msg.role === 'user'
              ? 'self-end bg-gradient-to-r from-purple-600 to-blue-500 text-white rounded-br-sm'
              : 'self-start chat-bubble-ai rounded-bl-sm'
          ]"
        >
          {{ msg.text }}
        </div>
        <!-- Thinking indicator -->
        <div v-if="isThinking" class="self-start chat-bubble-ai px-3 py-2 rounded-2xl rounded-bl-sm text-sm flex items-center gap-1">
          <span class="thinking-dot" style="animation-delay: 0s">●</span>
          <span class="thinking-dot" style="animation-delay: 0.2s">●</span>
          <span class="thinking-dot" style="animation-delay: 0.4s">●</span>
        </div>
      </div>

      <!-- Full Chat Bar State -->
      <div v-if="!isCollapsed" class="chat-capsule w-full flex items-center p-[6px]" :class="snapPosition === 'left' ? 'flex-row-reverse' : ''">
        <!-- Drag Handle (Grip) -->
        <div 
          class="w-6 h-8 flex items-center justify-center cursor-move text-white/30 hover:text-white/60 transition-colors" 
          title="Kéo thả"
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
          placeholder="Nhờ LIVA hỗ trợ..."
          class="chat-input flex-1 bg-transparent border-none pl-1 pr-4 focus:outline-none w-full"
        />
        <div class="flex items-center gap-1.5" :class="snapPosition === 'left' ? 'flex-row-reverse pl-1' : 'pr-1'">
          <!-- Toggle Collapse Button -->
          <button
            @click="toggleCollapse"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center"
            title="Thu gọn"
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
            title="Đổi giao diện"
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
            title="Camera đang bật"
          >
            👁️
          </div>
          
          <button
            @click="openDashboard"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center"
            title="Cài đặt"
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
            :title="isThinking ? 'Ngắt lời LIVA' : (isListening ? 'Dừng ghi âm' : 'Nói với LIVA')"
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
          title="Kéo thả"
        ></div>
        
        <!-- Expand Button -->
        <button
          @mousedown.stop
          @click="toggleCollapse"
          class="chat-icon-btn bg-transparent border-none outline-none w-9 h-9 rounded-full flex justify-center items-center hover:bg-white/10 transition-colors z-20"
          title="Mở rộng"
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
</style>
