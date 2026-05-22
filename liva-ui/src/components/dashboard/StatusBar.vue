<script setup lang="ts">
/**
 * StatusBar.vue — Footer Status Bar
 * ====================================
 * Shows: WebSocket status, AI model, engine mode, latency.
 */
import { computed } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";

const gateway = useGateway();
const { t } = useI18n();

const wsStatus = computed(() => gateway.isConnected.value ? 'connected' : 'disconnected');
const aiModel = computed<string>(() => {
  if (gateway.systemStatus.value && (gateway.systemStatus.value as any).model) {
    return String((gateway.systemStatus.value as any).model);
  }
  if (gateway.configData.value && (gateway.configData.value as any).ai) {
    return String((gateway.configData.value as any).ai.routerModel || 'Loading...');
  }
  return 'Loading...';
});
const engineMode = computed(() => {
  const raw = (gateway.configData.value as any)?.avatar;
  const mode = String(raw?.engineMode ?? 'auto');
  if (mode.toLowerCase() === 'auto') return t('engine_auto');
  return `Engine: ${mode.toUpperCase() === '2D' ? '2D' : mode.toUpperCase() === '3D' ? '3D' : mode}`;
});
const latency = computed<number>(() => gateway.isConnected.value ? Number((gateway.systemStatus.value as any)?.latencyMs ?? 0) : 0);

const latencyColor = computed(() => {
  const ms = latency.value;
  if (ms <= 80) return 'var(--color-success)';
  if (ms <= 150) return '#84cc16'; // lime
  if (ms <= 300) return 'var(--color-warning)';
  return 'var(--color-danger)';
});
</script>

<template>
  <footer class="statusbar">
    <div class="statusbar-left">
      <!-- Connection Status -->
      <div class="status-item">
        <span :class="['status-dot', wsStatus, wsStatus === 'connected' ? 'alive-pulse' : '']"></span>
        <span class="status-text">
          {{ wsStatus === 'connected' ? t('connected') : t('disconnected') }}
        </span>
      </div>

      <!-- Divider -->
      <span class="status-divider">│</span>

      <!-- AI Model -->
      <div class="status-item">
        <span class="status-icon">🧠</span>
        <span class="status-text">{{ aiModel }}</span>
      </div>
    </div>

    <div class="statusbar-right">
      <!-- Engine Mode -->
      <div class="status-item">
        <span class="status-icon">🎮</span>
        <span class="status-text">{{ engineMode }}</span>
      </div>

      <span class="status-divider">│</span>

      <!-- Latency -->
      <div class="status-item">
        <span class="status-text latency-badge" :style="{ color: latencyColor, borderColor: latencyColor + '33' }">
          {{ latency }}ms
        </span>
      </div>
    </div>
  </footer>
</template>

<style scoped>
.statusbar {
  height: 32px;
  background: var(--statusbar-bg);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border: 1px solid var(--statusbar-border);
  border-radius: 999px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  flex-shrink: 0;
  font-size: 11px;
  margin: 0 16px 16px 16px;
  position: relative;
  z-index: 50;
  box-shadow: var(--shadow-sm);
}

.statusbar-left,
.statusbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.status-item {
  display: flex;
  align-items: center;
  gap: 4px;
}

.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-dot.connected {
  background: var(--color-success);
  box-shadow: 0 0 6px var(--color-success);
}

.status-dot.connecting {
  background: var(--color-warning);
  animation: pulse 1.5s infinite;
}

.status-dot.disconnected {
  background: var(--color-danger);
}

.status-text {
  color: var(--text-secondary);
}

.status-icon {
  font-size: 12px;
}

.status-divider {
  color: var(--text-muted);
  font-size: 10px;
}

/* Alive Pulse Animation for Connected Dot */
.alive-pulse {
  animation: alivePulse 2.5s ease-in-out infinite;
}
@keyframes alivePulse {
  0%, 100% { box-shadow: 0 0 4px var(--color-success); }
  50% { box-shadow: 0 0 10px var(--color-success), 0 0 2px var(--color-success); }
}

/* Latency Badge */
.latency-badge {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  padding: 1px 6px;
  border-radius: 4px;
  border: 1px solid;
  font-size: 10px;
  letter-spacing: 0.3px;
}
</style>
