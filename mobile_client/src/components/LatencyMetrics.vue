<template>
  <div class="latency-metrics-widget">
    <div class="metrics-header">
      <span class="title">System Diagnostics</span>
      <span class="status-indicator" :class="statusClass">{{ statusText }}</span>
    </div>
    
    <div class="metrics-grid">
      <div class="metric-item">
        <span class="metric-label">WS Ping</span>
        <span class="metric-value">{{ typeof pingLatency === 'number' ? `${pingLatency}ms` : '--' }}</span>
      </div>
      <div class="metric-item">
        <span class="metric-label">VAD State</span>
        <span class="metric-value highlight" :class="{ speaking: vadActive }">
          {{ vadActive ? 'SPEAKING' : 'SILENT' }}
        </span>
      </div>
      <div class="metric-item">
        <span class="metric-label">LLM Speed</span>
        <span class="metric-value">{{ llmSpeed }} t/s</span>
      </div>
      <div class="metric-item">
        <span class="metric-label">TTS Delay</span>
        <span class="metric-value">{{ ttsDelay }}ms</span>
      </div>
    </div>
  </div>
</template>

<script lang="ts">
import { defineComponent, computed } from 'vue'

export default defineComponent({
  name: 'LatencyMetrics',
  props: {
    pingLatency: {
      type: Number,
      default: null
    },
    vadActive: {
      type: Boolean,
      default: false
    },
    llmSpeed: {
      type: Number,
      default: 45
    },
    ttsDelay: {
      type: Number,
      default: 120
    },
    isConnected: {
      type: Boolean,
      default: false
    }
  },
  setup(props) {
    const statusText = computed(() => {
      if (!props.isConnected) return 'DISCONNECTED'
      if (props.pingLatency && props.pingLatency > 150) return 'HIGH LATENCY'
      return 'HEALTHY'
    })

    const statusClass = computed(() => {
      if (!props.isConnected) return 'disconnected'
      if (props.pingLatency && props.pingLatency > 150) return 'warning'
      return 'healthy'
    })

    return {
      statusText,
      statusClass
    }
  }
})
</script>

<style scoped>
.latency-metrics-widget {
  background: rgba(30, 41, 59, 0.7);
  backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 12px;
  padding: 12px 16px;
  font-family: monospace;
}

.metrics-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
  font-size: 0.75rem;
  font-weight: bold;
  color: #94a3b8;
}

.status-indicator {
  font-size: 0.65rem;
  padding: 2px 6px;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.2);
}

.status-indicator.healthy {
  color: #10b981;
  border: 1px solid rgba(16, 185, 129, 0.3);
}

.status-indicator.warning {
  color: #f59e0b;
  border: 1px solid rgba(245, 158, 11, 0.3);
}

.status-indicator.disconnected {
  color: #ef4444;
  border: 1px solid rgba(239, 68, 68, 0.3);
}

.metrics-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 8px;
}

.metric-item {
  background: rgba(15, 23, 42, 0.4);
  padding: 8px;
  border-radius: 6px;
  display: flex;
  flex-direction: column;
}

.metric-label {
  font-size: 0.6rem;
  color: #64748b;
  text-transform: uppercase;
  margin-bottom: 2px;
}

.metric-value {
  font-size: 0.85rem;
  color: #f1f5f9;
  font-weight: 600;
}

.metric-value.highlight {
  color: #64748b;
}

.metric-value.speaking {
  color: #10b981;
  text-shadow: 0 0 8px rgba(16, 185, 129, 0.4);
}
</style>
