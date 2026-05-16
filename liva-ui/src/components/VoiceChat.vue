<template>
  <div class="chat-wrapper">
    <!-- [v25 FIX] System Busy Toast -->
    <div v-if="busyToast" class="busy-toast">
      <span class="busy-icon">⏳</span>
      <span class="busy-text">{{ busyToast }}</span>
    </div>
    
    <div class="chat-display">
      <div v-for="(msg, index) in messages" :key="index" :class="['message', msg.role]">
        <strong>{{ msg.role === 'user' ? 'Bạn' : 'Liva' }}:</strong> {{ msg.text }}
      </div>
      <div v-if="currentAiText" class="message ai">
        <strong>Liva:</strong> {{ currentAiText }}
      </div>
    </div>
    
    <div class="controls">
      <input 
        v-model="textInput" 
        @keyup.enter="sendText" 
        placeholder="Nhap tin nhan... (Enter de gui)"
        class="text-input"
      />
      <button @click="sendText" :disabled="!textInput.trim()" class="send-btn">
        Gui
      </button>
      <button @click="toggleMic" :class="{ recording: isRecording }">
        {{ isRecording ? 'Dang lay... (Ban de dung)' : 'Bat dau noi' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { logger } from '../utils/logger';

const isRecording = ref(false);
const textInput = ref('');
const messages = ref<{role: string, text: string}[]>([]);
const currentAiText = ref('');
const busyToast = ref('');  // [v25 FIX] System busy toast message
let busyToastTimer: ReturnType<typeof setTimeout> | null = null;

let ws: WebSocket | null = null;
let recognition: any = null;
let audioContext: AudioContext | null = null;

// Hàng đợi Audio giúp phát âm thanh không bị gián đoạn (Gapless Playback)
let nextPlayTime = 0;
let activeSources: AudioBufferSourceNode[] = [];

onMounted(() => {
  initWebSocket();
  initSpeechRecognition();
});

onUnmounted(() => {
  ws?.close();
  recognition?.stop();
  audioContext?.close();
  if (busyToastTimer) {
    clearTimeout(busyToastTimer);
    busyToastTimer = null;
  }
});

const initWebSocket = () => {
  ws = new WebSocket('ws://127.0.0.1:8082'); // Trỏ về Openclaw Gateway
  
  ws.onmessage = async (event) => {
    const data = JSON.parse(event.data);
    
    // [v25 FIX] System Busy Toast — hiển thị thông báo thay vì chat bubble
    if (data.event === 'system_busy' || data.type === 'system_busy') {
      const msg = data.data?.message || data.message || 'Liva đang xử lý...';
      busyToast.value = msg;
      // Auto-hide toast after 3 seconds
      if (busyToastTimer) clearTimeout(busyToastTimer);
      busyToastTimer = setTimeout(() => { busyToast.value = ''; }, 3000);
      return;
    }
    
    if (data.type === 'text') {
      currentAiText.value += data.text; // Cập nhật stream text
    } else if (data.type === 'audio') {
      await scheduleAudioChunk(data.data); // Xếp lịch phát đoạn âm thanh
    } else if (data.type === 'turn_end') {
      if (currentAiText.value) {
        messages.value.push({ role: 'ai', text: currentAiText.value });
        currentAiText.value = '';
      }
    }
  };
};

const scheduleAudioChunk = async (base64Audio: string) => {
  if (!audioContext) {
    const AudioContextClass = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
    audioContext = new AudioContextClass();
  }
  if (audioContext.state === 'suspended') await audioContext.resume();

  // Chuyển Base64 MP3 về lại ArrayBuffer
  const binaryString = atob(base64Audio);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) bytes[i] = binaryString.codePointAt(i) as number;

  try {
    const audioBuffer = await audioContext.decodeAudioData(bytes.buffer);
    const source = audioContext.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(audioContext.destination);

    // Toán học cho Gapless: Tính toán thời điểm chính xác để phát đoạn âm thanh này
    const currentTime = audioContext.currentTime;
    if (nextPlayTime < currentTime) nextPlayTime = currentTime + 0.05; // Cộng 50ms buffer để tránh bị vấp tiếng

    source.start(nextPlayTime);
    nextPlayTime += audioBuffer.duration; // Tăng con trỏ thời gian cho chunk TIẾP THEO

    activeSources.push(source);
    source.onended = () => { activeSources = activeSources.filter(s => s !== source); };
  } catch (err) {
    logger.error('[VoiceChat]', 'Lỗi giải mã âm thanh:', err instanceof Error ? err.message : String(err));
  }
};

const stopAudio = () => {
  activeSources.forEach(source => { try { source.stop(); } catch (e) { void e; } });
  activeSources = [];
  nextPlayTime = 0; // Đặt lại timeline phát nhạc
};

const initSpeechRecognition = () => {
  const SpeechRecognition = (globalThis as any).SpeechRecognition || (globalThis as any).webkitSpeechRecognition;
    if (!SpeechRecognition) return logger.warn('[VoiceChat]', 'Trình duyệt không hỗ trợ Web Speech API.');

  recognition = new SpeechRecognition();
  recognition.lang = 'vi-VN'; // Cài đặt tiếng Việt
  recognition.interimResults = true;

  recognition.onresult = (event: any) => {
    let finalTranscript = '';
    for (let i = event.resultIndex; i < event.results.length; ++i) {
      if (event.results[i].isFinal) finalTranscript += event.results[i][0].transcript;
    }
    
    if (finalTranscript) {
      // Dừng đọc nếu người dùng bắt đầu nói chèn vào
      stopAudio(); 
      ws?.send(JSON.stringify({ type: 'interrupt' })); // Gửi tín hiệu Barge-in cho backend
      
      messages.value.push({ role: 'user', text: finalTranscript });
      
      if (currentAiText.value) {
        messages.value.push({ role: 'ai', text: currentAiText.value });
        currentAiText.value = '';
      }
      
      // GỬI TOÀN BỘ NGỮ CẢNH HỘI THOẠI
      ws?.send(JSON.stringify({ type: 'prompt', messages: messages.value }));
    }
  };

  recognition.onend = () => {
      isRecording.value = false;
  };
};

const toggleMic = async () => {
  if (audioContext?.state === 'suspended') await audioContext.resume();

  if (isRecording.value) {
    recognition?.stop();
    isRecording.value = false;
  } else {
    stopAudio();
    ws?.send(JSON.stringify({ type: 'interrupt' }));
    recognition?.start();
    isRecording.value = true;
  }
};

const sendText = () => {
  const text = textInput.value.trim();
  if (!text || !ws) return;
  
  stopAudio();
  ws?.send(JSON.stringify({ type: 'interrupt' }));
  
  messages.value.push({ role: 'user', text });
  
  if (currentAiText.value) {
    messages.value.push({ role: 'ai', text: currentAiText.value });
    currentAiText.value = '';
  }
  
  ws?.send(JSON.stringify({ type: 'prompt', messages: messages.value }));
  textInput.value = '';
};
</script>

<style scoped>
.chat-wrapper { background-color: rgba(255, 255, 255, 0.85); backdrop-filter: blur(10px); display: flex; flex-direction: column; width: 100%; height: 100vh; padding: 20px; font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; box-sizing: border-box; }

/* [v25 FIX] System Busy Toast */
.busy-toast {
  position: fixed;
  top: 20px;
  left: 50%;
  transform: translateX(-50%);
  background: linear-gradient(135deg, #ff9800, #ff5722);
  color: white;
  padding: 12px 24px;
  border-radius: 25px;
  display: flex;
  align-items: center;
  gap: 8px;
  box-shadow: 0 4px 20px rgba(255, 87, 34, 0.4);
  z-index: 1000;
  animation: toast-slide-in 0.3s ease-out;
  font-size: 14px;
  font-weight: 500;
}

.busy-icon { font-size: 18px; }
.busy-text { white-space: nowrap; }

@keyframes toast-slide-in {
  from { transform: translateX(-50%) translateY(-20px); opacity: 0; }
  to { transform: translateX(-50%) translateY(0); opacity: 1; }
}

.chat-display { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; padding-right: 5px;}
.message { padding: 12px 16px; border-radius: 8px; max-width: 80%; line-height: 1.5; font-size: 14px; word-wrap: break-word; }
.user { align-self: flex-end; background: #007bff; color: white; border-bottom-right-radius: 2px;}
.ai { align-self: flex-start; background: #e9ecef; color: black; border-bottom-left-radius: 2px; }
.controls { margin-top: 20px; text-align: center; padding-bottom: 20px; display: flex; gap: 10px; justify-content: center; align-items: center; flex-wrap: wrap; }
.text-input { padding: 12px 16px; font-size: 14px; border: 2px solid #ddd; border-radius: 25px; flex: 1; max-width: 300px; min-width: 150px; outline: none; }
.text-input:focus { border-color: #007bff; }
.send-btn { padding: 12px 20px; font-size: 14px; border: none; border-radius: 25px; cursor: pointer; background: #007bff; color: white; }
.send-btn:disabled { background: #ccc; cursor: not-allowed; }
button { padding: 15px 25px; font-size: 16px; font-weight: bold; border: none; border-radius: 50px; cursor: pointer; transition: all 0.3s ease; background: #28a745; color: white; box-shadow: 0 4px 15px rgba(40, 167, 69, 0.4);}
button.recording { background: #dc3545; box-shadow: 0 4px 15px rgba(220, 53, 69, 0.4); animation: pulse 1.5s infinite; }
@keyframes pulse { 0% { transform: scale(1); box-shadow: 0 0 0 0 rgba(220, 53, 69, 0.7); } 70% { transform: scale(1.05); box-shadow: 0 0 0 10px rgba(220, 53, 69, 0); } 100% { transform: scale(1); box-shadow: 0 0 0 0 rgba(220, 53, 69, 0); } }
::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: rgba(0,0,0,0.2); border-radius: 4px; }
</style>
