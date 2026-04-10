<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from "vue";

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

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data);
      if (data.event === "ai_thinking_start") {
        isThinking.value = true;
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
          lastMsg.text = data.payload.text; // Đè kết quả hoàn chỉnh để fix ký tự rác nếu có
        } else if (!lastMsg || lastMsg.role === "user") {
          messages.value.push({ role: "assistant", text: data.payload.text });
        }
        scrollToBottom();
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
    <!-- Cấy Trực tiếp Bể Nuôi 3D PIXI rộng 500x800 để Chứa Nguyên 1 cái Body Dài -->
    <canvas
      ref="l2dCanvas"
      width="500"
      height="800"
      class="fixed right-0 bottom-[-20px] z-0 pointer-events-auto cursor-pointer object-contain"
    ></canvas>

    <!-- Removed Background Blobs for Full Desktop Window Transparency -->

    <div
      class="glass w-full max-w-[400px] h-[75%] rounded-3xl p-6 flex flex-col relative z-10 animate-fade-in-up mb-[100px] shadow-2xl"
    >
      <header
        class="flex items-center justify-between border-b border-white border-opacity-10 pb-4 mb-4"
      >
        <h1 class="text-white text-2xl font-bold tracking-wider">
          LIVA Assistant
          <span class="text-xs font-normal opacity-70 ml-2">v2.0 NVFP4</span>
        </h1>
        <div class="flex items-center gap-2">
          <span
            v-if="isSensing"
            class="text-[10px] text-green-300 animate-pulse font-mono uppercase tracking-widest"
            >Sensory Active</span
          >
          <div
            :class="[
              'w-3 h-3 rounded-full transition-all duration-500',
              isSensing
                ? 'bg-green-400 shadow-[0_0_15px_rgba(74,222,128,1)]'
                : 'bg-white/20',
            ]"
          ></div>
        </div>
      </header>

      <main
        ref="chatContainer"
        class="flex-1 overflow-y-auto pr-2 space-y-4 scrollbar-hide"
      >
        <div
          v-for="(msg, idx) in messages"
          :key="idx"
          :class="[
            'flex items-end gap-3',
            msg.role === 'user' ? 'flex-row-reverse' : 'flex-row',
          ]"
        >
          <div
            v-if="msg.role === 'assistant'"
            class="w-10 h-10 rounded-full glass flex items-center justify-center shrink-0 text-xl border border-white/20"
          >
            🤖
          </div>
          <div
            :class="[
              'py-3 px-4 shadow-sm text-sm whitespace-pre-wrap leading-relaxed max-w-[85%]',
              msg.role === 'user'
                ? 'bg-white text-purple-900 rounded-2xl rounded-tr-sm font-medium'
                : 'glass text-white/95 rounded-2xl rounded-tl-sm border border-white/10',
            ]"
          >
            {{ msg.text }}
          </div>
        </div>

        <div v-if="isThinking" class="flex items-start gap-4 animate-pulse">
          <div
            class="w-10 h-10 rounded-full glass flex items-center justify-center shrink-0 text-xl border border-white/20 opacity-50"
          >
            🤖
          </div>
          <div class="glass py-3 px-4 rounded-2xl rounded-tl-sm flex gap-1">
            <span
              class="w-2 h-2 bg-white/50 rounded-full animate-bounce"
            ></span>
            <span
              class="w-2 h-2 bg-white/50 rounded-full animate-bounce"
              style="animation-delay: 0.1s"
            ></span>
            <span
              class="w-2 h-2 bg-white/50 rounded-full animate-bounce"
              style="animation-delay: 0.2s"
            ></span>
          </div>
        </div>
      </main>

      <footer
        class="mt-4 pt-4 border-t border-white border-opacity-10 relative"
      >
        <input
          v-model="inputText"
          @keyup.enter="sendMessage"
          type="text"
          placeholder="Nhờ LIVA quét ổ đĩa, check email, tìm Google Drive..."
          class="w-full bg-white bg-opacity-10 border border-white border-opacity-20 text-white placeholder-white/50 px-5 py-3 pr-12 rounded-xl focus:outline-none focus:ring-2 focus:ring-white/40 transition-all font-medium"
        />
        <button
          @click="sendMessage"
          :disabled="!inputText.trim()"
          class="absolute right-4 top-1/2 transform -translate-y-1/2 mt-2 w-8 h-8 rounded-full bg-white text-purple-600 hover:bg-purple-100 disabled:opacity-30 disabled:bg-white/20 flex justify-center items-center font-bold transition-all disabled:cursor-not-allowed"
        >
          ↑
        </button>
      </footer>
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
