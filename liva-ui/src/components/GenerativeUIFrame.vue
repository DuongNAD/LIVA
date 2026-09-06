<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useLivingCanvas } from '../composables/useLivingCanvas';
import { logger } from '../utils/logger';

const {
  activeWidget,
  widgetStreamStatus,
  streamProgress,
  widgetError,
  sendWidgetAction,
} = useLivingCanvas();

const viewportWidth = ref<'100%' | '768px' | '375px'>('100%');
const showCodeDrawer = ref(false);

const compiledSrcDoc = computed(() => {
  if (!activeWidget.value) return '';

  const widget = activeWidget.value;
  const csp =
    "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data: blob: https:; font-src data:; connect-src 'none';";

  const bootstrapScript = `
    <script>
      (function() {
        window.__LIVA_WIDGET_STATE__ = ${JSON.stringify(widget.props || {})};

        window.onerror = function(msg, url, lineNo, columnNo, error) {
          window.parent.postMessage({
            type: 'LIVA_GENUI_ERROR',
            widget_id: '${widget.widget_id}',
            error: msg
          }, '*');
          return false;
        };

        window.LivaWidget = {
          getState: function() {
            return window.__LIVA_WIDGET_STATE__;
          },
          emitAction: function(action, payload) {
            window.parent.postMessage({
              type: 'LIVA_GENUI_EVENT',
              widget_id: '${widget.widget_id}',
              action: action,
              payload: payload
            }, '*');
          },
          onStateUpdate: function(callback) {
            window.addEventListener('message', function(e) {
              if (e.data && e.data.type === 'LIVA_GENUI_STATE_UPDATE') {
                callback(e.data.payload);
              }
            });
          }
        };

        window.addEventListener('load', function() {
          window.parent.postMessage({
            type: 'LIVA_GENUI_READY',
            widget_id: '${widget.widget_id}'
          }, '*');
        });
      })();
    ${'<' + '/script>'}
  `;

  return `
    <!DOCTYPE html>
    <html>
      <head>
        <meta charset="utf-8">
        <meta http-equiv="Content-Security-Policy" content="${csp}">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <style>
          *, *::before, *::after { box-sizing: border-box; }
          body { margin: 0; padding: 16px; font-family: system-ui, -apple-system, sans-serif; background: #0b0d14; color: #f1f5f9; }
          ${widget.css || ''}
        </style>
      </head>
      <body>
        ${widget.html || ''}
        ${bootstrapScript}
        <script>
          try {
            ${widget.js || ''}
          } catch(e) {
            window.onerror(e.message, 'widget.js', 0, 0, e);
          }
        ${'<' + '/script>'}
      </body>
    </html>
  `;
});

const handleIframeMessage = (event: MessageEvent) => {
  if (!event.data || typeof event.data !== 'object') return;
  const { type, widget_id, action, payload, error } = event.data;

  if (widget_id !== activeWidget.value?.widget_id) return;

  if (type === 'LIVA_GENUI_EVENT') {
    sendWidgetAction(widget_id, action, payload);
  } else if (type === 'LIVA_GENUI_ERROR') {
    logger.error('[GenerativeUIFrame]', `Error from widget ${widget_id}:`, error);
  }
};

onMounted(() => {
  window.addEventListener('message', handleIframeMessage);
});

onUnmounted(() => {
  window.removeEventListener('message', handleIframeMessage);
});
</script>

<template>
  <div class="generative-ui-frame">
    <!-- Viewport Toolbar -->
    <div class="frame-toolbar">
      <div class="toolbar-left">
        <span class="widget-title font-medium text-slate-200">
          {{ activeWidget?.title || 'Interactive Widget' }}
        </span>
        <span v-if="activeWidget" class="version-badge">v{{ activeWidget.version }}</span>
      </div>

      <div class="toolbar-right">
        <!-- Viewport Width Presets -->
        <div class="viewport-presets">
          <button
            :class="['vp-btn', { active: viewportWidth === '100%' }]"
            @click="viewportWidth = '100%'"
          >
            Full
          </button>
          <button
            :class="['vp-btn', { active: viewportWidth === '768px' }]"
            @click="viewportWidth = '768px'"
          >
            Tablet
          </button>
          <button
            :class="['vp-btn', { active: viewportWidth === '375px' }]"
            @click="viewportWidth = '375px'"
          >
            Mobile
          </button>
        </div>

        <button
          @click="showCodeDrawer = !showCodeDrawer"
          class="btn-icon"
          title="Inspect Source"
        >
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
            <path stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Streaming Skeleton Loader -->
    <div v-if="widgetStreamStatus === 'streaming'" class="streaming-skeleton">
      <div class="skeleton-header">
        <div class="skeleton-shimmer h-6 w-48 rounded mb-2"></div>
        <div class="skeleton-shimmer h-4 w-72 rounded"></div>
      </div>
      <div class="streaming-progress">
        <div class="progress-bar">
          <div class="progress-fill"></div>
        </div>
        <span class="text-xs text-purple-400 mt-2 font-mono">
          Streaming widget components... ({{ streamProgress.receivedChunks }} chunks,
          {{ streamProgress.totalBytes }} bytes)
        </span>
      </div>
      <div class="skeleton-body">
        <div class="skeleton-shimmer h-32 w-full rounded mb-3"></div>
        <div class="skeleton-shimmer h-24 w-full rounded"></div>
      </div>
    </div>

    <!-- Error Boundary Display -->
    <div v-else-if="widgetError" class="error-boundary-view">
      <svg
        class="w-10 h-10 text-rose-500 mb-2"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
      >
        <path
          stroke-width="2"
          d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
        />
      </svg>
      <h4 class="text-rose-400 font-semibold mb-1">Widget Rendering Failed</h4>
      <p class="text-slate-400 text-sm mb-3">{{ widgetError }}</p>
      <button @click="showCodeDrawer = true" class="btn btn-sm btn-reject">Inspect Code</button>
    </div>

    <!-- Empty State -->
    <div v-else-if="!activeWidget" class="frame-empty">
      <svg
        class="w-12 h-12 text-slate-600 mb-3"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
      >
        <path
          stroke-width="1.5"
          d="M4 5a1 1 0 011-1h14a1 1 0 011 1v14a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 9h16M9 9v11"
        />
      </svg>
      <p class="text-slate-400 font-medium">No Active Generative UI</p>
      <p class="text-slate-600 text-sm">
        Agent-synthesized interactive widgets and simulations will stream here.
      </p>
    </div>

    <!-- Active Sandboxed Viewport -->
    <div v-else class="viewport-wrapper">
      <iframe
        ref="iframeRef"
        :srcdoc="compiledSrcDoc"
        sandbox="allow-scripts allow-forms allow-popups"
        class="sandboxed-iframe"
        :style="{ width: viewportWidth }"
      ></iframe>
    </div>

    <!-- Code Inspection Drawer -->
    <div v-if="showCodeDrawer && activeWidget" class="code-drawer">
      <div class="drawer-header">
        <span class="font-semibold text-sm">Source Code (HTML/CSS/JS)</span>
        <button @click="showCodeDrawer = false" class="btn-icon">✕</button>
      </div>
      <div class="drawer-content">
        <pre class="code-block"><code>{{ compiledSrcDoc }}</code></pre>
      </div>
    </div>
  </div>
</template>

<style scoped>
.generative-ui-frame {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-primary, #07080d);
  overflow: hidden;
  position: relative;
}

.frame-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 16px;
  background: var(--bg-tertiary, #141724);
  border-bottom: 1px solid var(--border-default, rgba(255, 255, 255, 0.06));
}

.toolbar-left,
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.version-badge {
  font-size: 10px;
  padding: 1px 6px;
  background: rgba(168, 85, 247, 0.2);
  color: #c084fc;
  border-radius: 4px;
  font-family: monospace;
}

.viewport-presets {
  display: flex;
  background: rgba(0, 0, 0, 0.3);
  padding: 2px;
  border-radius: 6px;
}

.vp-btn {
  padding: 2px 8px;
  font-size: 11px;
  color: #94a3b8;
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.vp-btn.active {
  background: var(--bg-elevated, #1a1e2f);
  color: #f1f5f9;
}

.viewport-wrapper {
  flex: 1;
  display: flex;
  justify-content: center;
  align-items: stretch;
  overflow: auto;
  padding: 12px;
  background: #040508;
}

.sandboxed-iframe {
  height: 100%;
  border: 1px solid var(--border-default, rgba(255, 255, 255, 0.08));
  border-radius: 8px;
  background: #0b0d14;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
  transition: width 0.2s ease;
}

.streaming-skeleton {
  flex: 1;
  padding: 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.skeleton-shimmer {
  background: linear-gradient(
    90deg,
    rgba(255, 255, 255, 0.03) 25%,
    rgba(255, 255, 255, 0.08) 50%,
    rgba(255, 255, 255, 0.03) 75%
  );
  background-size: 200% 100%;
  animation: shimmer 1.8s infinite;
}

@keyframes shimmer {
  0% {
    background-position: 200% 0;
  }
  100% {
    background-position: -200% 0;
  }
}

.progress-bar {
  height: 4px;
  width: 100%;
  background: rgba(255, 255, 255, 0.08);
  border-radius: 2px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  width: 60%;
  background: linear-gradient(90deg, #a855f7, #6366f1);
  animation: progress-indeterminate 1.5s infinite ease-in-out;
}

@keyframes progress-indeterminate {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(200%);
  }
}

.code-drawer {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 50%;
  background: #0c0e17;
  border-top: 1px solid rgba(168, 85, 247, 0.3);
  display: flex;
  flex-direction: column;
  z-index: 30;
}

.drawer-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 16px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
}

.drawer-content {
  flex: 1;
  overflow: auto;
  padding: 12px;
}

.code-block {
  font-family: monospace;
  font-size: 11px;
  color: #a5f3fc;
  white-space: pre-wrap;
}

.frame-empty,
.error-boundary-view {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 24px;
}

.btn-icon {
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
}
.btn-icon:hover {
  background: rgba(255, 255, 255, 0.08);
  color: #f1f5f9;
}
</style>
