<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from "vue";
import { logger } from "./utils/logger";
import { safeFetch } from "./utils/fetch";

// Khởi tạo cầu nối IPC qua PlatformBridge (Agnostic)
import { inject } from "vue";
import type { IPlatformAdapter } from "./platform/IPlatformAdapter";

const platform = inject<IPlatformAdapter>('platform');

const handleMouseEnter = () => {
  if (platform) platform.toggleGhostMode(false);
};

const handleMouseLeave = () => {
  if (platform) platform.toggleGhostMode(true);
};

const isSensing = ref(false);
let sensingTimer: ReturnType<typeof setTimeout> | null = null;
const isThinking = ref(false);
const inputText = ref("");

import { useI18n } from "./composables/useI18n";
const { t } = useI18n();

const messages = ref<{ role: "user" | "assistant"; text: string }[]>([
  {
    role: "assistant",
    text: t('welcome_liva_turbo'),
  },
]);
const chatContainer = ref<HTMLElement | null>(null);

let ws: WebSocket | null = null;
const l2dCanvas = ref<HTMLCanvasElement | null>(null);
let avatarModel: any = null;
let pixiApp: any = null; // 🔒 [Memory Fix #4] Lưu handle PIXI App để destroy() khi unmount

// Audio Queue State
let audioCtx: AudioContext | null = null;
let nextAudioTime = 0;
let activeAudioSources: AudioBufferSourceNode[] = [];
let audioQueueEpoch = 0;
let isAudioPlaybackBlocked = false;
let isPlayingAudio = false;

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

  nextAudioTime = audioCtx ? audioCtx.currentTime : 0;

  if (isPlayingAudio) {
    isPlayingAudio = false;
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ event: "audio_play_finished" }));
    }
  }
};

watch(isThinking, (val) => {
  if (avatarModel) {
    if (val) {
      avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
    }
  }
});

const scrollToBottom = async () => {
  await nextTick();
  if (chatContainer.value) {
    chatContainer.value.scrollTop = chatContainer.value.scrollHeight;
  }
};

const handleKeydown = async (e: KeyboardEvent) => {
  if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "s") {
    isSensing.value = true;
    if (sensingTimer) clearTimeout(sensingTimer);
    sensingTimer = setTimeout(() => { isSensing.value = false; sensingTimer = null; }, 30000);
    try {
      await safeFetch("http://127.0.0.1:3000/api/sensory-capture", { method: "POST" });
    } catch {
      // sensing flag resets via timer
    }
  }
};

const sendMessage = () => {
  if (!inputText.value.trim() || !ws || ws.readyState !== WebSocket.OPEN)
    return;

  stopQueuedAudio();

  const text = inputText.value.trim();
  messages.value.push({ role: "user", text });

  ws.send(
    JSON.stringify({
      event: "user_voice_command",
      payload: { text },
    }),
  );

  inputText.value = "";
  scrollToBottom();
};

onMounted(() => {
  globalThis.addEventListener("keydown", handleKeydown);

  if (platform) {
    platform.onGatewayReady((port, token) => {
      const wsUrl = token ? `ws://127.0.0.1:${port}?token=${token}` : `ws://127.0.0.1:${port}`;
      ws = new WebSocket(wsUrl);
      ws.onopen = () => logger.info('[App]', `WSS Connected LIVA on port ${port}`);

      ws.onmessage = async (event) => {
        try {
          if (typeof event.data === "string" && event.data.trim() === "[INTERRUPT]") {
            stopQueuedAudio();
            return;
          }

          const data = JSON.parse(event.data);
          if (data.event === "ai_thinking_start") {
            isThinking.value = true;
            stopQueuedAudio();
            scrollToBottom();
          } else if (data.event === "ai_thinking_end") {
            isThinking.value = false;
          } else if (data.event === "ai_stream_start") {
            isAudioPlaybackBlocked = false;
            isThinking.value = false;
            messages.value.push({ role: "assistant", text: "" });
            scrollToBottom();
          } else if (data.event === "ai_stream_chunk") {
            if (messages.value.length > 0) {
              messages.value[messages.value.length - 1].text +=
                data.payload.textChunk;
              scrollToBottom();

              if (avatarModel && Math.random() > 0.9) {
                avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
              }
            }
          } else if (data.event === "ai_spoken_response") {
            isAudioPlaybackBlocked = false;
            isThinking.value = false;
            const lastMsg = messages.value[messages.value.length - 1];
            if (
              lastMsg &&
              lastMsg.role === "assistant" &&
              lastMsg.text.length > 0 &&
              data.payload.text.includes(lastMsg.text.trim())
            ) {
              lastMsg.text = data.payload.text;
            } else if (!lastMsg || lastMsg.role === "user") {
              messages.value.push({ role: "assistant", text: data.payload.text });
            }
            scrollToBottom();
          } else if (data.event === "ai_audio_chunk") {
            if (isAudioPlaybackBlocked) return;

            try {
              if (!audioCtx) {
                const AudioContextCls = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
                audioCtx = new AudioContextCls();
              }
              if (audioCtx.state === 'suspended') {
                await audioCtx.resume();
              }

              const base64 = data.payload.audio;
              const queueEpoch = audioQueueEpoch;
              const binaryStr = atob(base64);
              const bytes = new Uint8Array(binaryStr.length);
              for (let i = 0; i < binaryStr.length; i++) bytes[i] = binaryStr.codePointAt(i) as number;
              
              const audioBuffer = await audioCtx.decodeAudioData(bytes.buffer);
              if (queueEpoch !== audioQueueEpoch || isAudioPlaybackBlocked) return;

              const source = audioCtx.createBufferSource();
              source.buffer = audioBuffer;
              source.connect(audioCtx.destination);
              source.onended = () => {
                removeAudioSource(source);
                if (activeAudioSources.length === 0 && isPlayingAudio) {
                  isPlayingAudio = false;
                  if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({ event: "audio_play_finished" }));
                  }
                }
              };
              
              let overlap = 0.1;
              let currentTime = audioCtx.currentTime;
              if (nextAudioTime < currentTime) {
                  nextAudioTime = currentTime;
              }
              activeAudioSources.push(source);

              if (!isPlayingAudio && activeAudioSources.length === 1) {
                isPlayingAudio = true;
                if (ws && ws.readyState === WebSocket.OPEN) {
                  ws.send(JSON.stringify({ event: "audio_play_started" }));
                }
              }

              source.start(nextAudioTime);
              nextAudioTime += (audioBuffer.duration - overlap);

              if (avatarModel) {
                avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
              }
            } catch (audioErr: unknown) {
              logger.warn('[App]', 'Lỗi phát âm thanh:', audioErr instanceof Error ? audioErr.message : String(audioErr));
            }
          }
        } catch (wsErr: unknown) {
          logger.warn('[App]', 'WebSocket message error:', wsErr instanceof Error ? wsErr.message : String(wsErr));
        }
      };
    });
  }

  // 2. Tái sinh Bể nuôi PIXI chứa Búp Bê
  setTimeout(async () => {
    try {
      const PIXI = await import("pixi.js");
      (globalThis as any).PIXI = PIXI;
      const { Live2DModel } = await import("pixi-live2d-display/cubism2");

      const app = new PIXI.Application({
        view: l2dCanvas.value!,
        backgroundAlpha: 0,
        width: 500,
        height: 700,
        autoStart: true,
      });

      avatarModel = await Live2DModel.from(
        "https://unpkg.com/live2d-widget-model-pio@9.1.2/assets/index.json",
      );
      app.stage.addChild(avatarModel);
      pixiApp = app;

      avatarModel.scale.set(0.35);
      avatarModel.x = 100;
      avatarModel.y = 320;

      avatarModel.on("pointertap", () => {
        avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
      });
    } catch (e: unknown) {
      logger.error('[App]', 'PIXI Model Injection failed:', e instanceof Error ? e.message : String(e));
    }
  }, 100);
});

onUnmounted(() => {
  globalThis.removeEventListener("keydown", handleKeydown);
  if (sensingTimer) { clearTimeout(sensingTimer); sensingTimer = null; }
  if (ws) {
    ws.close();
    ws = null;
  }
  // 🔒 [Memory Fix #4] Hủy tài nguyên WebGL và Texture của PIXI
  if (pixiApp) {
    pixiApp.destroy(true, { children: true, texture: true, baseTexture: true });
    pixiApp = null;
    avatarModel = null;
  }
  // 🔒 [Memory Fix #5] Đóng AudioContext để giải phóng WebAudio resources
  stopQueuedAudio();
  if (audioCtx) {
    audioCtx.close();
    audioCtx = null;
  }
});
</script>

<template>
  <div
    class="h-screen w-screen flex flex-col items-end justify-end bg-transparent font-sans relative overflow-hidden pr-4 pb-4"
  >
    <!-- Canvas Live2D: render trực tiếp để không bị trong suốt hoàn toàn trên nền transparent -->
    <canvas
      ref="l2dCanvas"
      @mouseenter="handleMouseEnter"
      @mouseleave="handleMouseLeave"
      width="500"
      height="800"
      style="mix-blend-mode: normal; position: fixed; right: 0; bottom: -20px; z-index: 0; cursor: pointer; pointer-events: auto; opacity: 1;"
    ></canvas>

    <!-- Removed Background Blobs for Full Desktop Window Transparency -->

    <div
      @mouseenter="handleMouseEnter"
      @mouseleave="handleMouseLeave"
      class="glass w-full max-w-[400px] rounded-[24px] p-2 flex flex-col relative z-10 animate-fade-in-up mb-[60px] shadow-2xl"
    >
      <div class="relative w-full">
        <input
          v-model="inputText"
          @keyup.enter="sendMessage"
          type="text"
          placeholder="Nhờ LIVA quét ổ đĩa, check email, tìm Google..."
          class="w-full bg-white bg-opacity-10 border border-white border-opacity-20 text-white placeholder-white/50 px-5 py-3 pr-12 rounded-[18px] focus:outline-none focus:ring-2 focus:ring-white/40 transition-all font-medium"
        />
        <button
          @click="sendMessage"
          :disabled="!inputText.trim()"
          class="absolute right-2 top-1/2 transform -translate-y-1/2 w-8 h-8 rounded-full bg-white text-purple-600 hover:bg-purple-100 disabled:opacity-30 disabled:bg-white/20 flex justify-center items-center font-bold transition-all disabled:cursor-not-allowed"
        >
          ↑
        </button>
      </div>
    </div>
  </div>
</template>

<style>
@keyframes blob {
  0% {
    transform: translate(0px, 0px) scale(1);
  }
  33% {
    transform: translate(30px, -50px) scale(1.1);
  }
  66% {
    transform: translate(-20px, 20px) scale(0.9);
  }
  100% {
    transform: translate(0px, 0px) scale(1);
  }
}
.animate-blob {
  animation: blob 7s infinite;
}
.animation-delay-2000 {
  animation-delay: 2s;
}
@keyframes fadeInUp {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
.animate-fade-in-up {
  animation: fadeInUp 0.6s ease-out forwards;
}

.scrollbar-hide::-webkit-scrollbar {
  display: none;
}
.scrollbar-hide {
  -ms-overflow-style: none;
  scrollbar-width: none;
}
</style>
