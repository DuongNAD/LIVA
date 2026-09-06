<script setup lang="ts">
/**
 * BrowserPreviewView.vue — Headless Browser Automation Preview
 * ==============================================================
 * Live viewport preview of automated browser sessions, interactive
 * address navigation, DOM hierarchy extraction, and execution action timeline.
 */
import { ref, onMounted, onUnmounted, computed } from "vue";
import { useGateway } from "../../composables/useGateway";
import { logger } from "../../utils/logger";
import type { BrowserActionRecord } from "liva-common";

const gateway = useGateway();
const loading = ref(true);
const targetUrl = ref("https://liva.ai/dashboard");
const isNavigating = ref(false);
const isCapturing = ref(false);
const activeDomTab = ref<"preview" | "dom" | "timeline">("preview");
const domExtractionMode = ref<"accessibility" | "semantic" | "plain_text" | "html">("semantic");
const domContent = ref<string>("");
const isExtractingDom = ref(false);
const autoRefresh = ref(false);
let autoRefreshTimer: ReturnType<typeof setInterval> | null = null;
const toastMessage = ref<string | null>(null);

const showToast = (msg: string) => {
  toastMessage.value = msg;
  setTimeout(() => {
    toastMessage.value = null;
  }, 3500);
};

const browserStatus = computed(() => {
  return gateway.browserStatus.value;
});

const screenshotData = computed(() => {
  return gateway.browserScreenshot.value;
});

const actionLogs = computed<BrowserActionRecord[]>(() => {
  return gateway.browserActionLogs.value || [];
});

const refreshAll = async () => {
  loading.value = true;
  try {
    await Promise.all([
      gateway.fetchBrowserStatus(),
      gateway.fetchBrowserScreenshot(),
      gateway.fetchBrowserActionLogs(),
    ]);
    if (browserStatus.value?.currentUrl) {
      targetUrl.value = browserStatus.value.currentUrl;
    }
  } catch (err) {
    logger.error("[BrowserPreviewView]", "Failed to fetch browser state:", err);
  } finally {
    loading.value = false;
  }
};

const handleNavigate = async () => {
  if (!targetUrl.value.trim()) return;
  isNavigating.value = true;
  try {
    const res = await gateway.navigateBrowser(targetUrl.value.trim());
    if (res?.success) {
      showToast(`Navigated to ${res.title || res.url} (HTTP ${res.httpStatus})`);
    }
  } catch (err) {
    showToast(`Navigation failed: ${err}`);
  } finally {
    isNavigating.value = false;
  }
};

const handleTakeScreenshot = async () => {
  isCapturing.value = true;
  try {
    await gateway.fetchBrowserScreenshot();
    showToast("Captured viewport screenshot.");
  } catch (err) {
    showToast(`Screenshot failed: ${err}`);
  } finally {
    isCapturing.value = false;
  }
};

const handleExtractDom = async () => {
  isExtractingDom.value = true;
  try {
    const res = await gateway.extractBrowserDom(domExtractionMode.value);
    if (res?.content) {
      domContent.value = res.content;
      showToast(`Extracted ${res.length} chars of DOM (${domExtractionMode.value})`);
    }
  } catch (err) {
    showToast(`DOM Extraction failed: ${err}`);
  } finally {
    isExtractingDom.value = false;
  }
};

const handleControl = async (action: "pause" | "resume" | "stop" | "clear_logs") => {
  try {
    await gateway.controlBrowser(action);
    showToast(`Session action executed: ${action}`);
  } catch (err) {
    showToast(`Control action failed: ${err}`);
  }
};

const toggleAutoRefresh = () => {
  autoRefresh.value = !autoRefresh.value;
  if (autoRefresh.value) {
    autoRefreshTimer = setInterval(() => {
      gateway.fetchBrowserScreenshot();
      gateway.fetchBrowserStatus();
    }, 2500);
    showToast("Auto-refresh enabled (2.5s interval)");
  } else {
    if (autoRefreshTimer) {
      clearInterval(autoRefreshTimer);
      autoRefreshTimer = null;
    }
    showToast("Auto-refresh paused");
  }
};

const formatTimeAgo = (unix: number) => {
  if (!unix) return "Just now";
  const now = Math.floor(Date.now() / 1000);
  const diff = now - unix;
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  return `${Math.floor(diff / 3600)}h ago`;
};

onMounted(() => {
  refreshAll();
});

onUnmounted(() => {
  if (autoRefreshTimer) {
    clearInterval(autoRefreshTimer);
    autoRefreshTimer = null;
  }
});
</script>

<template>
  <div class="browser-view animate-fadeIn">
    <!-- Header -->
    <header class="view-header">
      <div class="header-titles">
        <h2>Browser Automation Preview</h2>
        <p class="subtitle">
          Real-time viewport preview of automated browser sessions with SSRF protection, DOM tree extraction, and execution timelines.
        </p>
      </div>
      <div class="header-actions">
        <button
          :class="['btn btn-sm', autoRefresh ? 'btn-emerald' : 'btn-secondary']"
          @click="toggleAutoRefresh"
        >
          {{ autoRefresh ? '🟢 Live Auto-Refresh (2.5s)' : '⏸️ Auto-Refresh Off' }}
        </button>
        <button
          class="btn btn-secondary btn-sm"
          :disabled="isCapturing"
          @click="handleTakeScreenshot"
        >
          📷 Capture Viewport
        </button>
        <button class="btn btn-secondary btn-sm" @click="refreshAll">
          🔄 Refresh
        </button>
      </div>
    </header>

    <!-- Toast Banner -->
    <Transition name="fade">
      <div v-if="toastMessage" class="toast-banner">
        <span>{{ toastMessage }}</span>
      </div>
    </Transition>

    <!-- Browser Address & Control Bar -->
    <div class="card nav-bar-card">
      <div class="nav-controls">
        <button class="icon-btn" title="Back" @click="handleNavigate">◀</button>
        <button class="icon-btn" title="Forward" @click="handleNavigate">▶</button>
        <button class="icon-btn" title="Reload" @click="handleNavigate">🔄</button>
      </div>

      <div class="url-input-box">
        <span class="security-badge" title="SSRF Guard Active">🔒 Sandboxed</span>
        <input
          v-model="targetUrl"
          type="text"
          class="url-input"
          placeholder="https://example.com"
          @keyup.enter="handleNavigate"
        />
        <span v-if="browserStatus?.httpStatus" class="http-status-pill">
          HTTP {{ browserStatus.httpStatus }}
        </span>
      </div>

      <button
        class="btn btn-primary"
        :disabled="isNavigating || !targetUrl.trim()"
        @click="handleNavigate"
      >
        {{ isNavigating ? 'Loading...' : 'Go' }}
      </button>

      <div class="session-controls">
        <button
          v-if="!browserStatus?.isPaused"
          class="btn btn-secondary btn-sm"
          @click="handleControl('pause')"
          title="Pause active automation"
        >
          ⏸️ Pause
        </button>
        <button
          v-else
          class="btn btn-emerald btn-sm"
          @click="handleControl('resume')"
          title="Resume automation session"
        >
          ▶ Resume
        </button>
        <button
          class="btn btn-danger btn-sm"
          @click="handleControl('stop')"
          title="Terminate browser process"
        >
          ⏹️ Stop
        </button>
      </div>
    </div>

    <!-- Main Workspace: Viewport & Inspection Tabs -->
    <div class="workspace-layout">
      <!-- Left Column: Live Viewport Preview -->
      <div class="viewport-column">
        <div class="card viewport-card">
          <div class="viewport-header">
            <div class="viewport-title">
              <span class="dot-live"></span>
              <strong>{{ browserStatus?.pageTitle || 'Viewport (1280x800)' }}</strong>
            </div>
            <div class="viewport-meta">
              <span>{{ browserStatus?.viewportWidth || 1280 }}x{{ browserStatus?.viewportHeight || 800 }}</span>
              <span v-if="browserStatus?.ssrfGuard" class="badge-guard">SSRF Protected</span>
            </div>
          </div>

          <div class="viewport-canvas">
            <div v-if="loading && !screenshotData" class="canvas-placeholder">
              <div class="spinner"></div>
              <p>Connecting to headless browser viewport...</p>
            </div>

            <div v-else-if="screenshotData" class="screenshot-container">
              <img :src="screenshotData" alt="Browser Viewport Screenshot" class="screenshot-img" />
            </div>

            <div v-else class="canvas-placeholder">
              <span class="icon-large">🌐</span>
              <p>Ready to navigate. Enter a URL above and click "Go".</p>
            </div>
          </div>
        </div>
      </div>

      <!-- Right Column: Inspector (Timeline & DOM Extraction) -->
      <div class="inspector-column">
        <div class="card inspector-card">
          <!-- Tab Headers -->
          <div class="inspector-tabs">
            <button
              :class="['tab-btn', { active: activeDomTab === 'preview' }]"
              @click="activeDomTab = 'preview'"
            >
              📊 State & Metrics
            </button>
            <button
              :class="['tab-btn', { active: activeDomTab === 'dom' }]"
              @click="activeDomTab = 'dom'"
            >
              🌳 DOM Extraction
            </button>
            <button
              :class="['tab-btn', { active: activeDomTab === 'timeline' }]"
              @click="activeDomTab = 'timeline'"
            >
              ⏱️ Action Timeline
            </button>
          </div>

          <!-- Tab 1: State & Metrics -->
          <div v-if="activeDomTab === 'preview'" class="tab-body">
            <div class="metric-grid">
              <div class="metric-box">
                <span class="label">Browser State</span>
                <strong class="val status-green">{{ browserStatus?.isRunning ? 'Running' : 'Stopped' }}</strong>
              </div>
              <div class="metric-box">
                <span class="label">Sandbox Mode</span>
                <strong class="val text-blue">Isolated Chromium</strong>
              </div>
              <div class="metric-box">
                <span class="label">Memory Limit</span>
                <strong class="val">512 MB Max</strong>
              </div>
              <div class="metric-box">
                <span class="label">Max Execution</span>
                <strong class="val">30s per action</strong>
              </div>
            </div>

            <div class="info-section">
              <h4>Security & Policy Guard</h4>
              <ul class="guard-list">
                <li>✓ Localhost/Private RFC-1918 egress blocked by default (SSRF Guard)</li>
                <li>✓ Cookie sandbox isolated per automation task run</li>
                <li>✓ JavaScript eval execution constrained to active context</li>
              </ul>
            </div>
          </div>

          <!-- Tab 2: DOM Extraction -->
          <div v-else-if="activeDomTab === 'dom'" class="tab-body">
            <div class="dom-control-row">
              <select v-model="domExtractionMode" class="dom-select">
                <option value="semantic">Semantic Clean Markdown</option>
                <option value="accessibility">Accessibility Tree</option>
                <option value="plain_text">Plain Text Content</option>
                <option value="html">Full HTML Source</option>
              </select>
              <button
                class="btn btn-secondary btn-sm"
                :disabled="isExtractingDom"
                @click="handleExtractDom"
              >
                {{ isExtractingDom ? 'Extracting...' : 'Extract Content' }}
              </button>
            </div>

            <div class="dom-preview-box">
              <pre v-if="domContent"><code>{{ domContent }}</code></pre>
              <div v-else class="empty-dom">
                <span>📄</span>
                <p>Click "Extract Content" to parse and inspect the semantic DOM tree of the current page.</p>
              </div>
            </div>
          </div>

          <!-- Tab 3: Action Timeline -->
          <div v-else class="tab-body">
            <div class="timeline-header">
              <span>Recent Steps ({{ actionLogs.length }})</span>
              <button class="btn-text" @click="handleControl('clear_logs')">Clear</button>
            </div>

            <div v-if="actionLogs.length === 0" class="empty-timeline">
              <span>💤</span>
              <p>No actions logged in the current session.</p>
            </div>

            <div v-else class="timeline-list">
              <div
                v-for="log in actionLogs"
                :key="log.id"
                class="timeline-item"
                :class="log.status"
              >
                <div class="timeline-top">
                  <span class="action-tag">{{ log.action }}</span>
                  <span class="time-tag">{{ formatTimeAgo(log.timestamp_unix) }}</span>
                </div>
                <div class="timeline-target">
                  <code>{{ log.target }}</code>
                </div>
                <p class="timeline-details">{{ log.details }}</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.browser-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 24px;
  overflow-y: auto;
  gap: 20px;
  background: var(--bg-secondary, #0e1017);
}

.view-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
}

.header-titles h2 {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary, #ffffff);
  margin: 0 0 6px 0;
}

.subtitle {
  font-size: 13px;
  color: var(--text-secondary, #94a3b8);
  margin: 0;
  max-width: 650px;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.toast-banner {
  padding: 10px 16px;
  border-radius: 8px;
  background: rgba(56, 189, 248, 0.12);
  border: 1px solid rgba(56, 189, 248, 0.3);
  color: #38bdf8;
  font-size: 13px;
  font-weight: 500;
}

.card {
  background: rgba(18, 20, 29, 0.7);
  border: 1px solid var(--border-default, #242738);
  border-radius: 12px;
  backdrop-filter: blur(10px);
}

.nav-bar-card {
  padding: 12px 16px;
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.nav-controls {
  display: flex;
  gap: 4px;
}

.icon-btn {
  background: transparent;
  border: none;
  color: var(--text-secondary, #94a3b8);
  font-size: 14px;
  padding: 6px 8px;
  border-radius: 4px;
  cursor: pointer;
}

.icon-btn:hover {
  background: rgba(255, 255, 255, 0.05);
  color: #ffffff;
}

.url-input-box {
  display: flex;
  align-items: center;
  flex: 1;
  background: #080a10;
  border: 1px solid var(--border-default, #242738);
  border-radius: 6px;
  padding: 0 10px;
  min-width: 280px;
}

.security-badge {
  font-size: 10px;
  font-weight: 600;
  color: #4ade80;
  background: rgba(34, 197, 94, 0.1);
  padding: 2px 6px;
  border-radius: 4px;
  white-space: nowrap;
}

.url-input {
  flex: 1;
  background: transparent;
  border: none;
  padding: 8px 10px;
  color: #ffffff;
  font-size: 13px;
}

.url-input:focus {
  outline: none;
}

.http-status-pill {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 4px;
  background: rgba(56, 189, 248, 0.15);
  color: #38bdf8;
}

.session-controls {
  display: flex;
  gap: 6px;
  border-left: 1px solid rgba(255, 255, 255, 0.05);
  padding-left: 12px;
}

.workspace-layout {
  display: grid;
  grid-template-columns: 1.4fr 1fr;
  gap: 20px;
  min-height: 480px;
}

.viewport-column, .inspector-column {
  display: flex;
  flex-direction: column;
}

.viewport-card, .inspector-card {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.viewport-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.viewport-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-primary, #ffffff);
}

.dot-live {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #4ade80;
  box-shadow: 0 0 8px #4ade80;
}

.viewport-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--text-muted, #64748b);
}

.badge-guard {
  background: rgba(56, 189, 248, 0.12);
  color: #38bdf8;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 10px;
}

.viewport-canvas {
  flex: 1;
  background: #06070b;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 400px;
  overflow: hidden;
  border-radius: 0 0 12px 12px;
}

.screenshot-container {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}

.screenshot-img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
}

.canvas-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-muted, #64748b);
  font-size: 13px;
  padding: 40px;
  text-align: center;
}

.canvas-placeholder .icon-large {
  font-size: 48px;
  opacity: 0.4;
}

.inspector-tabs {
  display: flex;
  padding: 0 16px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  gap: 14px;
}

.tab-btn {
  background: transparent;
  border: none;
  padding: 12px 4px;
  color: var(--text-secondary, #94a3b8);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  position: relative;
}

.tab-btn:hover {
  color: #ffffff;
}

.tab-btn.active {
  color: #818cf8;
  font-weight: 600;
}

.tab-btn.active::after {
  content: "";
  position: absolute;
  bottom: -1px;
  left: 0;
  right: 0;
  height: 2px;
  background: #818cf8;
}

.tab-body {
  padding: 16px;
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.metric-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

.metric-box {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: 8px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.metric-box .label {
  font-size: 10px;
  color: var(--text-muted, #64748b);
  text-transform: uppercase;
}

.metric-box .val {
  font-size: 13px;
  color: var(--text-primary, #ffffff);
}

.status-green { color: #4ade80 !important; }
.text-blue { color: #38bdf8 !important; }

.info-section h4 {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0 0 8px 0;
}

.guard-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
  color: var(--text-secondary, #94a3b8);
}

.dom-control-row {
  display: flex;
  gap: 10px;
}

.dom-select {
  flex: 1;
  background: #080a10;
  border: 1px solid var(--border-default, #242738);
  border-radius: 6px;
  padding: 6px 10px;
  color: #ffffff;
  font-size: 12px;
}

.dom-preview-box {
  flex: 1;
  background: #080a10;
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: 8px;
  padding: 12px;
  overflow-y: auto;
  max-height: 380px;
}

.dom-preview-box pre {
  margin: 0;
  color: #cbd5e1;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  white-space: pre-wrap;
  word-break: break-all;
}

.empty-dom {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 180px;
  gap: 8px;
  color: var(--text-muted, #64748b);
  text-align: center;
  font-size: 12px;
}

.empty-dom span {
  font-size: 28px;
}

.timeline-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 12px;
  color: var(--text-muted, #64748b);
}

.btn-text {
  background: transparent;
  border: none;
  color: #818cf8;
  font-size: 11px;
  cursor: pointer;
}

.timeline-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.timeline-item {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: 6px;
  padding: 8px 10px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.timeline-item.success {
  border-left: 2px solid #22c55e;
}

.timeline-item.failed {
  border-left: 2px solid #ef4444;
}

.timeline-top {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.action-tag {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  color: #38bdf8;
}

.time-tag {
  font-size: 10px;
  color: var(--text-muted, #64748b);
}

.timeline-target code {
  font-size: 11px;
  color: #e2e8f0;
  font-family: monospace;
}

.timeline-details {
  font-size: 11px;
  color: var(--text-secondary, #94a3b8);
  margin: 0;
}

.btn-emerald {
  background: rgba(16, 185, 129, 0.15);
  color: #34d399;
  border: 1px solid rgba(16, 185, 129, 0.3);
}

.spinner {
  width: 24px;
  height: 24px;
  border: 2px solid rgba(255, 255, 255, 0.1);
  border-top-color: #818cf8;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

@media (max-width: 900px) {
  .workspace-layout {
    grid-template-columns: 1fr;
  }
}
</style>
