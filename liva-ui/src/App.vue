<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from "vue";

// Khởi tạo cầu nối IPC giữa Vue và Electron
const ipcRenderer = window.require ? window.require('electron').ipcRenderer : null;

const handleMouseEnter = () => {
  if (ipcRenderer) ipcRenderer.send('set-ignore-mouse-events', false);
};

const handleMouseLeave = () => {
  if (ipcRenderer) ipcRenderer.send('set-ignore-mouse-events', true, { forward: true });
};

const isSensing = ref(false);
const isThinking = ref(false);
const inputText = ref("");
const messages = ref<{ role: "user" | "assistant"; text: string }[]>([
  {
    role: "assistant",
    text: "Xin chào! Mình là LIVA. Mô hình suy luận và bộ nhớ TurboQuant QJL đã được kích hoạt thành công. Anh cần hỗ trợ gì ạ?",
  },
]);
const chatContainer = ref<HTMLElement | null>(null);

let ws: WebSocket | null = null;
const l2dCanvas = ref<HTMLCanvasElement | null>(null);
let avatarModel: any = null;

// Audio Queue State
let audioCtx: AudioContext | null = null;
let nextAudioTime = 0;

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
    try {
      await fetch("http://localhost:3000/api/sensory-capture", {
        method: "POST",
      });
      setTimeout(() => {
        isSensing.value = false;
      }, 30000);
    } catch (e) {
      setTimeout(() => {
        isSensing.value = false;
      }, 30000);
    }
  }
};

const sendMessage = () => {
  if (!inputText.value.trim() || !ws || ws.readyState !== WebSocket.OPEN)
    return;

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
  window.addEventListener("keydown", handleKeydown);

  ws = new WebSocket("ws://localhost:8082");
  ws.onopen = () => console.log("WSS Connected LIVA");

  // 2. Tái sinh Bể nuôi PIXI chứa Búp Bê
  setTimeout(async () => {
    try {
      // Dynamic import để ép vòng đời ưu tiên (tránh Hoisting Error gây màn hình trắng)
      const PIXI = await import("pixi.js");
      (window as any).PIXI = PIXI;
      const { Live2DModel } = await import("pixi-live2d-display/cubism2");

      // Điều chỉnh Khuôn viên Nhốt Búp bê rộn rãi hơn (Đừng cắt màn hình)
      const app = new PIXI.Application({
        view: l2dCanvas.value!,
        transparent: true,
        width: 500,
        height: 700,
        autoStart: true,
      });

      // Nhất quyết Trở về bản Căn Nguyên đẹp nhất: Bé Phù Thủy Pio!
      // Chấp nhận việc ẻm không có thân dưới, nhưng đổi lại nhan sắc chuẩn AAA+ Live2D. Tôi sẽ giấu phần khuyết của ẻm xuống dưới màn hình!
      avatarModel = await Live2DModel.from(
        "https://unpkg.com/live2d-widget-model-pio@9.1.2/assets/index.json",
      );
      app.stage.addChild(avatarModel);

      // Trọng tâm lại Tỷ lệ (Gắn xương cho bé Phù Thủy)
      avatarModel.scale.set(0.35); // Phóng to chà bá
      avatarModel.x = 100; // Canh vào giữa một chút để khỏi chém cánh
      avatarModel.y = 320; // Kéo giật phần eo cụt của ẻm xuống dưới đáy màn hình! Mọi thứ sẽ mượt!

      // Kết nối dây thần kinh Vật Lý (Chọc tức / Vuốt ve)
      avatarModel.on("pointertap", () => {
        avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
      });
    } catch (e) {
      console.error("PIXI Model Injection failed: ", e);
    }
  }, 100);

  ws.onmessage = async (event) => {
    try {
      const data = JSON.parse(event.data);
      if (data.event === "ai_thinking_start") {
        isThinking.value = true;
        // Dừng và làm rỗng hàng đợi nếu AI bị ngắt lời
        if (audioCtx) {
          nextAudioTime = audioCtx.currentTime;
        }
        scrollToBottom();
      } else if (data.event === "ai_thinking_end") {
        isThinking.value = false;
      } else if (data.event === "ai_stream_start") {
        isThinking.value = false;
        messages.value.push({ role: "assistant", text: "" });
        scrollToBottom();
      } else if (data.event === "ai_stream_chunk") {
        if (messages.value.length > 0) {
          messages.value[messages.value.length - 1].text +=
            data.payload.textChunk;
          scrollToBottom();

          // Tương tác vật lý: Khi AI thốt ra chữ, cứ 10% xác suất thì nhấp môi hoặc chớp mắt múa tay
          if (avatarModel && Math.random() > 0.9) {
            avatarModel.internalModel.motionManager.startRandomMotion(
              "tap_body",
            );
          }
        }
      } else if (data.event === "ai_spoken_response") {
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
        // Phát âm thanh base64 MP3 từ voice_engine.py (edge_tts) và xếp hàng tự động (Web Audio API)
        try {
          if (!audioCtx) {
            const AudioContextCls = window.AudioContext || (window as any).webkitAudioContext;
            audioCtx = new AudioContextCls();
          }
          if (audioCtx.state === 'suspended') {
            await audioCtx.resume();
          }

          const base64 = data.payload.audio;
          const binaryStr = atob(base64);
          const bytes = new Uint8Array(binaryStr.length);
          for (let i = 0; i < binaryStr.length; i++) bytes[i] = binaryStr.charCodeAt(i);
          
          const audioBuffer = await audioCtx.decodeAudioData(bytes.buffer);
          const source = audioCtx.createBufferSource();
          source.buffer = audioBuffer;
          source.connect(audioCtx.destination);
          
          let overlap = 0.1; // Cắt bỏ 100ms MP3 Silence Padding để nối liền mạch các câu
          let currentTime = audioCtx.currentTime;
          if (nextAudioTime < currentTime) {
              nextAudioTime = currentTime;
          }
          source.start(nextAudioTime);
          nextAudioTime += (audioBuffer.duration - overlap);

          // Cử động khuôn miệng của LIVA Live2D cho tới khi audio dừng
          if (avatarModel) {
            avatarModel.internalModel.motionManager.startRandomMotion("tap_body");
          }
        } catch (audioErr) {
          console.warn('[Audio] Lỗi phát âm thanh:', audioErr);
        }
      }
    } catch (e) {}
  };
});

onUnmounted(() => {
  window.removeEventListener("keydown", handleKeydown);
  if (ws) ws.close();
});
</script>

<template>
  <div
    class="h-screen w-screen flex flex-col items-end justify-end bg-transparent font-sans relative overflow-hidden pr-4 pb-4"
  >
    <!-- Canvas Live2D: mix-blend-mode:multiply để nền đen biến mất, chỉ render nhân vật -->
    <canvas
      ref="l2dCanvas"
      @mouseenter="handleMouseEnter"
      @mouseleave="handleMouseLeave"
      width="500"
      height="800"
      style="mix-blend-mode: multiply; position: fixed; right: 0; bottom: -20px; z-index: 0; cursor: pointer; pointer-events: auto;"
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
