<script setup lang="ts">
/**
 * SystemView.vue — System Monitor
 * ==================================
 * Real-time system status and performance metrics.
 */
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { profileHardware, type HardwareProfile } from "../../utils/HardwareDetector";
import { useGateway } from "../../composables/useGateway";

const gateway = useGateway();

// Hardware info
const hardware = ref<HardwareProfile | null>(null);

// Service status
interface ServiceStatus {
  name: string;
  icon: string;
  status: 'online' | 'offline' | 'loading';
  port: string;
  detail: string;
}

const services = computed<ServiceStatus[]>(() => {
  const isConn = gateway.isConnected.value;
  return [
    { 
      name: 'AI Engine', 
      icon: '🧠', 
      status: isConn ? 'online' : 'offline', 
      port: '8000', 
      detail: gateway.systemStatus.value?.model || 'llama.cpp HTTP' 
    },
    { 
      name: 'Voice Engine', 
      icon: '🎤', 
      status: isConn ? 'online' : 'offline', 
      port: '8002', 
      detail: 'Edge-TTS' 
    },
    { 
      name: 'Gateway', 
      icon: '🔗', 
      status: isConn ? 'online' : 'offline', 
      port: '8082', 
      detail: 'WebSocket Server' 
    },
    { 
      name: 'Widget UI', 
      icon: '🤖', 
      status: 'online', 
      port: '5173', 
      detail: 'Vite Dev Server' 
    },
  ];
});

// Performance
const uptime = computed(() => {
  if (!gateway.isConnected.value || !gateway.systemStatus.value?.uptime) return '--';
  const elapsed = Math.floor(gateway.systemStatus.value.uptime);
  const mins = Math.floor(elapsed / 60);
  const secs = elapsed % 60;
  return `${mins}m ${secs}s`;
});

const memoryUsage = computed(() => {
  if (!gateway.isConnected.value || !gateway.systemStatus.value?.memoryUsage) return '--';
  const mem = gateway.systemStatus.value.memoryUsage;
  return `${Math.round(mem / 1024 / 1024)} MB`;
});

const platform = ref(typeof globalThis !== 'undefined' ? globalThis.navigator.platform : '--');

onMounted(() => {
  // Profile hardware
  hardware.value = profileHardware();
});

</script>

<template>
  <div class="system-view animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">📊 System Monitor</h1>
      <p class="page-desc">Trạng thái hệ thống LIVA real-time</p>
    </div>

    <!-- Service Status Grid -->
    <div class="section-subtitle">Services</div>
    <div class="grid-2" style="margin-bottom: var(--space-lg);">
      <div v-for="svc in services" :key="svc.name" class="card service-card">
        <div class="service-header">
          <span class="service-icon">{{ svc.icon }}</span>
          <div class="service-info">
            <h3 class="service-name">{{ svc.name }}</h3>
            <p class="service-detail">{{ svc.detail }} (port {{ svc.port }})</p>
          </div>
          <span :class="['status-dot', svc.status]"></span>
        </div>
        <span :class="['badge', svc.status === 'online' ? 'badge-success' : svc.status === 'loading' ? 'badge-warning' : 'badge-danger']">
          {{ svc.status === 'online' ? 'Online' : svc.status === 'loading' ? 'Checking...' : 'Offline' }}
        </span>
      </div>
    </div>

    <!-- Hardware Info -->
    <div class="section-subtitle">Hardware</div>
    <div class="card hardware-card" v-if="hardware">
      <div class="hw-grid">
        <div class="hw-item">
          <span class="hw-label">GPU</span>
          <span class="hw-value">{{ hardware.gpu }}</span>
          <span :class="['badge', hardware.isWeakGPU ? 'badge-warning' : 'badge-success']">
            {{ hardware.isWeakGPU ? 'Integrated' : 'Discrete' }}
          </span>
        </div>
        <div class="hw-item">
          <span class="hw-label">RAM</span>
          <span class="hw-value">{{ hardware.ram }} GB</span>
        </div>
        <div class="hw-item">
          <span class="hw-label">CPU Cores</span>
          <span class="hw-value">{{ hardware.cores }}</span>
        </div>
        <div class="hw-item">
          <span class="hw-label">Recommended Engine</span>
          <span class="hw-value">{{ hardware.recommendedEngine }}</span>
          <span :class="['badge', hardware.recommendedEngine === '3D' ? 'badge-success' : 'badge-info']">
            {{ hardware.recommendedEngine === '3D' ? 'VRM 3D' : 'Live2D' }}
          </span>
        </div>
      </div>
    </div>

    <!-- Performance -->
    <div class="section-subtitle" style="margin-top: var(--space-lg);">Performance</div>
    <div class="grid-3">
      <div class="card metric-card">
        <span class="metric-label">Uptime</span>
        <span class="metric-value">{{ uptime }}</span>
      </div>
      <div class="card metric-card">
        <span class="metric-label">JS Heap</span>
        <span class="metric-value">{{ memoryUsage }}</span>
      </div>
      <div class="card metric-card">
        <span class="metric-label">Platform</span>
        <span class="metric-value">{{ platform }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.system-view { padding: var(--space-lg); overflow-y: auto; height: 100%; }
.page-header { margin-bottom: var(--space-lg); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }

.service-card { display: flex; flex-direction: column; gap: var(--space-sm); }
.service-header { display: flex; align-items: center; gap: var(--space-md); }
.service-icon { font-size: 24px; }
.service-info { flex: 1; }
.service-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
.service-detail { font-size: 11px; color: var(--text-muted); }

.status-dot {
  width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0;
}
.status-dot.online { background: var(--color-success); box-shadow: 0 0 8px var(--color-success); }
.status-dot.loading { background: var(--color-warning); animation: pulse 1.5s infinite; }
.status-dot.offline { background: var(--color-danger); }

.hardware-card { margin-bottom: var(--space-md); }
.hw-grid { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-md); }
.hw-item { display: flex; flex-direction: column; gap: 4px; }
.hw-label { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px; font-weight: 600; }
.hw-value { font-size: 13px; color: var(--text-primary); font-weight: 500; word-break: break-all; }

.metric-card { display: flex; flex-direction: column; align-items: center; gap: 4px; padding: var(--space-lg); }
.metric-label { font-size: 11px; color: var(--text-muted); text-transform: uppercase; font-weight: 600; }
.metric-value { font-size: 20px; font-weight: 700; background: var(--accent-gradient); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
</style>
