<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';

const isSensing = ref(false);

const handleKeydown = async (e: KeyboardEvent) => {
  // Móc nối tổ hợp phím Ctrl + Shift + S
  if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 's') {
    isSensing.value = true;
    try {
        // [Mock] Bắn lệnh đánh thức giác quan qua AI Gateway Server gốc
        await fetch('http://localhost:3000/api/sensory-capture', { method: 'POST' });
        
        // Sensory có TTL 30s nên vạch sáng cảm biến sẽ tắt sau 30s
        setTimeout(() => { isSensing.value = false; }, 30000); 
    } catch(e) {
        console.error("Lỗi giao tiếp kết nối AI:", e);
        // Tạm thời mô phỏng local dev
        setTimeout(() => { isSensing.value = false; }, 30000); 
    }
  }
};

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div class="h-screen w-screen flex items-center justify-center bg-gradient-to-br from-indigo-500 via-purple-500 to-pink-500 bg-cover font-sans relative overflow-hidden">
    <!-- Animated background element -->
    <div class="absolute top-1/4 left-1/4 w-96 h-96 bg-blue-400 rounded-full mix-blend-multiply filter blur-3xl opacity-50 animate-blob"></div>
    <div class="absolute bottom-1/4 right-1/4 w-96 h-96 bg-pink-400 rounded-full mix-blend-multiply filter blur-3xl opacity-50 animate-blob animation-delay-2000"></div>

    <div class="glass w-full max-w-lg h-3/4 rounded-3xl p-8 flex flex-col relative z-10 animate-fade-in-up">
      <header class="flex items-center justify-between border-b border-light-50 border-opacity-10 pb-4 mb-4">
        <h1 class="text-white text-2xl font-bold tracking-wider">LIVA Assistant <span class="text-xs font-normal opacity-70 ml-2">v2.0 NVFP4</span></h1>
        
        <div class="flex items-center gap-2">
            <span v-if="isSensing" class="text-[10px] text-green-300 animate-pulse font-mono uppercase tracking-widest text-shadow-sm">Sensory Active</span>
            <div :class="['w-3 h-3 rounded-full transition-all duration-500', isSensing ? 'bg-green-400 shadow-[0_0_15px_rgba(74,222,128,1)]' : 'bg-white/20']"></div>
        </div>
      </header>
      
      <main class="flex-1 overflow-y-auto pr-2 space-y-4">
        <div class="flex items-start gap-4">
          <div class="w-10 h-10 rounded-full glass flex items-center justify-center shrink-0">
            🤖
          </div>
          <div class="glass py-3 px-4 rounded-2xl rounded-tl-sm text-white/90 shadow-sm leading-relaxed text-sm">
            Xin chào! Mình là LIVA. Mô hình suy luận và bộ nhớ TurboQuant QJL đã được kích hoạt thành công. Mình có thể giúp gì cho bạn?
          </div>
        </div>
      </main>

      <footer class="mt-4 pt-4 border-t border-light-50 border-opacity-10 relative">
        <input 
          type="text" 
          placeholder="Hỏi LIVA điều gì đó..." 
          class="w-full bg-white bg-opacity-10 border border-white border-opacity-20 text-white placeholder-white/50 px-5 py-3 rounded-xl focus:outline-none focus:ring-2 focus:ring-white/40 transition-all"
        />
        <button class="absolute right-4 top-1/2 transform -translate-y-1/2 mt-2 w-8 h-8 rounded-full bg-white bg-opacity-20 hover:bg-opacity-30 flex justify-center items-center text-white transition-all">
          ↑
        </button>
      </footer>
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
.animate-blob {
  animation: blob 7s infinite;
}
.animation-delay-2000 {
  animation-delay: 2s;
}
@keyframes fadeInUp {
  from { opacity: 0; transform: translateY(20px); }
  to { opacity: 1; transform: translateY(0); }
}
.animate-fade-in-up {
  animation: fadeInUp 0.6s ease-out forwards;
}
</style>
