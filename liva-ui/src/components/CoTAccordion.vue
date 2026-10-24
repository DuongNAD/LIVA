<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { renderSafeRichText } from '../utils/richText';

const props = defineProps<{
  thinking: string;
  isStreaming: boolean;
  startTime?: number;
  endTime?: number;
}>();

// Open by default if currently streaming; collapsed if already completed
const isOpen = ref(props.isStreaming);
const manualToggle = ref(false);
const localElapsedMs = ref(0);
let timerHandle: ReturnType<typeof setInterval> | null = null;

function getNow(): number {
  return Date.now();
}

function startTimer() {
  if (timerHandle) clearInterval(timerHandle);
  const t0 = props.startTime ?? getNow();
  localElapsedMs.value = Math.max(0, getNow() - t0);
  timerHandle = setInterval(() => {
    localElapsedMs.value = Math.max(0, getNow() - t0);
  }, 100);
}

function stopTimer() {
  if (timerHandle) {
    clearInterval(timerHandle);
    timerHandle = null;
  }
  if (props.startTime && props.endTime) {
    localElapsedMs.value = Math.max(0, props.endTime - props.startTime);
  }
}

onMounted(() => {
  if (props.isStreaming) {
    startTimer();
  } else if (props.startTime && props.endTime) {
    localElapsedMs.value = Math.max(0, props.endTime - props.startTime);
  }
});

watch(
  () => props.isStreaming,
  (streaming) => {
    if (streaming) {
      if (!manualToggle.value) {
        isOpen.value = true;
      }
      startTimer();
    } else {
      stopTimer();
      if (!manualToggle.value) {
        isOpen.value = false;
      }
    }
  }
);

onUnmounted(() => {
  stopTimer();
});

function toggleAccordion() {
  manualToggle.value = true;
  isOpen.value = !isOpen.value;
}

// Token count estimation (~3.2 chars per token for bilingual technical text)
const estimatedTokens = computed(() => {
  if (!props.thinking) return 0;
  return Math.max(1, Math.round(props.thinking.length / 3.2));
});

const formattedElapsed = computed(() => {
  const sec = (localElapsedMs.value / 1000).toFixed(1);
  return `${sec}s`;
});

defineExpose({
  isOpen,
  manualToggle,
  toggleAccordion,
  localElapsedMs,
});
</script>

<template>
  <div
    class="cot-accordion-root w-full my-1.5 rounded-xl border border-purple-500/25 bg-purple-950/25 backdrop-blur-sm overflow-hidden transition-all duration-200"
    data-testid="cot-accordion"
  >
    <!-- Header / Toggle Button -->
    <button
      type="button"
      class="cot-header w-full px-3 py-2 flex items-center justify-between text-xs text-purple-300 hover:text-purple-200 transition-colors cursor-pointer select-none border-none bg-transparent text-left"
      data-testid="cot-toggle-btn"
      @click="toggleAccordion"
    >
      <div class="flex items-center gap-2">
        <span
          class="cot-chevron inline-block text-[10px] transform transition-transform duration-200"
          :class="{ 'rotate-90': isOpen }"
          data-testid="cot-chevron"
        >▶</span>
        <span class="font-medium flex items-center gap-1.5">
          <span class="cot-pulse-dot" v-if="isStreaming" data-testid="cot-pulse-dot"></span>
          <span data-testid="cot-status-text">
            {{ isStreaming ? 'Reasoning Process...' : 'Reasoning Process' }}
          </span>
        </span>
      </div>
      <div class="flex items-center gap-2 text-[11px] text-purple-400/80 font-mono">
        <span data-testid="cot-elapsed">{{ formattedElapsed }}</span>
        <span>•</span>
        <span data-testid="cot-tokens">{{ estimatedTokens }} tokens</span>
      </div>
    </button>

    <!-- Collapsible Body -->
    <div
      v-show="isOpen"
      class="cot-content px-3.5 pb-3 pt-1 border-t border-purple-500/10 text-xs text-gray-300 leading-relaxed font-sans overflow-x-auto"
      data-testid="cot-content"
    >
      <div
        class="cot-body whitespace-pre-wrap select-text"
        data-testid="cot-body"
        v-html="renderSafeRichText(thinking)"
      ></div>
    </div>
  </div>
</template>

<style scoped>
.cot-pulse-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background-color: #c084fc;
  box-shadow: 0 0 8px #a855f7;
  animation: cotPulse 1.2s infinite ease-in-out;
}

@keyframes cotPulse {
  0%, 100% {
    opacity: 0.4;
    transform: scale(0.9);
  }
  50% {
    opacity: 1;
    transform: scale(1.2);
  }
}

.cot-chevron {
  font-size: 9px;
  line-height: 1;
}

.cot-body :deep(code) {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  background: rgba(255, 255, 255, 0.08);
  padding: 1px 4px;
  border-radius: 4px;
  font-size: 0.9em;
}
</style>
