<template>
  <div class="chat-wrapper">
    <div class="connection-pill" :class="{ online: isConnected, connecting: isConnecting }">
      <span class="connection-dot"></span>
      <span>{{ isConnected ? 'Đã kết nối Gateway' : isConnecting ? 'Đang kết nối Gateway...' : 'Mất kết nối Gateway' }}</span>
    </div>

    <!-- [v25 FIX] System Busy Toast -->
    <div v-if="busyToast" class="busy-toast">
      <span class="busy-icon">⏳</span>
      <span class="busy-text">{{ busyToast }}</span>
    </div>
    
    <div class="chat-display">
      <div v-for="(msg, index) in messages" :key="index" :class="['message', msg.role]">
        <strong>{{ msg.role === 'user' ? 'Bạn' : 'Liva' }}:</strong> {{ msg.text }}
      </div>
      <div v-if="isConnecting" class="message system">Đang chờ Gateway trả kết nối...</div>
      <div v-if="currentAiText" class="message ai">
        <strong>Liva:</strong> {{ currentAiText }}
      </div>
    </div>
    
    <div class="controls">
      <input 
        v-model="textInput" 
        @keyup.enter="sendText" 
        :placeholder="canSend ? 'Nhap tin nhan... (Enter de gui)' : 'Dang doi ket noi Gateway...'
        "
        :disabled="!canSend"
        class="text-input"
      />
      <button @click="sendText" :disabled="!canSend || !textInput.trim()" class="send-btn">
        Gui
      </button>
      <button @click="toggleMic" :class="{ recording: isRecording }">
        {{ isRecording ? 'Dang lay... (Ban de dung)' : 'Bat dau noi' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, shallowRef, triggerRef, inject } from 'vue';
import type { IPlatformAdapter } from '../platform/IPlatformAdapter';
import { logger } from '../utils/logger';

const platform = inject<IPlatformAdapter>('platform');

const isRecording = ref(false);
const isConnecting = ref(true);
const isConnected = ref(false);
const textInput = ref('');
const messages = ref<{ role: string; text: string }[]>([]);
const currentAiText = shallowRef('');
const busyToast = ref('');
let busyToastTimer: ReturnType<typeof setTimeout> | null = null;

let ws: WebSocket | null = null;
let recognition: any = null;
let audioContext: AudioContext | null = null;
let recognitionBusy = false;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

// Hàng đợi Audio giúp phát âm thanh không bị gián đoạn (Gapless Playback)
let nextPlayTime = 0;
let activeSources: AudioBufferSourceNode[] = [];

const canSend = computed(() => isConnected.value && !!ws && ws.readyState === WebSocket.OPEN);

onMounted(() => {
  initWebSocket();
  initSpeechRecognition();
});

onUnmounted(() => {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  try { ws?.close(); } catch { /* noop */ }
  try { recognition?.stop(); } catch { /* noop */ }
  if (audioContext && audioContext.state !== 'closed') {
    audioContext.close().catch(() => {});
  }
  if (busyToastTimer) {
    clearTimeout(busyToastTimer);
    busyToastTimer = null;
  }
  stopAudio();
});

const setBusyToast = (message: string) => {
  busyToast.value = message;
  if (busyToastTimer) clearTimeout(busyToastTimer);
  busyToastTimer = setTimeout(() => {
    busyToast.value = '';
  }, 3000);
};

const appendAiText = (text: string) => {
  currentAiText.value += text;
  triggerRef(currentAiText);
};

const flushCurrentAiText = () => {
  if (currentAiText.value.trim()) {
    messages.value.push({ role: 'ai', text: currentAiText.value });
    currentAiText.value = '';
  }
};

const sendToGateway = (payload: Record<string, unknown>) => {
  if (!canSend.value) {
    logger.warn('[VoiceChat]', 'WebSocket chưa sẵn sàng, bỏ qua payload:', payload);
    return false;
  }

  try {
    ws!.send(JSON.stringify(payload));
    return true;
  } catch (err) {
    logger.error('[VoiceChat]', 'Không thể gửi WebSocket payload:', err instanceof Error ? err.message : String(err));
    return false;
  }
};

const sendInterrupt = () => {
  stopAudio();
  sendToGateway({ event: 'interrupt', type: 'interrupt' });
  if (canSend.value) {
    try {
      ws!.send('[INTERRUPT]');
    } catch {
      // ignore fallback send error
    }
  }
};

const initWebSocket = () => {
  if (platform) {
    platform.onGatewayReady((port, token) => {
      isConnecting.value = true;
      const wsUrl = token ? `ws://127.0.0.1:${port}?token=${token}` : `ws://127.0.0.1:${port}`;
      ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        isConnecting.value = false;
        isConnected.value = true;
        logger.info('[VoiceChat]', `WebSocket connected to Gateway on port ${port}`);
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  };

  ws.onclose = () => {
    isConnecting.value = false;
    isConnected.value = false;
    isRecording.value = false;
    logger.warn('[VoiceChat]', 'WebSocket disconnected from Gateway');
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      if (!ws || ws.readyState === WebSocket.CLOSED) {
        initWebSocket();
      }
    }, 3000);
  };

  ws.onerror = (err) => {
    isConnecting.value = false;
    isConnected.value = false;
    logger.error('[VoiceChat]', 'WebSocket error:', err);
    try { ws?.close(); } catch { /* noop */ }
  };

  ws.onmessage = async (event) => {
    if (typeof event.data !== 'string') {
      return;
    }

    let data: any;
    try {
      data = JSON.parse(event.data);
    } catch (err) {
      logger.warn('[VoiceChat]', 'Bỏ qua message không phải JSON:', err instanceof Error ? err.message : String(err));
      return;
    }

    if (data.event === 'system_busy' || data.type === 'system_busy') {
      const msg = data.data?.message || data.message || 'Liva đang xử lý...';
      setBusyToast(msg);
      return;
    }

    if (data.type === 'text' || data.event === 'ai_stream_chunk') {
      appendAiText(String(data.text ?? data.payload?.textChunk ?? ''));
      return;
    }

    if (data.type === 'audio' || data.event === 'ai_audio_chunk') {
      const audioBase64 = String(data.data ?? data.payload?.audio ?? '');
      if (audioBase64) {
        await scheduleAudioChunk(audioBase64);
      }
      return;
    }

    if (data.type === 'turn_end' || data.event === 'ai_spoken_response') {
      flushCurrentAiText();
    }
  };
    });
  }
};

const scheduleAudioChunk = async (base64Audio: string) => {
  if (!base64Audio) return;

  if (!audioContext) {
    const AudioContextClass = globalThis.AudioContext || (globalThis as any).webkitAudioContext;
    if (!AudioContextClass) {
      logger.warn('[VoiceChat]', 'Trình duyệt không hỗ trợ Web Audio API.');
      return;
    }
    audioContext = new AudioContextClass();
  }

  if (audioContext.state === 'suspended') {
    try {
      await audioContext.resume();
    } catch (err) {
      logger.warn('[VoiceChat]', 'Không thể resume AudioContext:', err instanceof Error ? err.message : String(err));
      return;
    }
  }

  try {
    const binaryString = atob(base64Audio);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }

    const audioBuffer = await audioContext.decodeAudioData(bytes.buffer.slice(0));
    const source = audioContext.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(audioContext.destination);

    const currentTime = audioContext.currentTime;
    if (nextPlayTime < currentTime) nextPlayTime = currentTime + 0.05;

    source.start(nextPlayTime);
    nextPlayTime += audioBuffer.duration;

    activeSources.push(source);
    source.onended = () => {
      activeSources = activeSources.filter(s => s !== source);
    };
  } catch (err) {
    logger.warn('[VoiceChat]', 'Lỗi giải mã/phát âm thanh:', err instanceof Error ? err.message : String(err));
  }
};

const stopAudio = () => {
  activeSources.forEach(source => {
    try {
      source.stop();
    } catch {
      // ignore source state race
    }
  });
  activeSources = [];
  nextPlayTime = audioContext ? audioContext.currentTime : 0;
};

const initSpeechRecognition = () => {
  const SpeechRecognition = (globalThis as any).SpeechRecognition || (globalThis as any).webkitSpeechRecognition;
  if (!SpeechRecognition) {
    logger.warn('[VoiceChat]', 'Trình duyệt không hỗ trợ SpeechRecognition.');
    return;
  }

  try {
    recognition = new SpeechRecognition();
    recognition.lang = 'vi-VN';
    recognition.interimResults = true;
    recognition.continuous = false;

    recognition.onstart = () => {
      isRecording.value = true;
      recognitionBusy = false;
    };

    recognition.onresult = (event: any) => {
      let finalTranscript = '';
      for (let i = event.resultIndex; i < event.results.length; ++i) {
        if (event.results[i].isFinal) finalTranscript += event.results[i][0].transcript;
      }

      const transcript = finalTranscript.trim();
      if (!transcript || recognitionBusy) return;
      recognitionBusy = true;

      sendInterrupt();
      messages.value.push({ role: 'user', text: transcript });
      flushCurrentAiText();

      const sent = sendToGateway({
        event: 'user_voice_command',
        type: 'user_voice_command',
        payload: { text: transcript },
      });

      if (!sent) {
        recognitionBusy = false;
      }
    };

    recognition.onerror = (event: any) => {
      recognitionBusy = false;
      isRecording.value = false;
      logger.warn('[VoiceChat]', 'SpeechRecognition error:', event?.error || event);
    };

    recognition.onend = () => {
      recognitionBusy = false;
      isRecording.value = false;
    };
  } catch (err) {
    logger.error('[VoiceChat]', 'Không thể khởi tạo SpeechRecognition:', err instanceof Error ? err.message : String(err));
    recognition = null;
  }
};

const toggleMic = async () => {
  if (audioContext?.state === 'suspended') {
    try {
      await audioContext.resume();
    } catch {
      // ignore
    }
  }

  if (!recognition) {
    logger.warn('[VoiceChat]', 'Mic không khả dụng trên trình duyệt hiện tại.');
    setBusyToast('Trình duyệt không hỗ trợ mic.');
    return;
  }

  if (isRecording.value) {
    try {
      recognition.stop();
    } catch (err) {
      logger.warn('[VoiceChat]', 'Không thể dừng SpeechRecognition:', err instanceof Error ? err.message : String(err));
    }
    isRecording.value = false;
    recognitionBusy = false;
    return;
  }

  sendInterrupt();

  try {
    recognition.start();
  } catch (err) {
    logger.warn('[VoiceChat]', 'Không thể bắt đầu SpeechRecognition:', err instanceof Error ? err.message : String(err));
    isRecording.value = false;
  }
};

const sendText = () => {
  const text = textInput.value.trim();
  if (!text) return;

  if (!canSend.value) {
    setBusyToast('Chưa kết nối tới Gateway.');
    return;
  }

  sendInterrupt();
  messages.value.push({ role: 'user', text });
  flushCurrentAiText();

  const sent = sendToGateway({
    event: 'user_voice_command',
    type: 'user_voice_command',
    payload: { text },
  });

  if (sent) {
    textInput.value = '';
  }
};
</script>

<style scoped>
.chat-wrapper { background-color: rgba(255, 255, 255, 0.85); backdrop-filter: blur(10px); display: flex; flex-direction: column; width: 100%; height: 100vh; padding: 20px; font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; box-sizing: border-box; position: relative; }

.connection-pill {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  align-self: flex-start;
  margin-bottom: 12px;
  padding: 8px 12px;
  border-radius: 999px;
  background: rgba(0,0,0,0.08);
  color: #444;
  font-size: 12px;
}

.connection-pill.online { background: rgba(16, 185, 129, 0.12); color: #0f766e; }
.connection-pill.connecting { background: rgba(245, 158, 11, 0.12); color: #b45309; }
.connection-dot { width: 8px; height: 8px; border-radius: 999px; background: currentColor; opacity: 0.9; }

.message.system { align-self: center; background: rgba(0,0,0,0.06); color: #555; }


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
