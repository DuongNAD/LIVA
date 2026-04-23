<script setup lang="ts">
/**
 * WidgetApp.vue — Hybrid Dual-Engine Widget
 * ==========================================
 * Entry component cho cửa sổ widget (transparent overlay).
 * - Auto-detect GPU → lazy load đúng engine (Live2D hoặc VRM)
 * - Zero cross-contamination: engine không dùng = 0 bytes RAM
 * - Phantom Bounding Box Fix (Phương án 1: pointer-events + IPC)
 */
import { ref, shallowRef, defineAsyncComponent, onMounted, onUnmounted, nextTick, watch } from "vue";
import { detectOptimalEngine, type EngineMode } from "./utils/HardwareDetector";
import { useMicrophone } from "./composables/useMicrophone";

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

// ═══════════════════════════════════════════════════════
//  Chat State
// ═══════════════════════════════════════════════════════
const isThinking = ref(false);
const inputText = ref("");
const messages = ref<{ role: "user" | "assistant"; text: string }[]>([
  {
    role: "assistant",
    text: "Xin chào! Mình là LIVA. Hệ thống đã sẵn sàng phục vụ anh ạ! 🚀",
  },
]);
const chatContainer = ref<HTMLElement | null>(null);
const isSensing = ref(false);
const isCameraActive = ref(false);

// ═══════════════════════════════════════════════════════
//  Voice Input (Microphone → STT)
// ═══════════════════════════════════════════════════════
const { isListening, volumeLevel, startListening, stopListening } = useMicrophone();

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

// ═══════════════════════════════════════════════════════
//  Engine ref for triggering motions
// ═══════════════════════════════════════════════════════
const engineRef = ref<any>(null);

// ═══════════════════════════════════════════════════════
//  Electron API (via preload.cjs contextBridge)
// ═══════════════════════════════════════════════════════
const electronAPI = (window as any).electronAPI;

// ═══════════════════════════════════════════════════════
//  Phantom Bounding Box Fix (Phương án 1: Ghost Mode)
// ═══════════════════════════════════════════════════════
const enableMouse = () => {
  if (electronAPI) electronAPI.setIgnoreMouse(false);
};
const disableMouse = () => {
  if (electronAPI) electronAPI.setIgnoreMouse(true);
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
// ═══════════════════════════════════════════════════════
const toggleVoice = async () => {
  if (isListening.value) {
    stopListening();
  } else {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    await startListening(ws);
  }
};

// Interrupt: if user clicks mic while LIVA is speaking
const interruptLIVA = () => {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send('[INTERRUPT]');
  }
};

// ═══════════════════════════════════════════════════════
//  Send Message
// ═══════════════════════════════════════════════════════
const sendMessage = () => {
  if (!inputText.value.trim() || !ws || ws.readyState !== WebSocket.OPEN) return;

  const text = inputText.value.trim();
  messages.value.push({ role: "user", text });

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
  window.addEventListener("keydown", handleKeydown);

  // 1. Auto-detect engine và lazy load
  // TODO: đọc preference từ config qua WebSocket, mặc định auto
  engineMode.value = detectOptimalEngine('auto');
  activeEngine.value = engineMode.value === '3D' ? VRMEngine : Live2DEngine;

  // 2. Mặc định xuyên chuột (Ghost Mode)
  disableMouse();

  // 3. Connect WebSocket
  ws = new WebSocket("ws://127.0.0.1:8082");
  ws.onopen = () => console.log("[Widget] WSS Connected to Gateway");

  ws.onmessage = async (event) => {
    try {
      const data = JSON.parse(event.data);

      if (data.event === "ai_thinking_start") {
        isThinking.value = true;
        if (audioCtx) nextAudioTime = audioCtx.currentTime;
        scrollToBottom();
      } else if (data.event === "ai_thinking_end") {
        isThinking.value = false;
      } else if (data.event === "ai_stream_start") {
        isThinking.value = false;
        messages.value.push({ role: "assistant", text: "" });
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
          scrollToBottom();
        }
      } else if (data.event === "ai_spoken_response") {
        isThinking.value = false;
        const lastMsg = messages.value[messages.value.length - 1];
        if (lastMsg && lastMsg.role === "assistant" && lastMsg.text.length > 0
            && data.payload.text.includes(lastMsg.text.trim())) {
          lastMsg.text = data.payload.text;
        } else if (!lastMsg || lastMsg.role === "user") {
          messages.value.push({ role: "assistant", text: data.payload.text });
        }
        scrollToBottom();
      } else if (data.event === "ai_audio_chunk") {
        // Audio playback (base64 MP3 from voice_engine)
        try {
          if (!audioCtx) {
            const AudioContextCls = window.AudioContext || (window as any).webkitAudioContext;
            audioCtx = new AudioContextCls();
          }
          if (audioCtx.state === 'suspended') await audioCtx.resume();

          const binaryStr = atob(data.payload.audio);
          const bytes = new Uint8Array(binaryStr.length);
          for (let i = 0; i < binaryStr.length; i++) bytes[i] = binaryStr.codePointAt(i) as number;

          const audioBuffer = await audioCtx.decodeAudioData(bytes.buffer);
          const source = audioCtx.createBufferSource();
          source.buffer = audioBuffer;
          source.connect(audioCtx.destination);

          const overlap = 0.1;
          const currentTime = audioCtx.currentTime;
          if (nextAudioTime < currentTime) nextAudioTime = currentTime;
          source.start(nextAudioTime);
          nextAudioTime += (audioBuffer.duration - overlap);

          // Audio-driven lip-sync via AnalyserNode
          if (engineRef.value?.startAudioLipSync && audioCtx) {
            engineRef.value.startAudioLipSync(audioCtx, source);
            source.onended = () => {
              if (engineRef.value?.stopAudioLipSync) engineRef.value.stopAudioLipSync();
            };
          }
        } catch {
          // ignore audio errors
        }
      }
    } catch {
      // ignore parse errors
    }
  };

  // 4. Listen for avatar hot-swap from Dashboard
  if (electronAPI) {
    electronAPI.onAvatarChanged((config: any) => {
      console.log('[Widget] Avatar config changed, reloading...', config);
      // Re-detect or force engine mode
      if (config.engineMode && config.engineMode !== 'auto') {
        engineMode.value = config.engineMode;
      } else {
        engineMode.value = detectOptimalEngine('auto');
      }
      activeEngine.value = engineMode.value === '3D' ? VRMEngine : Live2DEngine;
    });
  }
});

onUnmounted(() => {
  window.removeEventListener("keydown", handleKeydown);
  if (ws) { ws.close(); ws = null; }
  if (audioCtx) { audioCtx.close(); audioCtx = null; }
  stopFrameCapture();
  stopListening();
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
      style="pointer-events: none; position: fixed; right: 0; bottom: -20px; z-index: 0;"
    />

    <!-- Chat UI Layer (pointer-events: auto → bắt click) -->
    <div
      class="glass w-full max-w-[400px] rounded-[24px] p-2 flex flex-col relative z-10 animate-fade-in-up mb-[60px] mr-4 shadow-2xl"
      style="pointer-events: auto;"
      @mouseenter="enableMouse"
      @mouseleave="disableMouse"
    >
      <!-- Messages (scrollable) -->
      <div
        ref="chatContainer"
        v-if="messages.length > 1 || isThinking"
        class="scrollbar-hide mb-2 max-h-[300px] overflow-y-auto flex flex-col gap-2 px-2 py-1"
      >
        <div
          v-for="(msg, idx) in messages"
          :key="idx"
          :class="[
            'px-3 py-2 rounded-2xl text-sm max-w-[85%] leading-relaxed',
            msg.role === 'user'
              ? 'self-end bg-gradient-to-r from-purple-600 to-blue-500 text-white rounded-br-sm'
              : 'self-start bg-white/10 text-white/90 rounded-bl-sm'
          ]"
        >
          {{ msg.text }}
        </div>
        <!-- Thinking indicator -->
        <div v-if="isThinking" class="self-start bg-white/10 text-white/60 px-3 py-2 rounded-2xl rounded-bl-sm text-sm flex items-center gap-1">
          <span class="thinking-dot" style="animation-delay: 0s">●</span>
          <span class="thinking-dot" style="animation-delay: 0.2s">●</span>
          <span class="thinking-dot" style="animation-delay: 0.4s">●</span>
        </div>
      </div>

      <!-- Input -->
      <div class="relative w-full">
        <input
          v-model="inputText"
          @keyup.enter="sendMessage"
          type="text"
          placeholder="Nhờ LIVA hỗ trợ..."
          class="w-full bg-white bg-opacity-10 border border-white border-opacity-20 text-white placeholder-white/50 px-5 py-3 pr-20 rounded-[18px] focus:outline-none focus:ring-2 focus:ring-white/40 transition-all font-medium text-sm"
        />
        <div class="absolute right-2 top-1/2 transform -translate-y-1/2 flex gap-1">
          <!-- Camera indicator -->
          <div
            v-if="isCameraActive"
            class="w-8 h-8 rounded-full bg-green-500/20 text-green-400 flex justify-center items-center text-xs"
            title="Camera đang bật"
          >
            👁️
          </div>
          <!-- Voice button (mic toggle) -->
          <button
            @click="isThinking ? interruptLIVA() : toggleVoice()"
            class="voice-btn w-8 h-8 rounded-full flex justify-center items-center text-xs transition-all relative"
            :class="{
              'bg-red-500/30 text-red-400 hover:bg-red-500/50': isListening,
              'bg-white/10 text-white/70 hover:bg-white/20': !isListening && !isThinking,
              'bg-orange-500/30 text-orange-400 hover:bg-orange-500/50 animate-pulse': isThinking,
            }"
            :title="isThinking ? 'Ngắt lời LIVA' : (isListening ? 'Dừng ghi âm' : 'Nói với LIVA')"
          >
            <!-- Volume ring (when recording) -->
            <svg v-if="isListening" class="voice-ring" viewBox="0 0 36 36">
              <circle cx="18" cy="18" r="16" fill="none" stroke="currentColor" stroke-width="2" opacity="0.2" />
              <circle
                cx="18" cy="18" r="16" fill="none" stroke="currentColor" stroke-width="2.5"
                :stroke-dasharray="`${volumeLevel * 100} ${100 - volumeLevel * 100}`"
                stroke-linecap="round"
                transform="rotate(-90 18 18)"
              />
            </svg>
            {{ isThinking ? '✋' : (isListening ? '⏹' : '🎤') }}
          </button>
          <!-- Dashboard button -->
          <button
            @click="openDashboard"
            class="w-8 h-8 rounded-full bg-white/10 text-white/70 hover:bg-white/20 flex justify-center items-center text-xs transition-all"
            title="Mở Dashboard"
          >
            ⚙
          </button>
          <!-- Send button -->
          <button
            @click="sendMessage"
            :disabled="!inputText.trim()"
            class="w-8 h-8 rounded-full bg-white text-purple-600 hover:bg-purple-100 disabled:opacity-30 disabled:bg-white/20 flex justify-center items-center font-bold transition-all disabled:cursor-not-allowed"
          >
            ↑
          </button>
        </div>
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
