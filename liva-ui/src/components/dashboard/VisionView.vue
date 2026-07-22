<script setup lang="ts">
/**
 * VisionView.vue — "Nhìn màn hình"
 * ================================
 * Bấm để LIVA (lõi hợp nhất Qwen3-VL) chụp & mô tả màn hình qua `vision:ask`.
 * Khi đang chơi game, backend tự cắt ảnh nhỏ quanh con trỏ (Mouse-Guided) để
 * nhẹ máy. Yêu cầu core chạy bản RELEASE (vision tắt ở debug).
 */
import { ref } from 'vue';
import { useGateway } from '../../composables/useGateway';

const {
  askVision, visionAnswer, visionBusy, visionError, isConnected,
  watchActive, watchStarting, watchError, watchLastDiff, watchEvents,
  startScreenWatch, stopScreenWatch,
} = useGateway();
const question = ref('');

const onAsk = () => {
  if (visionBusy.value) return;
  askVision(question.value);
};

const errorText = (code: string): string => {
  if (code === 'timeout') {
    return 'Hết thời gian chờ. Đảm bảo LIVA Core chạy bản release (vision cần release).';
  }
  if (code === 'not_connected') return 'Chưa kết nối tới LIVA Core.';
  // Lõi nay trả về lý do thật thay vì để client chờ hết giờ. Trường hợp hay
  // gặp nhất là chạy bản debug — dịch sang tiếng Việt kèm cách khắc phục,
  // thay vì ném nguyên văn tiếng Anh của llama.cpp vào giao diện.
  if (code.includes('requires a release build')) {
    return 'Vision cần bản build release. Chạy lõi bằng `cargo build --release` rồi thử lại '
      + '(bản debug bung assert trong bộ nạp mmproj của llama.cpp).';
  }
  if (code.includes('mmproj')) {
    return 'Chưa cấu hình mô hình thị giác. Kiểm tra `ai.mmprojModel` trong data/liva-config.json.';
  }
  return code;
};
</script>

<template>
  <div class="vision-view">
    <header class="vision-header">
      <h2>Nhìn màn hình</h2>
      <p class="vision-sub">
        LIVA dùng lõi Qwen3-VL đọc màn hình của bạn. Khi có game chạy toàn màn hình,
        ảnh được cắt nhỏ quanh con trỏ chuột để nhẹ máy — không làm tụt FPS.
      </p>
    </header>

    <div class="vision-controls">
      <textarea
        v-model="question"
        class="vision-input"
        rows="2"
        placeholder="Hỏi gì về màn hình? (để trống = mô tả nhanh)"
        :disabled="visionBusy"
        @keydown.enter.exact.prevent="onAsk"
      ></textarea>
      <button class="vision-btn" :disabled="visionBusy || !isConnected" @click="onAsk">
        <span v-if="visionBusy" class="spinner" aria-hidden="true"></span>
        <span>{{ visionBusy ? 'Đang nhìn…' : 'Nhìn màn hình' }}</span>
      </button>
    </div>

    <div v-if="visionError" class="vision-error">{{ errorText(visionError) }}</div>

    <div v-if="visionAnswer" class="vision-answer">
      <div class="vision-answer-label">LIVA thấy</div>
      <p>{{ visionAnswer }}</p>
    </div>
    <div v-else-if="!visionBusy && !visionError" class="vision-hint">
      Bấm “Nhìn màn hình” để LIVA chụp và mô tả những gì đang hiển thị. Bạn cũng có
      thể ra lệnh bằng giọng nói: <em>“LIVA, nhìn màn hình xem có gì.”</em>
    </div>

    <!-- Canh chừng màn hình: diff vùng mỗi 3 giây, KHÔNG gọi model — chỉ so
         điểm ảnh, nên nhẹ hơn "Nhìn màn hình" rất nhiều. -->
    <section class="watch-card">
      <div class="watch-head">
        <div>
          <h3>Canh chừng màn hình</h3>
          <p class="watch-sub">
            So khung hình mỗi 3 giây và ghi lại lúc màn hình thay đổi — không dùng
            model, không tốn GPU. Hữu ích khi chờ build xong, render xong, hay một
            trang web đổi nội dung.
          </p>
        </div>
        <button
          class="vision-btn watch-toggle"
          :class="{ active: watchActive }"
          :disabled="watchStarting || !isConnected"
          @click="watchActive ? stopScreenWatch() : startScreenWatch()"
        >
          <span v-if="watchStarting" class="spinner" aria-hidden="true"></span>
          <span>{{ watchStarting ? 'Đang bật…' : watchActive ? 'Dừng canh chừng' : 'Bắt đầu canh chừng' }}</span>
        </button>
      </div>

      <div v-if="watchError" class="vision-error">{{ watchError }}</div>

      <div v-if="watchActive" class="watch-status">
        <span class="watch-dot" aria-hidden="true"></span>
        Đang canh chừng — thay đổi khung gần nhất: {{ (watchLastDiff * 100).toFixed(1) }}%
      </div>

      <ul v-if="watchEvents.length" class="watch-events">
        <li v-for="(ev, i) in watchEvents" :key="`${ev.time}-${i}`">
          <span class="watch-time">{{ ev.time }}</span>
          <span>màn hình thay đổi {{ (ev.difference * 100).toFixed(1) }}%</span>
        </li>
      </ul>
      <p v-else-if="watchActive" class="vision-hint">Chưa ghi nhận thay đổi nào.</p>
    </section>
  </div>
</template>

<style scoped>
.vision-view {
  height: 100%;
  overflow-y: auto;
  padding: 28px 32px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.vision-header h2 {
  margin: 0 0 6px;
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
}

.vision-sub {
  margin: 0;
  max-width: 64ch;
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1.55;
}

.vision-controls {
  display: flex;
  gap: 12px;
  align-items: stretch;
}

.vision-input {
  flex: 1;
  resize: vertical;
  min-height: 46px;
  padding: 12px 14px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--bg-primary);
  color: var(--text-primary);
  font: inherit;
  font-size: 14px;
  outline: none;
  transition: border-color var(--transition-fast);
}

.vision-input:focus {
  border-color: #818cf8;
}

.vision-btn {
  flex-shrink: 0;
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 12px 20px;
  border: none;
  border-radius: var(--radius-md);
  background: linear-gradient(135deg, var(--accent-start, #a855f7), var(--accent-end, #6366f1));
  color: #fff;
  font-weight: 600;
  font-size: 14px;
  cursor: pointer;
  transition: opacity var(--transition-fast), transform var(--transition-fast);
}

.vision-btn:hover:not(:disabled) {
  transform: translateY(-1px);
}

.vision-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff;
  border-radius: 50%;
  animation: vspin 0.7s linear infinite;
}

@keyframes vspin {
  to { transform: rotate(360deg); }
}

.vision-error {
  padding: 12px 14px;
  border: 1px solid rgba(224, 107, 107, 0.4);
  background: rgba(224, 107, 107, 0.08);
  border-radius: var(--radius-md);
  color: #e06b6b;
  font-size: 13px;
}

.vision-answer {
  padding: 16px 18px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--bg-primary);
}

.vision-answer-label {
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: #818cf8;
  font-weight: 600;
  margin-bottom: 8px;
}

.vision-answer p {
  margin: 0;
  color: var(--text-primary);
  font-size: 15px;
  line-height: 1.6;
  white-space: pre-wrap;
}

.vision-hint {
  color: var(--text-muted, var(--text-secondary));
  font-size: 13px;
  line-height: 1.55;
  max-width: 64ch;
}

.vision-hint em {
  color: var(--text-secondary);
}

/* ── Canh chừng màn hình ── */
.watch-card {
  border: 1px solid var(--border-color, rgba(128, 128, 128, 0.25));
  border-radius: 12px;
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.watch-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
}

.watch-head h3 {
  margin: 0 0 4px;
  font-size: 15px;
}

.watch-sub {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12.5px;
  line-height: 1.5;
  max-width: 52ch;
}

.watch-toggle.active {
  background: var(--danger, #b4453a);
}

.watch-status {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-secondary);
}

.watch-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--success, #3d8f4d);
  animation: watchPulse 1.6s ease-in-out infinite;
}

@keyframes watchPulse {
  50% { opacity: 0.35; }
}

@media (prefers-reduced-motion: reduce) {
  .watch-dot { animation: none; }
}

.watch-events {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 13px;
  max-height: 220px;
  overflow-y: auto;
}

.watch-events li {
  display: flex;
  gap: 10px;
  align-items: baseline;
}

.watch-time {
  font-variant-numeric: tabular-nums;
  color: var(--text-secondary);
  font-size: 12px;
}
</style>
