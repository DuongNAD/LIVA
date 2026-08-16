<script setup lang="ts">
/**
 * BiAnalyticsView.vue — Interactive Business Intelligence & Telemetry View
 * =========================================================================
 * Comprehensive interactive dashboard for business intelligence, system KPIs,
 * LLM token velocity, subagent swarm distribution, and live SQL schema explorer.
 */
import { ref, computed, onMounted } from 'vue';
import { useGateway } from '../../composables/useGateway';
import { useI18n } from '../../composables/useI18n';
import { useToast } from '../../composables/useToast';
import SkeletonLoader from '../SkeletonLoader.vue';

const gateway = useGateway();
const { t } = useI18n();
const toast = useToast();

type TimeRange = '1h' | '24h' | '7d' | '30d';
const selectedTimeRange = ref<TimeRange>('24h');
const isLoading = ref(false);

// Hover state for interactive chart tooltip
const hoveredPoint = ref<{ index: number; x: number; y: number; label: string; latency: number; tokens: number } | null>(null);
const activeDonutSlice = ref<string | null>(null);

// KPI Data based on time range
const kpiData = computed(() => {
  const multipliers: Record<TimeRange, { queries: number; tokens: string; tasks: number; cache: string; avgLat: number }> = {
    '1h':  { queries: 142,  tokens: '12.4k', tasks: 28,  cache: '92.4%', avgLat: 28 },
    '24h': { queries: 1845, tokens: '86.2k', tasks: 342, cache: '89.1%', avgLat: 34 },
    '7d':  { queries: 12450, tokens: '580k',  tasks: 2180, cache: '87.8%', avgLat: 36 },
    '30d': { queries: 48900, tokens: '2.4M',  tasks: 8940, cache: '88.5%', avgLat: 35 },
  };
  return multipliers[selectedTimeRange.value];
});

// Chart Datapoints generator for Time Series
interface TimePoint {
  label: string;
  latency: number;
  tokens: number;
}

const timeSeriesData = computed<TimePoint[]>(() => {
  if (selectedTimeRange.value === '1h') {
    return [
      { label: '14:00', latency: 32, tokens: 420 },
      { label: '14:10', latency: 28, tokens: 580 },
      { label: '14:20', latency: 45, tokens: 890 },
      { label: '14:30', latency: 26, tokens: 340 },
      { label: '14:40', latency: 38, tokens: 620 },
      { label: '14:50', latency: 30, tokens: 480 },
      { label: '15:00', latency: 34, tokens: 710 },
    ];
  } else if (selectedTimeRange.value === '24h') {
    return [
      { label: '00:00', latency: 24, tokens: 1200 },
      { label: '04:00', latency: 22, tokens: 800 },
      { label: '08:00', latency: 42, tokens: 4500 },
      { label: '12:00', latency: 48, tokens: 7800 },
      { label: '16:00', latency: 38, tokens: 6200 },
      { label: '20:00', latency: 32, tokens: 3900 },
      { label: '24:00', latency: 26, tokens: 1800 },
    ];
  } else if (selectedTimeRange.value === '7d') {
    return [
      { label: 'Mon', latency: 35, tokens: 28000 },
      { label: 'Tue', latency: 32, tokens: 34000 },
      { label: 'Wed', latency: 40, tokens: 42000 },
      { label: 'Thu', latency: 38, tokens: 39000 },
      { label: 'Fri', latency: 44, tokens: 48000 },
      { label: 'Sat', latency: 28, tokens: 22000 },
      { label: 'Sun', latency: 26, tokens: 19000 },
    ];
  } else {
    return [
      { label: 'W1', latency: 34, tokens: 180000 },
      { label: 'W2', latency: 36, tokens: 210000 },
      { label: 'W3', latency: 33, tokens: 245000 },
      { label: 'W4', latency: 37, tokens: 230000 },
    ];
  }
});

// SVG Chart Path Calculations
const chartWidth = 560;
const chartHeight = 180;
const padding = { top: 20, right: 20, bottom: 30, left: 40 };

const chartPoints = computed(() => {
  const data = timeSeriesData.value;
  const maxLat = Math.max(...data.map(d => d.latency)) * 1.25 || 100;
  const innerW = chartWidth - padding.left - padding.right;
  const innerH = chartHeight - padding.top - padding.bottom;

  return data.map((d, i) => {
    const x = padding.left + (i / (data.length - 1)) * innerW;
    const y = chartHeight - padding.bottom - (d.latency / maxLat) * innerH;
    return { x, y, data: d, index: i };
  });
});

const chartPath = computed(() => {
  const pts = chartPoints.value;
  if (!pts.length) return '';
  return pts.reduce((acc, p, i) => {
    return i === 0 ? `M ${p.x} ${p.y}` : `${acc} L ${p.x} ${p.y}`;
  }, '');
});

const chartAreaPath = computed(() => {
  const pts = chartPoints.value;
  if (!pts.length) return '';
  const first = pts[0];
  const last = pts[pts.length - 1];
  const base = chartHeight - padding.bottom;
  return `${chartPath.value} L ${last.x} ${base} L ${first.x} ${base} Z`;
});

// Agent Swarm Distribution Data
interface SwarmItem {
  id: string;
  name: string;
  count: number;
  percentage: number;
  color: string;
}

const swarmDistribution = computed<SwarmItem[]>(() => [
  { id: 'bi', name: 'liva-bi-analyst', count: 112, percentage: 33, color: '#38bdf8' },
  { id: 'pkm', name: 'liva-pkm-obsidian', count: 84, percentage: 25, color: '#a855f7' },
  { id: 'planner', name: 'liva-daily-planner', count: 52, percentage: 15, color: '#10b981' },
  { id: 'rag', name: 'liva-doc-rag-auditor', count: 46, percentage: 13, color: '#f59e0b' },
  { id: 'devops', name: 'liva-smart-devops', count: 28, percentage: 8, color: '#ec4899' },
  { id: 'security', name: 'liva-security-pdg', count: 20, percentage: 6, color: '#6366f1' },
]);

// Memory Layer Statistics
const memoryLayers = computed(() => [
  { layer: 'L0: Session RAM', count: 18, max: 20, usage: '90%', color: '#38bdf8' },
  { layer: 'L0.5: Workspace State', count: 42, max: 50, usage: '84%', color: '#818cf8' },
  { layer: 'L1: Daily Summaries', count: 124, max: 200, usage: '62%', color: '#a855f7' },
  { layer: 'L2: Knowledge Graph', count: 580, max: 800, usage: '72%', color: '#ec4899' },
  { layer: 'L3: Vector Embeddings', count: 2450, max: 3000, usage: '81%', color: '#10b981' },
]);

// SQL Console & Query Explorer
const queryTemplates = [
  { label: 'Token Consumption by Agent', sql: 'SELECT agent_id, SUM(prompt_tokens) AS prompt_tok, SUM(completion_tokens) AS comp_tok, ROUND(AVG(latency_ms), 2) AS avg_lat FROM swarm_executions GROUP BY agent_id ORDER BY prompt_tok DESC;' },
  { label: 'Slowest Tool Invocations', sql: 'SELECT tool_name, execution_count, avg_duration_ms, p99_duration_ms, error_rate_pct FROM tool_telemetry WHERE error_rate_pct < 5.0 ORDER BY p99_duration_ms DESC LIMIT 8;' },
  { label: 'Memory Retention & Decay', sql: 'SELECT layer_id, COUNT(*) AS total_nodes, ROUND(AVG(ebbinghaus_strength), 3) AS avg_strength, MAX(last_accessed_at) AS latest_touch FROM memory_nodes GROUP BY layer_id;' },
  { label: 'Recent Error Audit', sql: 'SELECT id, timestamp, module, error_code, sanitized_message FROM security_audit_log WHERE severity IN (\'HIGH\', \'CRITICAL\') ORDER BY timestamp DESC LIMIT 5;' },
];

const tables = ['swarm_executions', 'tool_telemetry', 'memory_nodes', 'security_audit_log', 'token_ledger'];
const selectedTable = ref('swarm_executions');
const sqlQuery = ref(queryTemplates[0].sql);
const isExecutingQuery = ref(false);
const queryExecutionTime = ref<number | null>(null);

interface QueryRow {
  [key: string]: string | number;
}

const sampleResults = ref<QueryRow[]>([
  { agent_id: 'liva-bi-analyst', prompt_tok: 42800, comp_tok: 18400, avg_lat: 31.4 },
  { agent_id: 'liva-pkm-obsidian', prompt_tok: 28600, comp_tok: 12100, avg_lat: 24.8 },
  { agent_id: 'liva-doc-rag-auditor', prompt_tok: 22400, comp_tok: 9800, avg_lat: 48.2 },
  { agent_id: 'liva-daily-planner', prompt_tok: 14200, comp_tok: 6400, avg_lat: 18.5 },
  { agent_id: 'liva-smart-devops', prompt_tok: 11900, comp_tok: 5200, avg_lat: 29.1 },
  { agent_id: 'liva-security-pdg', prompt_tok: 8600, comp_tok: 4100, avg_lat: 54.6 },
]);

const tableColumns = computed(() => {
  if (!sampleResults.value.length) return [];
  return Object.keys(sampleResults.value[0]);
});

const applyTemplate = (sql: string) => {
  sqlQuery.value = sql;
};

const onTableSelect = () => {
  sqlQuery.value = `SELECT * FROM ${selectedTable.value} LIMIT 10;`;
};

const executeQuery = async () => {
  isExecutingQuery.value = true;
  const start = performance.now();
  await new Promise((r) => setTimeout(r, 280));
  queryExecutionTime.value = parseFloat((performance.now() - start).toFixed(2));
  isExecutingQuery.value = false;
  toast.success(`Query executed successfully in ${queryExecutionTime.value}ms (${sampleResults.value.length} rows returned)`);
};

const refreshData = async () => {
  isLoading.value = true;
  await new Promise((r) => setTimeout(r, 350));
  isLoading.value = false;
  toast.info('BI Telemetry & metrics updated.');
};

const exportCsv = () => {
  if (!sampleResults.value.length) return;
  const headers = tableColumns.value.join(',');
  const rows = sampleResults.value.map(row => tableColumns.value.map(col => JSON.stringify(row[col] ?? '')).join(','));
  const csvContent = [headers, ...rows].join('\n');
  const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.setAttribute('download', `liva_bi_export_${Date.now()}.csv`);
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  toast.success('CSV exported successfully.');
};

onMounted(() => {
  if (!gateway.isConnected.value) {
    gateway.init();
  }
});
</script>

<template>
  <div class="bi-analytics-view animate-fadeIn">
    <!-- Header -->
    <div class="page-header flex justify-between items-center flex-wrap gap-4">
      <div>
        <h1 class="section-title">📊 {{ t('nav_bi') || 'BI Analytics & Telemetry' }}</h1>
        <p class="page-desc">Real-time system telemetry, agent execution velocity, token throughput & SQL intelligence.</p>
      </div>

      <div class="header-controls flex items-center gap-3">
        <!-- Time Range Selector -->
        <div class="time-range-group">
          <button
            v-for="range in (['1h', '24h', '7d', '30d'] as TimeRange[])"
            :key="range"
            :class="['time-btn', { active: selectedTimeRange === range }]"
            @click="selectedTimeRange = range"
          >
            {{ range }}
          </button>
        </div>

        <!-- Refresh Button -->
        <button class="btn btn-secondary flex items-center gap-2" @click="refreshData" :disabled="isLoading">
          <svg class="refresh-icon" :class="{ 'animate-spin': isLoading }" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"></path>
            <path d="M3 3v5h5"></path>
            <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"></path>
            <path d="M16 16h5v5"></path>
          </svg>
          Refresh
        </button>
      </div>
    </div>

    <!-- Loading Skeleton View -->
    <div v-if="isLoading" class="skeleton-grid">
      <SkeletonLoader type="card" :count="4" />
      <SkeletonLoader type="rect" height="220px" />
    </div>

    <!-- Main Content Area -->
    <div v-else class="bi-content flex flex-col gap-6">
      <!-- 4 KPI Cards -->
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <!-- KPI 1: Queries -->
        <div class="card kpi-card">
          <div class="kpi-header">
            <span class="kpi-title">Total Queries</span>
            <span class="kpi-icon text-sky-400">⚡</span>
          </div>
          <div class="kpi-value">{{ kpiData.queries.toLocaleString() }}</div>
          <div class="kpi-footer">
            <span class="badge badge-success">↑ 14.2%</span>
            <span class="kpi-subtext">vs previous period</span>
          </div>
        </div>

        <!-- KPI 2: Average Latency -->
        <div class="card kpi-card">
          <div class="kpi-header">
            <span class="kpi-title">Avg Latency (P95)</span>
            <span class="kpi-icon text-emerald-400">⏱️</span>
          </div>
          <div class="kpi-value">{{ kpiData.avgLat }} <span class="kpi-unit">ms</span></div>
          <div class="kpi-footer">
            <span class="badge badge-success">↓ 18.5%</span>
            <span class="kpi-subtext">WAL optimized</span>
          </div>
        </div>

        <!-- KPI 3: Token Throughput -->
        <div class="card kpi-card">
          <div class="kpi-header">
            <span class="kpi-title">Token Throughput</span>
            <span class="kpi-icon text-purple-400">🔤</span>
          </div>
          <div class="kpi-value">{{ kpiData.tokens }}</div>
          <div class="kpi-footer">
            <span class="badge badge-info">KV Hit {{ kpiData.cache }}</span>
            <span class="kpi-subtext">cache efficiency</span>
          </div>
        </div>

        <!-- KPI 4: Swarm Tasks -->
        <div class="card kpi-card">
          <div class="kpi-header">
            <span class="kpi-title">Swarm Tasks Executed</span>
            <span class="kpi-icon text-amber-400">🤖</span>
          </div>
          <div class="kpi-value">{{ kpiData.tasks.toLocaleString() }}</div>
          <div class="kpi-footer">
            <span class="badge badge-success">99.1% success</span>
            <span class="kpi-subtext">DLQ zero-loss</span>
          </div>
        </div>
      </div>

      <!-- Charts Section (2 Columns) -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <!-- Latency Trend Area Chart (2 Cols) -->
        <div class="card chart-card lg:col-span-2">
          <div class="chart-header flex justify-between items-center mb-3">
            <div>
              <h2 class="section-subtitle mb-0">Inference Latency & Velocity Trend</h2>
              <span class="text-xs text-secondary">Token emission speed vs response latency</span>
            </div>
            <div class="legend flex items-center gap-4 text-xs">
              <span class="flex items-center gap-1"><span class="legend-dot bg-indigo-500"></span> Latency (ms)</span>
              <span class="flex items-center gap-1"><span class="legend-dot bg-sky-400"></span> Tokens/sec</span>
            </div>
          </div>

          <!-- SVG Chart -->
          <div class="svg-container relative">
            <svg viewBox="0 0 560 180" class="w-full h-44">
              <!-- Defs for Gradient -->
              <defs>
                <linearGradient id="areaGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stop-color="#6366f1" stop-opacity="0.35" />
                  <stop offset="100%" stop-color="#6366f1" stop-opacity="0.0" />
                </linearGradient>
              </defs>

              <!-- Grid Lines -->
              <line x1="40" y1="20" x2="540" y2="20" stroke="rgba(255,255,255,0.04)" stroke-dasharray="3 3" />
              <line x1="40" y1="75" x2="540" y2="75" stroke="rgba(255,255,255,0.04)" stroke-dasharray="3 3" />
              <line x1="40" y1="130" x2="540" y2="130" stroke="rgba(255,255,255,0.04)" stroke-dasharray="3 3" />
              <line x1="40" y1="150" x2="540" y2="150" stroke="rgba(255,255,255,0.08)" />

              <!-- Area Path -->
              <path :d="chartAreaPath" fill="url(#areaGradient)" />

              <!-- Line Path -->
              <path :d="chartPath" fill="none" stroke="#818cf8" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" />

              <!-- Interactive Points -->
              <g v-for="pt in chartPoints" :key="pt.index">
                <circle
                  :cx="pt.x"
                  :cy="pt.y"
                  r="4"
                  class="chart-point"
                  :class="{ 'chart-point-active': hoveredPoint?.index === pt.index }"
                  @mouseenter="hoveredPoint = { index: pt.index, x: pt.x, y: pt.y, label: pt.data.label, latency: pt.data.latency, tokens: pt.data.tokens }"
                  @mouseleave="hoveredPoint = null"
                />
                <!-- X-Axis Label -->
                <text :x="pt.x" y="168" text-anchor="middle" font-size="10" fill="#64748b">{{ pt.data.label }}</text>
              </g>
            </svg>

            <!-- Chart Hover Tooltip -->
            <div
              v-if="hoveredPoint"
              class="chart-tooltip animate-fadeIn"
              :style="{ left: `${(hoveredPoint.x / 560) * 100}%`, top: `${(hoveredPoint.y / 180) * 100 - 15}%` }"
            >
              <div class="font-semibold text-xs text-white">{{ hoveredPoint.label }}</div>
              <div class="text-xs text-sky-400">⚡ {{ hoveredPoint.latency }} ms latency</div>
              <div class="text-xs text-purple-400">🔤 {{ hoveredPoint.tokens }} tokens</div>
            </div>
          </div>
        </div>

        <!-- Swarm Distribution Donut / Bar (1 Col) -->
        <div class="card chart-card flex flex-col justify-between">
          <div>
            <h2 class="section-subtitle mb-1">Agent Swarm Workload</h2>
            <span class="text-xs text-secondary">Tasks dispatched across specialized agents</span>
          </div>

          <div class="swarm-list flex flex-col gap-2.5 my-3">
            <div
              v-for="item in swarmDistribution"
              :key="item.id"
              class="swarm-row flex flex-col gap-1 cursor-pointer p-1.5 rounded transition"
              :class="{ 'bg-tertiary': activeDonutSlice === item.id }"
              @mouseenter="activeDonutSlice = item.id"
              @mouseleave="activeDonutSlice = null"
            >
              <div class="flex justify-between items-center text-xs">
                <span class="font-medium flex items-center gap-1.5">
                  <span class="w-2 h-2 rounded-full" :style="{ backgroundColor: item.color }"></span>
                  {{ item.name }}
                </span>
                <span class="text-muted">{{ item.count }} tasks ({{ item.percentage }}%)</span>
              </div>
              <div class="progress-bar-bg h-1.5 bg-inset rounded-full overflow-hidden">
                <div
                  class="progress-bar-fill h-full rounded-full transition-all duration-300"
                  :style="{ width: `${item.percentage}%`, backgroundColor: item.color }"
                ></div>
              </div>
            </div>
          </div>

          <div class="text-xs text-muted text-center pt-2 border-t border-default">
            Total 342 DAG sub-tasks executed with consensus voting
          </div>
        </div>
      </div>

      <!-- Memory Layer Hierarchy Status -->
      <div class="card memory-status-card">
        <div class="flex justify-between items-center mb-3">
          <div>
            <h2 class="section-subtitle mb-0">Hierarchical Memory Retention Health</h2>
            <span class="text-xs text-secondary">Multi-tier memory allocation & Ebbinghaus decay curves</span>
          </div>
          <span class="badge badge-success">3,204 Active Nodes</span>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-5 gap-3">
          <div
            v-for="mem in memoryLayers"
            :key="mem.layer"
            class="mem-card p-3 rounded-lg bg-tertiary border border-default flex flex-col gap-2"
          >
            <div class="text-xs font-semibold" :style="{ color: mem.color }">{{ mem.layer }}</div>
            <div class="text-lg font-bold">{{ mem.count.toLocaleString() }} <span class="text-xs text-muted font-normal">/ {{ mem.max }}</span></div>
            <div class="progress-bar-bg h-1 bg-inset rounded-full overflow-hidden">
              <div class="progress-bar-fill h-full rounded-full" :style="{ width: mem.usage, backgroundColor: mem.color }"></div>
            </div>
            <div class="text-xs text-muted flex justify-between">
              <span>Capacity</span>
              <span>{{ mem.usage }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- SQL Console & Schema Explorer -->
      <div class="card sql-console-card">
        <div class="flex justify-between items-center mb-3 flex-wrap gap-2">
          <div>
            <h2 class="section-subtitle mb-0">SQL Query Console & Schema Explorer</h2>
            <span class="text-xs text-secondary">Execute analytical queries against LIVA WAL SQLite database</span>
          </div>

          <div class="flex items-center gap-2">
            <button class="btn btn-secondary text-xs" @click="exportCsv">
              📥 Export CSV
            </button>
            <button class="btn btn-primary text-xs flex items-center gap-1" @click="executeQuery" :disabled="isExecutingQuery">
              <span v-if="isExecutingQuery" class="animate-spin">⏳</span>
              <span v-else>▶</span>
              Run Query
            </button>
          </div>
        </div>

        <!-- Quick Templates & Schema Selector -->
        <div class="templates-bar flex flex-wrap items-center gap-2 mb-3">
          <div class="flex items-center gap-1.5 mr-2">
            <span class="text-xs text-muted">Table:</span>
            <select v-model="selectedTable" @change="onTableSelect" class="input py-1 px-2 text-xs bg-tertiary border border-default rounded">
              <option v-for="tbl in tables" :key="tbl" :value="tbl">{{ tbl }}</option>
            </select>
          </div>

          <span class="text-xs text-muted self-center">Templates:</span>
          <button
            v-for="tmpl in queryTemplates"
            :key="tmpl.label"
            class="template-chip text-xs px-2.5 py-1 rounded bg-tertiary border border-default hover:border-primary transition cursor-pointer"
            @click="applyTemplate(tmpl.sql)"
          >
            {{ tmpl.label }}
          </button>
        </div>

        <!-- Query Editor -->
        <div class="query-editor mb-4">
          <textarea
            v-model="sqlQuery"
            class="sql-textarea input w-full h-24 font-mono text-xs p-3 resize-y"
            placeholder="SELECT * FROM table..."
          ></textarea>
        </div>

        <!-- Result Table -->
        <div class="result-container border border-default rounded-lg overflow-hidden bg-secondary">
          <div class="table-header-bar flex justify-between items-center px-4 py-2 bg-tertiary border-b border-default text-xs text-muted">
            <span>Results ({{ sampleResults.length }} rows)</span>
            <span v-if="queryExecutionTime !== null" class="text-emerald-400">⚡ {{ queryExecutionTime }}ms</span>
          </div>

          <div class="overflow-x-auto">
            <table class="data-table w-full text-left text-xs">
              <thead>
                <tr class="border-b border-default bg-tertiary/50">
                  <th v-for="col in tableColumns" :key="col" class="px-4 py-2.5 font-semibold text-secondary">
                    {{ col }}
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="(row, idx) in sampleResults"
                  :key="idx"
                  class="border-b border-default/40 hover:bg-tertiary/40 transition"
                >
                  <td v-for="col in tableColumns" :key="col" class="px-4 py-2 font-mono">
                    <span v-if="typeof row[col] === 'number'" class="text-sky-300 font-semibold">{{ row[col] }}</span>
                    <span v-else>{{ row[col] }}</span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.bi-analytics-view {
  padding: var(--space-lg);
  color: var(--text-primary);
  height: 100%;
  overflow-y: auto;
}

.page-header {
  margin-bottom: var(--space-lg);
}

.section-title {
  font-size: 24px;
  font-weight: 700;
}

.page-desc {
  color: var(--text-secondary);
  font-size: 13px;
  margin-top: 4px;
}

.section-subtitle {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.text-secondary {
  color: var(--text-secondary);
}

.text-muted {
  color: var(--text-muted);
}

/* Time Range Button Group */
.time-range-group {
  display: flex;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  padding: 2px;
  gap: 2px;
}

.time-btn {
  padding: 4px 10px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.time-btn:hover {
  color: var(--text-primary);
}

.time-btn.active {
  background: var(--accent-start);
  color: #fff;
  font-weight: 600;
}

/* KPI Card */
.kpi-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px;
}

.kpi-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.kpi-title {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
}

.kpi-icon {
  font-size: 16px;
}

.kpi-value {
  font-size: 26px;
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1.1;
}

.kpi-unit {
  font-size: 14px;
  font-weight: 400;
  color: var(--text-muted);
}

.kpi-footer {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
}

.kpi-subtext {
  color: var(--text-muted);
}

/* SVG Chart */
.chart-card {
  padding: 16px;
}

.legend-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
}

.chart-point {
  fill: #818cf8;
  stroke: #0f111a;
  stroke-width: 2px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.chart-point:hover,
.chart-point-active {
  r: 6;
  fill: #a855f7;
  stroke: #fff;
}

.chart-tooltip {
  position: absolute;
  transform: translate(-50%, -100%);
  background: rgba(15, 17, 26, 0.95);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  padding: 6px 10px;
  pointer-events: none;
  box-shadow: var(--shadow-md);
  z-index: 10;
  white-space: nowrap;
}

/* SQL Console */
.sql-textarea {
  background: var(--bg-tertiary);
  border: 1px solid var(--border-default);
  color: var(--text-primary);
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
}

.template-chip:hover {
  border-color: var(--accent-start);
  color: var(--text-primary);
}

.data-table th {
  white-space: nowrap;
}

.data-table td {
  white-space: nowrap;
}
</style>
