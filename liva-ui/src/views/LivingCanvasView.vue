<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useLivingCanvas } from '../composables/useLivingCanvas';
import DiffReviewer from '../components/DiffReviewer.vue';
import GenerativeUIFrame from '../components/GenerativeUIFrame.vue';

const {
  layoutMode,
  splitRatio,
  setLayoutMode,
  setSplitRatio,
  pendingHunksCount,
  widgetStreamStatus,
  fetchPendingHunks,
  fetchCanvasState,
} = useLivingCanvas();

const isDragging = ref(false);
const containerRef = ref<HTMLElement | null>(null);

const startDrag = () => {
  isDragging.value = true;
  document.body.style.userSelect = 'none';

  const onMove = (moveEvent: MouseEvent | TouchEvent) => {
    if (!isDragging.value || !containerRef.value) return;
    const rect = containerRef.value.getBoundingClientRect();
    const clientX = 'touches' in moveEvent ? moveEvent.touches[0].clientX : moveEvent.clientX;
    const ratio = (clientX - rect.left) / rect.width;
    setSplitRatio(ratio);
  };

  const onEnd = () => {
    isDragging.value = false;
    document.body.style.userSelect = '';
    window.removeEventListener('mousemove', onMove);
    window.removeEventListener('mouseup', onEnd);
    window.removeEventListener('touchmove', onMove);
    window.removeEventListener('touchend', onEnd);
  };

  window.addEventListener('mousemove', onMove);
  window.addEventListener('mouseup', onEnd);
  window.addEventListener('touchmove', onMove);
  window.addEventListener('touchend', onEnd);
};

onMounted(async () => {
  await Promise.all([fetchPendingHunks(), fetchCanvasState()]);
});
</script>

<template>
  <div ref="containerRef" class="living-canvas-view">
    <!-- Top Global Workspace Bar -->
    <header class="workspace-header">
      <div class="header-left">
        <div class="logo-badge">
          <div class="pulse-dot"></div>
          <span class="font-bold text-sm bg-gradient-to-r from-purple-400 to-indigo-400 bg-clip-text text-transparent">
            Living Canvas
          </span>
        </div>

        <!-- Mode Selectors -->
        <div class="mode-toggles">
          <button
            :class="['mode-btn', { active: layoutMode === 'diff' }]"
            @click="setLayoutMode('diff')"
          >
            Diff Reviewer
            <span v-if="pendingHunksCount > 0" class="badge-count">{{ pendingHunksCount }}</span>
          </button>
          <button
            :class="['mode-btn', { active: layoutMode === 'canvas' }]"
            @click="setLayoutMode('canvas')"
          >
            Generative UI
            <span v-if="widgetStreamStatus === 'streaming'" class="stream-indicator">●</span>
          </button>
          <button
            :class="['mode-btn', { active: layoutMode === 'hybrid' }]"
            @click="setLayoutMode('hybrid')"
          >
            Split Canvas
          </button>
        </div>
      </div>

      <div class="header-right">
        <!-- Split Presets -->
        <div v-if="layoutMode === 'hybrid'" class="split-presets">
          <button class="preset-btn" @click="setSplitRatio(0.3)">30 / 70</button>
          <button class="preset-btn" @click="setSplitRatio(0.5)">50 / 50</button>
          <button class="preset-btn" @click="setSplitRatio(0.7)">70 / 30</button>
        </div>
      </div>
    </header>

    <!-- Workspace Split Viewport -->
    <main class="workspace-body" :class="`layout-${layoutMode}`">
      <!-- Left / Primary Pane: Diff Reviewer -->
      <section
        v-show="layoutMode === 'diff' || layoutMode === 'hybrid'"
        class="pane pane-diff"
        :style="layoutMode === 'hybrid' ? { flex: `0 0 ${splitRatio * 100}%` } : {}"
      >
        <DiffReviewer />
      </section>

      <!-- Drag Handle Splitter -->
      <div
        v-if="layoutMode === 'hybrid'"
        class="drag-splitter"
        :class="{ dragging: isDragging }"
        @mousedown="startDrag"
        @touchstart="startDrag"
      >
        <div class="splitter-line"></div>
        <div class="splitter-grip">
          <svg class="w-3 h-3 text-slate-400" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 6a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm0 6a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm0 6a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm12-12a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm0 6a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm0 6a2 2 0 1 1-4 0 2 2 0 0 1 4 0z" />
          </svg>
        </div>
        <!-- Pointer-shield overlay during drag -->
        <div v-if="isDragging" class="drag-shield"></div>
      </div>

      <!-- Right / Secondary Pane: Generative UI Canvas Frame -->
      <section
        v-show="layoutMode === 'canvas' || layoutMode === 'hybrid'"
        class="pane pane-canvas"
        :style="layoutMode === 'hybrid' ? { flex: `0 0 ${(1 - splitRatio) * 100}%` } : {}"
      >
        <GenerativeUIFrame />
      </section>
    </main>
  </div>
</template>

<style scoped>
.living-canvas-view {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-primary, #07080d);
  color: #f1f5f9;
  overflow: hidden;
}

.workspace-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 16px;
  background: var(--bg-secondary, #0f111a);
  border-bottom: 1px solid var(--border-default, rgba(255, 255, 255, 0.08));
  flex-shrink: 0;
}

.header-left,
.header-right {
  display: flex;
  align-items: center;
  gap: 16px;
}

.logo-badge {
  display: flex;
  align-items: center;
  gap: 8px;
}

.pulse-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #a855f7;
  box-shadow: 0 0 8px #a855f7;
}

.mode-toggles {
  display: flex;
  background: rgba(0, 0, 0, 0.3);
  padding: 2px;
  border-radius: 6px;
  gap: 2px;
}

.mode-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 12px;
  font-size: 12px;
  font-weight: 500;
  color: #94a3b8;
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.mode-btn.active {
  background: var(--bg-elevated, #1a1e2f);
  color: #ffffff;
}

.badge-count {
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 10px;
  background: rgba(245, 158, 11, 0.25);
  color: #fbbf24;
}

.stream-indicator {
  color: #a855f7;
  animation: pulse 1s infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.split-presets {
  display: flex;
  gap: 4px;
}

.preset-btn {
  font-size: 11px;
  padding: 3px 8px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 4px;
  color: #94a3b8;
  cursor: pointer;
}
.preset-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: #f1f5f9;
}

.workspace-body {
  flex: 1;
  display: flex;
  overflow: hidden;
  position: relative;
}

.pane {
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.pane-diff {
  background: var(--bg-secondary, #0f111a);
}

.pane-canvas {
  background: var(--bg-primary, #07080d);
}

.layout-diff .pane-diff {
  flex: 1 1 100% !important;
}

.layout-canvas .pane-canvas {
  flex: 1 1 100% !important;
}

.drag-splitter {
  width: 9px;
  cursor: col-resize;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.2);
  user-select: none;
  z-index: 10;
  transition: background 0.15s ease;
}

.drag-splitter:hover,
.drag-splitter.dragging {
  background: rgba(168, 85, 247, 0.2);
}

.splitter-line {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  background: rgba(255, 255, 255, 0.08);
}

.splitter-grip {
  position: absolute;
  width: 16px;
  height: 28px;
  border-radius: 4px;
  background: #1a1e2f;
  border: 1px solid rgba(255, 255, 255, 0.12);
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
}

.drag-shield {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 9999;
  cursor: col-resize;
}
</style>
