<script setup lang="ts">
/**
 * SystemView.vue — LIVA System Health Monitor (v2)
 * =================================================
 * 8 deep health probes with live latency, memory metrics,
 * remote control status, and event telemetry.
 */
import { ref, computed, onMounted, onUnmounted, onActivated, onDeactivated } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";
import { profileHardware, type HardwareProfile } from "../../utils/HardwareDetector";

const gateway = useGateway();
const { t } = useI18n();
const hardware = ref<HardwareProfile | null>(null);
const hc = computed(() => (gateway.systemStatus.value as any)?.healthChecks || null);
const osStats = computed(() => (gateway.systemStatus.value as any)?.osStats || {});
const telemetry = computed(() => (gateway.systemStatus.value as any)?.telemetry || []);

interface SvcCard {
  id: string; name: string; icon: string;
  status: 'online' | 'offline' | 'degraded' | 'loading' | 'standby' | 'not_configured';
  latencyMs: number; detail: string; port: string; critical: boolean;
}

const services = computed<SvcCard[]>(() => {
  const h = hc.value;
  const conn = gateway.isConnected.value;
  if (!conn) return defaultCards('offline');
  if (!h) return defaultCards('loading');
  return [
    card('gateway', '🔗', 'Gateway', 'online', 0, `${h.gateway?.wsClients ?? 0} clients · ${h.gateway?.skillsLoaded ?? 0} skills`, '8082', true),
    card('ai', '🧠', 'AI Engine', h.aiEngine?.status, h.aiEngine?.latencyMs, h.aiEngine?.detail, (gateway.systemStatus.value as any)?.engineMode === 'native_grpc' ? '8100' : '8000', true),
    card('orchestrator', '⚡', 'Orchestrator', h.orchestrator?.status, -1, h.orchestrator?.detail, '--', true),
    card('voice', '🎤', 'Voice Engine', h.voiceEngine?.status, h.voiceEngine?.latencyMs, h.voiceEngine?.detail, '8002', false),
    card('memory', '💾', 'Memory DB', h.memory?.status, -1, h.memory?.detail, '--', true),
    card('vram', '🎮', 'VRAM Guard', h.vramGuard?.status || (h.vramGuard?.isYielded ? 'degraded' : 'online'), -1, h.vramGuard?.detail, '--', false),
    card('whisper', '🗣️', 'Whisper STT', h.whisper?.status, -1, h.whisper?.detail, '--', false),
    card('telegram', '📡', 'Remote Control',
      h.remoteControl?.enabled ? (h.remoteControl.telegram?.status === 'online' ? 'online' : 'standby') : 'not_configured',
      -1,
      h.remoteControl?.enabled
        ? `TG: ${h.remoteControl.telegram?.status} · Zalo: ${h.remoteControl.zalo?.status}`
        : t('sys_not_enabled'),
      '--', false),
  ];
});

function card(id: string, icon: string, name: string, status: string | undefined, latencyMs: number | undefined, detail: string | undefined, port: string, critical: boolean): SvcCard {
  return { id, icon, name, status: (status || 'offline') as SvcCard['status'], latencyMs: latencyMs ?? -1, detail: detail || '--', port, critical };
}

function defaultCards(s: SvcCard['status']): SvcCard[] {
  return [
    card('gateway','🔗','Gateway',s,-1,s === 'loading' ? t('sys_checking') : 'Disconnected','8082',true),
    card('ai','🧠','AI Engine',s,-1,'--','8100',true),
    card('orchestrator','⚡','Orchestrator',s,-1,'--','--',true),
    card('voice','🎤','Voice Engine',s,-1,'--','8002',false),
    card('memory','💾','Memory DB',s,-1,'--','--',true),
    card('vram','🎮','VRAM Guard',s,-1,'--','--',false),
    card('whisper','🗣️','Whisper STT',s,-1,'--','--',false),
    card('telegram','📡','Remote Control',s,-1,'--','--',false),
  ];
}

// Overall health
const healthScore = computed(() => {
  const crit = services.value.filter(s => s.critical);
  const ok = crit.filter(s => s.status === 'online').length;
  const pct = Math.round((ok / Math.max(crit.length, 1)) * 100);
  return {
    score: pct,
    label: pct === 100 ? 'Healthy' : pct >= 50 ? 'Degraded' : 'Critical',
    color: pct === 100 ? 'var(--color-success)' : pct >= 50 ? 'var(--color-warning)' : 'var(--color-danger)',
  };
});

// Metrics
const uptime = computed(() => {
  const u = (gateway.systemStatus.value as any)?.uptime;
  if (!u) return '--';
  const h = Math.floor(u / 3600), m = Math.floor((u % 3600) / 60), s = Math.floor(u % 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m ${s}s`;
});
const heapMB = computed(() => {
  const v = (gateway.systemStatus.value as any)?.memoryUsage;
  return v ? `${Math.round(v / 1048576)} MB` : '--';
});
const rssMB = computed(() => {
  const v = (gateway.systemStatus.value as any)?.rssMemory;
  return v ? `${Math.round(v / 1048576)} MB` : '--';
});
const engineMode = computed(() => (gateway.systemStatus.value as any)?.engineMode === 'native_grpc' ? 'Native gRPC' : 'HTTP');
const aiModel = computed<string>(() => String((gateway.systemStatus.value as any)?.model || '--'));

// System Management Operations
const isOptimizing = ref(false);
const isSyncing = ref(false);
const isReloading = ref(false);
const isWiping = ref(false);

const optimizeMemory = () => {
  isOptimizing.value = true;
  gateway.sendMsg('force_gc');
  setTimeout(() => {
    isOptimizing.value = false;
  }, 1000);
};

const syncGitNexus = () => {
  isSyncing.value = true;
  gateway.sendMsg('trigger_gitnexus_index');
  setTimeout(() => {
    isSyncing.value = false;
  }, 1500);
};

const reloadSkills = () => {
  isReloading.value = true;
  gateway.sendMsg('reload_skills');
  setTimeout(() => {
    isReloading.value = false;
  }, 1000);
};

const confirmWipeMemory = () => {
  const confirmText = gateway.userProfile.value?.language === 'en-US'
    ? "⚠️ CRITICAL WARNING!\nThis will wipe all conversation history, SQLite DB facts, long-term memory, and personalized AI context. Are you absolutely sure?"
    : "⚠️ CẢNH BÁO NGUY HIỂM!\nHành động này sẽ xóa sạch toàn bộ lịch sử trò chuyện, dữ liệu SQLite DB, ký ức dài hạn và ngữ cảnh AI cá nhân hóa. Bạn có chắc chắn?";
  if (confirm(confirmText)) {
    isWiping.value = true;
    gateway.sendMsg('reset_memory');
    setTimeout(() => {
      isWiping.value = false;
    }, 1500);
  }
};

// Polling
let timer: ReturnType<typeof setInterval> | null = null;
const startPoll = () => { if (!timer) { gateway.sendMsg('get_system_status'); timer = setInterval(() => gateway.sendMsg('get_system_status'), 3000); } };
const stopPoll = () => { if (timer) { clearInterval(timer); timer = null; } };
onMounted(() => { hardware.value = profileHardware(); });
onActivated(startPoll);
onDeactivated(stopPoll);
onUnmounted(stopPoll);

function badgeCls(s: string) { return s === 'online' ? 'badge-success' : s === 'degraded' ? 'badge-warning' : s === 'loading' ? 'badge-info' : s === 'standby' ? 'badge-info' : s === 'not_configured' ? 'badge-warning' : 'badge-danger'; }
function badgeTxt(s: string) { return s === 'online' ? 'Online' : s === 'degraded' ? 'Degraded' : s === 'loading' ? 'Checking' : s === 'standby' ? 'Standby' : s === 'not_configured' ? 'N/A' : 'Offline'; }
</script>

<template>
  <div class="system-view animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">📊 {{ t('sys_title') }}</h1>
      <p class="page-desc">{{ t('sys_desc') }}</p>
    </div>

    <!-- Health Banner -->
    <div class="health-banner" :style="{ '--hc': healthScore.color }">
      <div class="h-ring">
        <svg viewBox="0 0 36 36"><circle cx="18" cy="18" r="15.5" fill="none" stroke="var(--border-default)" stroke-width="3"/>
        <circle cx="18" cy="18" r="15.5" fill="none" :stroke="healthScore.color" stroke-width="3" stroke-linecap="round" :stroke-dasharray="`${healthScore.score * 0.975} 97.5`" transform="rotate(-90 18 18)" style="transition:stroke-dasharray .8s"/></svg>
        <span class="h-score">{{ healthScore.score }}</span>
      </div>
      <div class="h-info">
        <span class="h-label" :style="{ color: healthScore.color }">{{ healthScore.label === 'Healthy' ? t('sys_healthy') : healthScore.label }}</span>
        <span class="h-sub">{{ services.filter(s => s.status === 'online').length }}/{{ services.length }} {{ t('sys_services') }}</span>
      </div>
      <div class="h-meta">
        <div class="hm"><span class="hm-l">{{ t('sys_uptime') }}</span><span class="hm-v">{{ uptime }}</span></div>
        <div class="hm"><span class="hm-l">{{ t('sys_engine') }}</span><span class="hm-v">{{ engineMode }}</span></div>
        <div class="hm"><span class="hm-l">{{ t('sys_heap') }}</span><span class="hm-v">{{ heapMB }}</span></div>
        <div class="hm"><span class="hm-l">{{ t('sys_rss') }}</span><span class="hm-v">{{ rssMB }}</span></div>
        <div class="hm"><span class="hm-l">{{ t('sys_model') }}</span><span class="hm-v model-t" :title="aiModel">{{ aiModel }}</span></div>
      </div>
    </div>

    <!-- System Operations Control -->
    <div class="section-subtitle" style="margin-top:var(--space-lg)">{{ t('sys_management') }}</div>
    <div class="card control-card">
      <div class="control-grid">
        <button class="btn-control" @click="optimizeMemory" :disabled="isOptimizing">
          <span class="btn-icon">🧹</span>
          <div class="btn-info">
            <span class="btn-title">{{ t('sys_btn_gc') }}</span>
            <span class="btn-desc">{{ t('sys_btn_gc_desc') }}</span>
          </div>
          <span v-if="isOptimizing" class="control-spinner"></span>
        </button>
        <button class="btn-control" @click="syncGitNexus" :disabled="isSyncing">
          <span class="btn-icon">⚡</span>
          <div class="btn-info">
            <span class="btn-title">{{ t('sys_btn_git') }}</span>
            <span class="btn-desc">{{ t('sys_btn_git_desc') }}</span>
          </div>
          <span v-if="isSyncing" class="control-spinner"></span>
        </button>
        <button class="btn-control" @click="reloadSkills" :disabled="isReloading">
          <span class="btn-icon">🔄</span>
          <div class="btn-info">
            <span class="btn-title">{{ t('sys_btn_reload') }}</span>
            <span class="btn-desc">{{ t('sys_btn_reload_desc') }}</span>
          </div>
          <span v-if="isReloading" class="control-spinner"></span>
        </button>
        <button class="btn-control btn-danger-action" @click="confirmWipeMemory" :disabled="isWiping">
          <span class="btn-icon">💀</span>
          <div class="btn-info">
            <span class="btn-title">{{ t('sys_btn_wipe') }}</span>
            <span class="btn-desc" style="opacity: 0.8">{{ t('sys_btn_wipe_desc') }}</span>
          </div>
          <span v-if="isWiping" class="control-spinner"></span>
        </button>
      </div>
    </div>

    <!-- Service Cards -->
    <div class="section-subtitle" style="margin-top:var(--space-lg)">{{ t('sys_service_health', { count: services.filter(s=>s.status==='online').length }) }}</div>
    <div class="svc-grid">
      <div v-for="svc in services" :key="svc.id" :class="['svc-card', svc.status]">
        <div :class="['svc-strip', svc.status]"></div>
        <div class="svc-body">
          <div class="svc-top">
            <span class="svc-icon">{{ svc.icon }}</span>
            <div class="svc-info"><h3 class="svc-name">{{ svc.name }}</h3><p class="svc-detail">{{ svc.detail }}</p></div>
            <span :class="['dot', svc.status]"></span>
          </div>
          <div class="svc-bottom">
            <span :class="['badge', badgeCls(svc.status)]">{{ badgeTxt(svc.status) }}</span>
            <span class="svc-port" v-if="svc.port !== '--'">:{{ svc.port }}</span>
            <span class="svc-lat" v-if="svc.latencyMs >= 0">{{ svc.latencyMs }}ms</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Hardware -->
    <div class="section-subtitle" style="margin-top:var(--space-lg)">{{ t('sys_hardware') }}</div>
    <div class="card hw-card" v-if="hardware">
      <div class="hw-grid">
        <div class="hw-box">
          <div class="hw-hdr"><span>🖥️</span><span class="hw-title">{{ t('sys_system').toUpperCase() }}</span></div>
          <div class="hw-row"><span class="hw-l">OS</span><span class="hw-v">{{ hardware.os }}</span></div>
          <div class="hw-row"><span class="hw-l">{{ t('sys_network') }}</span><span class="hw-v">{{ osStats.networkStatus || '...' }}</span></div>
          <div class="hw-row"><span class="hw-l">{{ t('sys_disk') }}</span><span class="hw-v hw-sm" :title="osStats.diskInfo">{{ osStats.diskInfo || '...' }}</span></div>
        </div>
        <div class="hw-box">
          <div class="hw-hdr"><span>⚡</span><span class="hw-title">{{ t('sys_cpu').toUpperCase() }}</span></div>
          <div class="hw-row"><span class="hw-l">CPU</span><span class="hw-v hw-sm" :title="osStats.cpuModel">{{ osStats.cpuModel || '...' }}</span></div>
          <div class="hw-row"><span class="hw-l">{{ t('sys_cores') }}</span><span class="hw-v">{{ hardware.cores }}</span></div>
          <div class="hw-row"><span class="hw-l">{{ t('sys_ram') }}</span><span class="hw-v">{{ osStats.totalRamGB || hardware.ram }} GB</span></div>
        </div>
        <div class="hw-box">
          <div class="hw-hdr"><span>🎮</span><span class="hw-title">{{ t('sys_gpu').toUpperCase() }}</span></div>
          <div class="hw-row"><span class="hw-l">GPU</span><span class="hw-v hw-sm" :title="hardware.gpu">{{ hardware.gpu }}</span></div>
          <div class="hw-row"><span class="hw-l">{{ t('sys_type') }}</span><span :class="['badge', hardware.isWeakGPU ? 'badge-warning' : 'badge-success']" style="font-size:10px">{{ hardware.isWeakGPU ? 'ONBOARD' : 'DISCRETE' }}</span></div>
          <div class="hw-row"><span class="hw-l">{{ t('sys_api') }}</span><span class="hw-v">{{ hardware.webglVersion }}</span></div>
        </div>
      </div>
    </div>

    <!-- Telemetry -->
    <div class="section-subtitle" style="margin-top:var(--space-lg)">{{ t('sys_event') }}</div>
    <div class="card logs-card">
      <div v-if="!telemetry.length" class="empty-logs">✅ {{ t('sys_stable') }}</div>
      <div v-else class="log-list">
        <div v-for="(log, i) in telemetry" :key="i" :class="['log-item', log.level]">
          <span class="log-t">{{ new Date(log.time).toLocaleTimeString() }}</span>
          <span class="log-m">{{ log.message }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.system-view { padding: var(--space-lg); overflow-y: auto; height: 100%; }

/* Control Card & Actions */
.control-card { padding: var(--space-md); margin-bottom: var(--space-md); }
.control-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: var(--space-sm); }
.btn-control {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  padding: var(--space-md);
  background: var(--bg-inset);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  cursor: pointer;
  text-align: left;
  transition: all var(--transition-fast);
  position: relative;
  outline: none;
  width: 100%;
}
.btn-control:hover:not(:disabled) {
  background: var(--bg-hover);
  border-color: var(--accent-start);
  transform: translateY(-2px);
  box-shadow: var(--shadow-glow);
}
.btn-control:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.btn-icon {
  font-size: 24px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  background: var(--bg-secondary);
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
}
.btn-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}
.btn-title {
  font-size: 13px;
  font-weight: 700;
  color: var(--text-primary);
}
.btn-desc {
  font-size: 10px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.btn-danger-action:hover:not(:disabled) {
  border-color: var(--color-danger) !important;
  box-shadow: 0 0 16px rgba(248, 81, 73, 0.15) !important;
}
.control-spinner {
  width: 14px;
  height: 14px;
  border: 2px solid var(--border-default);
  border-top-color: var(--accent-start);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  position: absolute;
  right: 12px;
  top: 12px;
}
.page-header { margin-bottom: var(--space-md); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }

/* Banner */
.health-banner { display:flex; align-items:center; gap:var(--space-lg); padding:var(--space-lg); background:var(--bg-secondary); border:1px solid var(--border-default); border-radius:var(--radius-lg); position:relative; overflow:hidden; }
.health-banner::before { content:''; position:absolute; top:-40px; right:-40px; width:120px; height:120px; background:var(--hc); opacity:.06; border-radius:50%; filter:blur(40px); }
.h-ring { position:relative; width:64px; height:64px; flex-shrink:0; }
.h-ring svg { width:100%; height:100%; }
.h-score { position:absolute; top:50%; left:50%; transform:translate(-50%,-50%); font-size:16px; font-weight:800; color:var(--text-primary); }
.h-info { display:flex; flex-direction:column; gap:2px; }
.h-label { font-size:18px; font-weight:700; text-transform:uppercase; letter-spacing:1px; }
.h-sub { font-size:12px; color:var(--text-secondary); }
.h-meta { margin-left:auto; display:flex; gap:var(--space-md); flex-wrap:wrap; }
.hm { display:flex; flex-direction:column; align-items:flex-end; gap:2px; }
.hm-l { font-size:10px; color:var(--text-muted); text-transform:uppercase; letter-spacing:.5px; font-weight:600; }
.hm-v { font-size:12px; font-weight:600; color:var(--text-primary); }
.model-t { max-width:140px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }

/* Service Grid */
.svc-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(210px,1fr)); gap:var(--space-sm); }
.svc-card { display:flex; background:var(--bg-secondary); border:1px solid var(--border-default); border-radius:var(--radius-md); overflow:hidden; transition:all var(--transition-fast); }
.svc-card:hover { border-color:rgba(124,58,237,.3); box-shadow:var(--shadow-glow); }
.svc-strip { width:3px; flex-shrink:0; }
.svc-strip.online { background:var(--color-success); }
.svc-strip.degraded { background:var(--color-warning); }
.svc-strip.loading,.svc-strip.standby { background:var(--color-info); animation:pulse 1.5s infinite; }
.svc-strip.offline { background:var(--color-danger); }
.svc-strip.not_configured { background:var(--text-muted); }
.svc-body { flex:1; padding:10px 12px; display:flex; flex-direction:column; gap:8px; }
.svc-top { display:flex; align-items:center; gap:var(--space-sm); }
.svc-icon { font-size:20px; }
.svc-info { flex:1; min-width:0; }
.svc-name { font-size:12px; font-weight:600; color:var(--text-primary); }
.svc-detail { font-size:10px; color:var(--text-muted); margin-top:1px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.dot { width:8px; height:8px; border-radius:50%; flex-shrink:0; }
.dot.online { background:var(--color-success); box-shadow:0 0 6px var(--color-success); }
.dot.degraded { background:var(--color-warning); animation:pulse 1.5s infinite; }
.dot.loading,.dot.standby { background:var(--color-info); animation:pulse 1s infinite; }
.dot.offline { background:var(--color-danger); }
.dot.not_configured { background:var(--text-muted); }
.svc-bottom { display:flex; align-items:center; gap:var(--space-sm); }
.svc-port { font-size:10px; color:var(--text-muted); font-family:'JetBrains Mono',monospace; }
.svc-lat { margin-left:auto; font-size:10px; font-family:'JetBrains Mono',monospace; color:var(--color-success); font-weight:600; }

/* Hardware */
.hw-card { padding:var(--space-md); }
.hw-grid { display:grid; grid-template-columns:repeat(3,1fr); gap:var(--space-md); }
@media(max-width:768px) { .hw-grid { grid-template-columns:1fr; } .health-banner { flex-wrap:wrap; } .h-meta { margin-left:0; width:100%; justify-content:space-around; } }
.hw-box { display:flex; flex-direction:column; gap:6px; padding:var(--space-sm) var(--space-md); background:var(--bg-inset); border:1px solid var(--border-subtle); border-radius:var(--radius-sm); }
.hw-hdr { display:flex; align-items:center; gap:6px; margin-bottom:2px; }
.hw-title { font-size:9px; font-weight:700; text-transform:uppercase; letter-spacing:1px; color:var(--text-muted); }
.hw-row { display:flex; justify-content:space-between; align-items:center; gap:8px; }
.hw-l { font-size:11px; color:var(--text-muted); white-space:nowrap; }
.hw-v { font-size:11px; font-weight:500; color:var(--text-primary); text-align:right; }
.hw-sm { max-width:160px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }

/* Logs */
.logs-card { padding:var(--space-md); max-height:180px; overflow-y:auto; background:var(--bg-inset); }
.empty-logs { font-size:12px; color:var(--color-success); text-align:center; padding:var(--space-sm) 0; }
.log-list { display:flex; flex-direction:column; gap:4px; }
.log-item { display:flex; gap:var(--space-md); font-size:11px; font-family:'JetBrains Mono',monospace; padding:3px 6px; border-radius:3px; }
.log-item.info { color:var(--text-primary); }
.log-item.warning,.log-item.warn { color:var(--color-warning); background:rgba(255,171,0,.08); }
.log-item.error { color:var(--color-danger); background:rgba(255,86,48,.08); }
.log-t { color:var(--text-muted); flex-shrink:0; }
.log-m { word-break:break-all; }
</style>
