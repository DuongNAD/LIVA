<script setup lang="ts">
/**
 * StatusBar.vue — Footer Status Bar
 * ====================================
 * Shows: WebSocket status, AI model, engine mode, latency.
 */
import { computed } from "vue";
import { useGateway } from "../../composables/useGateway";

const gateway = useGateway();

const wsStatus = computed(() => gateway.isConnected.value ? 'connected' : 'disconnected');
const aiModel = computed(() => {
  if (gateway.systemStatus.value && gateway.systemStatus.value.model) {
    return gateway.systemStatus.value.model;
  }
  if (gateway.configData.value && gateway.configData.value.ai) {
    return gateway.configData.value.ai.routerModel;
  }
  return 'Loading...';
});
const engineMode = computed(() => gateway.configData.value?.avatar?.engineMode || 'Auto');
const latency = computed(() => gateway.isConnected.value ? (gateway.systemStatus.value?.latencyMs ?? 0) : 0);
</script>

<template>
  <footer class="statusbar">
    <div class="statusbar-left">
      <!-- Connection Status -->
      <div class="status-item">
        <span :class="['status-dot', wsStatus]"></span>
        <span class="status-text">
          {{ wsStatus === 'connected' ? 'Connected' : wsStatus === 'connecting' ? 'Connecting...' : 'Disconnected' }}
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
        <span class="status-text">Engine: {{ engineMode }}</span>
      </div>

      <span class="status-divider">│</span>

      <!-- Latency -->
      <div class="status-item">
        <span class="status-text" :style="{ color: latency > 200 ? 'var(--color-danger)' : 'var(--color-success)' }">
          {{ latency }}ms
        </span>
      </div>
    </div>
  </footer>
</template>

<style scoped>
.statusbar {
  height: var(--statusbar-height, 28px);
  background: var(--bg-primary);
  border-top: 1px solid var(--border-default);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  flex-shrink: 0;
  font-size: 11px;
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
</style>
