<script setup lang="ts">
/**
 * SkillLogsPanel.vue — Skill Execution History & Logs Modal
 * ==========================================================
 * Displays recent execution traces, duration, input parameters,
 * caller principals, and tool output payloads for a skill.
 */
import { ref, onMounted } from "vue";
import { useGateway } from "../../../composables/useGateway";
import { logger } from "../../../utils/logger";
import type { SkillLogEntry } from "liva-common";

const props = defineProps<{
  skillId: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const gateway = useGateway();
const loading = ref(true);
const logs = ref<SkillLogEntry[]>([]);

const loadLogs = async () => {
  loading.value = true;
  try {
    const res = await gateway.fetchSkillLogs(props.skillId, 25);
    if (res?.logs) {
      logs.value = res.logs;
    }
  } catch (err) {
    logger.error("[SkillLogsPanel]", "Failed to load skill logs:", err);
  } finally {
    loading.value = false;
  }
};

const formatTime = (unix: number) => {
  if (!unix) return "Just now";
  const date = new Date(unix * 1000);
  return date.toLocaleTimeString() + " " + date.toLocaleDateString();
};

onMounted(() => {
  loadLogs();
});
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <div class="modal-card animate-scaleUp">
      <!-- Modal Header -->
      <div class="modal-header">
        <div class="header-title">
          <span class="icon">📜</span>
          <h3>Execution Logs: {{ props.skillId }}</h3>
        </div>
        <button class="btn-close" @click="emit('close')">✕</button>
      </div>

      <!-- Modal Body -->
      <div class="modal-body">
        <div v-if="loading" class="modal-loading">
          <div class="spinner"></div>
          <p>Fetching execution logs...</p>
        </div>

        <div v-else-if="logs.length === 0" class="empty-logs">
          <span>💤</span>
          <p>No execution logs found for this skill yet.</p>
        </div>

        <div v-else class="logs-list">
          <div
            v-for="log in logs"
            :key="log.id"
            class="log-item"
            :class="{ success: log.status === 'SUCCESS', error: log.status === 'ERROR' }"
          >
            <div class="log-top">
              <div class="log-status-row">
                <span class="status-badge" :class="log.status.toLowerCase()">
                  {{ log.status }}
                </span>
                <span class="caller-name">Caller: {{ log.caller }}</span>
              </div>
              <div class="log-meta">
                <span class="duration-badge">⏱️ {{ log.durationMs }}ms</span>
                <span class="log-time">{{ formatTime(log.timestampUnix) }}</span>
              </div>
            </div>

            <div class="log-payloads">
              <div v-if="log.input" class="payload-box">
                <span class="payload-label">Input</span>
                <pre><code>{{ JSON.stringify(log.input, null, 2) }}</code></pre>
              </div>
              <div v-if="log.output" class="payload-box">
                <span class="payload-label">Output</span>
                <pre><code>{{ JSON.stringify(log.output, null, 2) }}</code></pre>
              </div>
              <div v-if="log.error" class="payload-box error-payload">
                <span class="payload-label">Error</span>
                <pre><code>{{ log.error }}</code></pre>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="modal-footer">
        <button class="btn btn-secondary" @click="loadLogs">🔄 Refresh</button>
        <button class="btn btn-primary" @click="emit('close')">Done</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(5px);
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
}

.modal-card {
  width: 650px;
  max-width: 90vw;
  max-height: 85vh;
  background: #0f121d;
  border: 1px solid var(--border-default, #262a3d);
  border-radius: 12px;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.5);
  display: flex;
  flex-direction: column;
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.header-title {
  display: flex;
  align-items: center;
  gap: 8px;
}

.header-title .icon {
  font-size: 18px;
}

.header-title h3 {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0;
}

.btn-close {
  background: transparent;
  border: none;
  color: var(--text-secondary, #94a3b8);
  font-size: 16px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
}

.btn-close:hover {
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-primary, #ffffff);
}

.modal-body {
  padding: 20px;
  overflow-y: auto;
  flex: 1;
}

.modal-loading, .empty-logs {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 220px;
  gap: 12px;
  color: var(--text-secondary, #94a3b8);
}

.empty-logs span {
  font-size: 32px;
}

.logs-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.log-item {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 8px;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.log-item.success {
  border-left: 3px solid #22c55e;
}

.log-item.error {
  border-left: 3px solid #ef4444;
}

.log-top {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.log-status-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.status-badge {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 4px;
  text-transform: uppercase;
}

.status-badge.success {
  background: rgba(34, 197, 94, 0.15);
  color: #4ade80;
}

.status-badge.error {
  background: rgba(239, 68, 68, 0.15);
  color: #f87171;
}

.caller-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary, #ffffff);
}

.log-meta {
  display: flex;
  align-items: center;
  gap: 10px;
}

.duration-badge {
  font-size: 11px;
  color: #38bdf8;
  font-family: monospace;
}

.log-time {
  font-size: 11px;
  color: var(--text-muted, #64748b);
}

.log-payloads {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

.payload-box {
  background: #080a10;
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: 6px;
  padding: 8px 10px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.payload-label {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  color: var(--text-muted, #64748b);
}

.payload-box pre {
  margin: 0;
  color: #cbd5e1;
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 120px;
  overflow-y: auto;
}

.error-payload pre {
  color: #f87171;
}

.modal-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-top: 1px solid rgba(255, 255, 255, 0.05);
}

@keyframes scaleUp {
  from { transform: scale(0.95); opacity: 0; }
  to { transform: scale(1); opacity: 1; }
}

.animate-scaleUp {
  animation: scaleUp 0.2s cubic-bezier(0.16, 1, 0.3, 1);
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
</style>
