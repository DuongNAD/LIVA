<script setup lang="ts">
import { ref, shallowRef, triggerRef, onMounted, onUnmounted, nextTick, watch } from "vue";
import { logger } from "./utils/logger";
import { safeFetch } from "./utils/fetch";
import {
  OP_FLUSH,
  OP_SPEAKER_OUT,
  SpeakerEpochGate,
  VOICE_FRAME_HEADER_SIZE,
  parseSpeakerPayload,
} from "./utils/speakerFrame";
import { useSpeakerPlayback } from "./composables/useSpeakerPlayback";
import { pack, unpack } from "msgpackr";
import type { Application } from "pixi.js";
import type { Live2DModel } from "pixi-live2d-display/cubism2";

// Khung thông điệp WS từ core; các trường trong payload tùy theo `event`
interface WsMessage {
  event: string;
  payload: {
    isThought?: boolean;
    textChunk: string;
    text: string;
    audio: string;
  };
}

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

const messages = shallowRef<{ role: "user" | "assistant"; text: string }[]>([
  {
    role: "assistant",
    text: t('welcome_liva_turbo'),
  },
]);
const chatContainer = ref<HTMLElement | null>(null);

let ws: WebSocket | null = null;

const sendMsg = (event: string, payload: unknown = {}) => {
  if (ws && ws.readyState === WebSocket.OPEN) {
    const packed = pack({ event, payload });
    const message = new Uint8Array(1 + packed.byteLength);
    message[0] = 0x02; // MessagePack event
    message.set(new Uint8Array(packed), 1);
    ws.send(message);
  }
};

const l2dCanvas = ref<HTMLCanvasElement | null>(null);
// Ép kiểu không-null: model chỉ được chạm tới sau khi Live2DModel.from() xong,
// và closure "pointertap" của PIXI không giữ được thu hẹp kiểu của biến ngoài.
let avatarModel = null as unknown as Live2DModel;
let pixiApp: Application | null = null; // 🔒 [Memory Fix #4] Lưu handle PIXI App để destroy() khi unmount

// Audio Queue — gapless OP_SPEAKER_OUT PCM; encoded JSON audio is a separate path.
// FLUSH/barge-in (speaker.stop()/speaker.flush()) stops every scheduled source
// and resets the scheduling cursor.
const speaker = useSpeakerPlayback({
  channel: '[App]',
  onPlaybackStarted: () => sendMsg("audio_play_started"),
  onPlaybackFinished: () => sendMsg("audio_play_finished"),
  onSourceStarted: () => {
    if (avatarModel) {
      avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
    }
  },
});

// ⚡ [PERF P0-D] Pre-recorded filler audio state
let fillerBuffers: AudioBuffer[] = [];
let currentFillerSource: AudioBufferSourceNode | null = null;
let currentFillerGain: GainNode | null = null;
let fillerDebounceTimer: ReturnType<typeof setTimeout> | null = null; // [Upgrade D] Anti-stuttering debounce

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

  speaker.stop();

  const text = inputText.value.trim();
  messages.value.push({ role: "user", text });
  triggerRef(messages);

  sendMsg("user_voice_command", { text });

  inputText.value = "";
  scrollToBottom();
};

onMounted(() => {
  globalThis.addEventListener("keydown", handleKeydown);

  if (platform) {
    platform.onGatewayReady((port, token) => {
      const wsUrl = token ? `ws://127.0.0.1:${port}/ws?token=${token}` : `ws://127.0.0.1:${port}/ws`;
      ws = new WebSocket(wsUrl);
      ws.binaryType = "arraybuffer";
      const speakerEpochGate = new SpeakerEpochGate();
      ws.onopen = () => logger.info('[App]', `WSS Connected LIVA on port ${port}`);

      ws.onmessage = async (event) => {
        try {
          let data: WsMessage | null = null;
          if (event.data instanceof ArrayBuffer) {
            const arrayBuffer = event.data;
            if (arrayBuffer.byteLength > 0) {
              const view = new DataView(arrayBuffer);
              const type = view.getUint8(0);
              if (type === OP_SPEAKER_OUT) {
                // Check if this is a VoiceBinaryProtocol SPEAKER_OUT frame (header: opcode + seqId + payloadSize = 9 bytes)
                // VoiceBinaryProtocol uses 0x02 as SPEAKER_OUT opcode, but MsgPack events also use 0x02 prefix.
                // Distinguish: VoiceBinaryProtocol frames have header size >= 9, and the payloadSize field at bytes 5-8
                // matches (totalLength - 9). MsgPack frames don't have this structure.
                if (arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE) {
                  const payloadSize = view.getUint32(5, true); // Little-Endian
                  if (payloadSize === arrayBuffer.byteLength - VOICE_FRAME_HEADER_SIZE && payloadSize > 0) {
                    // SPEAKER_OUT payload: [turn_epoch u32][sample_rate u32][f32 PCM…]
                    // Fail closed: untagged/malformed audio must not bypass the epoch gate.
                    const payload = new Uint8Array(
                      arrayBuffer,
                      VOICE_FRAME_HEADER_SIZE,
                      payloadSize,
                    );
                    const chunk = parseSpeakerPayload(payload);
                    if (!chunk || !speakerEpochGate.accepts(chunk.turnEpoch)) return;
                    speaker.enqueueSpeakerPayload(payload);
                    return;
                  }
                }
                // Otherwise it's a MsgPack event (legacy path)
                try {
                  data = unpack(new Uint8Array(arrayBuffer, 1));
                } catch (unpackErr) {
                  logger.error('[App]', 'Lỗi unpack MsgPack:', unpackErr);
                  return;
                }
              } else if (type === OP_FLUSH) {
                // Barge-in: the core cancelled the active TTS session — stop all
                // scheduled audio and reset the cursor. Chunks arriving after the
                // The epoch watermark rejects any stale chunk queued behind this FLUSH.
                if (arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE) {
                  speakerEpochGate.observeFlush(view.getUint32(1, true));
                }
                speaker.flush();
                return;
              } else {
                return; // skip audio/other types
              }
            } else {
              return;
            }
          } else if (typeof event.data === "string") {
            if (event.data.trim() === "[INTERRUPT]") {
              speaker.stop();
              return;
            }
            try {
              data = JSON.parse(event.data);
            } catch (e) {
              logger.error('[App]', 'Lỗi phân giải JSON:', e);
              return;
            }
          } else {
            return;
          }

          if (!data) return;
          if (data.event === "ai_thinking_start") {
            isThinking.value = true;
            speaker.stop();
            // [Upgrade D] Cancel pending filler debounce on new thinking cycle
            if (fillerDebounceTimer) { clearTimeout(fillerDebounceTimer); fillerDebounceTimer = null; }
            scrollToBottom();
          } else if (data.event === "ai_thinking_end") {
            isThinking.value = false;
          } else if (data.event === "ai_stream_start") {
            // [Upgrade D] Cancel filler debounce if LLM responded within 200ms
            if (fillerDebounceTimer) { clearTimeout(fillerDebounceTimer); fillerDebounceTimer = null; }
            // ⚡ [PERF P0-D] Fade-out any playing filler audio
            const fillerCtx = speaker.getContext();
            if (currentFillerSource && currentFillerGain && fillerCtx) {
              const now = fillerCtx.currentTime;
              currentFillerGain.gain.setValueAtTime(currentFillerGain.gain.value, now);
              currentFillerGain.gain.linearRampToValueAtTime(0, now + 0.15);
              const src = currentFillerSource;
              setTimeout(() => { try { src.stop(); } catch { /* nguon co the da dung tu truoc — bo qua */ } }, 200);
              currentFillerSource = null;
              currentFillerGain = null;
            }
            speaker.unblock();
            isThinking.value = false;
            messages.value.push({ role: "assistant", text: "" });
            triggerRef(messages);
            scrollToBottom();
          } else if (data.event === "ai_stream_chunk") {
            if (data.payload.isThought) return;
            if (messages.value.length > 0) {
              messages.value[messages.value.length - 1].text +=
                data.payload.textChunk;
              triggerRef(messages);
              scrollToBottom();

              if (avatarModel && Math.random() > 0.9) {
                avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
              }
            }
          } else if (data.event === "ai_spoken_response") {
            speaker.unblock();
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
            triggerRef(messages);
            scrollToBottom();
          } else if (data.event === "ai_audio_chunk") {
            if (speaker.isBlocked()) return;

            try {
              const base64 = data.payload.audio;
              const binaryStr = atob(base64);
              const bytes = new Uint8Array(binaryStr.length);
              for (let i = 0; i < binaryStr.length; i++) bytes[i] = binaryStr.codePointAt(i) as number;

              await speaker.enqueueEncodedAudio(bytes.buffer);
            } catch (audioErr: unknown) {
              logger.warn('[App]', 'Lỗi phát âm thanh:', audioErr instanceof Error ? audioErr.message : String(audioErr));
            }
          } else if (data.event === "ai_filler_response") {
            // ⚡ [PERF P0-D + Upgrade D] Debounced filler audio (200ms)
            // Prevents stuttering when LLM responds instantly (cache hit / fast route)
            if (fillerBuffers.length > 0) {
              if (fillerDebounceTimer) clearTimeout(fillerDebounceTimer);
              fillerDebounceTimer = setTimeout(async () => {
                fillerDebounceTimer = null;
                const ctx = speaker.getContext();
                if (!ctx) return;
                try {
                  if (ctx.state === 'suspended') await ctx.resume();
                  const buffer = fillerBuffers[Math.floor(Math.random() * fillerBuffers.length)];
                  const gain = ctx.createGain();
                  gain.gain.value = 0.8;
                  gain.connect(ctx.destination);
                  const source = ctx.createBufferSource();
                  source.buffer = buffer;
                  source.connect(gain);
                  source.onended = () => {
                    if (currentFillerSource === source) {
                      currentFillerSource = null;
                      currentFillerGain = null;
                    }
                  };
                  // Stop previous filler if still playing
                  if (currentFillerSource) {
                    try { currentFillerSource.stop(); } catch { /* nguon co the da dung tu truoc — bo qua */ }
                  }
                  currentFillerSource = source;
                  currentFillerGain = gain;
                  source.start();
                } catch (e: unknown) {
                  logger.warn('[App]', 'Filler audio play error:', e instanceof Error ? e.message : String(e));
                }
              }, 200); // ⚡ Only play if LLM thinks longer than 200ms
            }
          }
        } catch (wsErr: unknown) {
          logger.warn('[App]', 'WebSocket message error:', wsErr instanceof Error ? wsErr.message : String(wsErr));
        }
      };
    });
  }

  // ⚡ [PERF P0-D] Pre-load filler audio files on mount
  (async () => {
    try {
      const ctx = speaker.ensureContext();
      const fillerPaths = [
        '/fillers/hmm_01.wav',
        '/fillers/hmm_02.wav',
        '/fillers/let_me_think_01.wav',
        '/fillers/ah_01.wav',
      ];
      const loaded: AudioBuffer[] = [];
      for (const p of fillerPaths) {
        try {
          const res = await safeFetch(p);
          if (res.ok) {
            const buf = await res.arrayBuffer();
            const decoded = await ctx.decodeAudioData(buf);
            loaded.push(decoded);
          }
        } catch { /* filler file not found — skip */ }
      }
      fillerBuffers = loaded;
      if (loaded.length > 0) {
        logger.info('[App]', `Loaded ${loaded.length} filler audio files`);
      }
    } catch (e: unknown) {
      logger.warn('[App]', 'Filler audio preload failed:', e instanceof Error ? e.message : String(e));
    }
  })();

  // 2. Tái sinh Bể nuôi PIXI chứa Búp Bê
  setTimeout(async () => {
    try {
      const PIXI = await import("pixi.js");
      (globalThis as unknown as { PIXI: unknown }).PIXI = PIXI;
      const { Live2DModel } = await import("pixi-live2d-display/cubism2");

      const app = new PIXI.Application({
        view: l2dCanvas.value!,
        backgroundAlpha: 0,
        width: 500,
        height: 700,
        autoStart: true,
      });

      avatarModel = await Live2DModel.from(
        "/assets/models/pio/index.json",
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
    avatarModel = null as unknown as Live2DModel;
  }
  // 🔒 [Memory Fix #5] Đóng AudioContext để giải phóng WebAudio resources
  speaker.close();
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
