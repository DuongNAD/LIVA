<script setup lang="ts">
/**
 * MemoryViewer.vue — Visualizer of LIVA's Unified Hierarchical Memory
 * ===================================================================
 * Realizes LIVA's cognitive memory layers in a tabbed dashboard:
 *   - L0 RAM Cache: Active working memory (conversation buffer)
 *   - L0.5 Session State: Persistent session context (SESSION-STATE.md)
 *   - L1 Vector Space: sqlite-vec semantic embeddings index
 *   - L2 Cognitive Events: Dual-perspective Φ/Ψ conversation analysis timeline
 *   - L3 Facts Board: Structured facts with Ebbinghaus decay & importance rankings
 */
import { ref, computed, onActivated, onDeactivated } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";

const gateway = useGateway();
const { currentLang } = useI18n();

const activeTab = ref<"l0" | "l0_5" | "facts" | "events" | "vectors">("facts");

// Search & Filtering State
const l0Query = ref("");
const factQuery = ref("");
const eventQuery = ref("");
const vectorQuery = ref("");

// Auto-refresh timer
let refreshTimer: ReturnType<typeof setInterval> | null = null;
const isRefreshing = ref(false);

const refreshMemory = () => {
  if (gateway.isConnected.value) {
    isRefreshing.value = true;
    gateway.sendMsg("get_memory_data");
    setTimeout(() => {
      isRefreshing.value = false;
    }, 600);
  }
};

// Manual consolidation trigger (migrated from former 3D tab)
const isConsolidating = ref(false);
const triggerConsolidation = () => {
  if (gateway.isConnected.value && !isConsolidating.value) {
    isConsolidating.value = true;
    gateway.sendMsg("consolidate_memory", { force: true });
    setTimeout(() => {
      isConsolidating.value = false;
      refreshMemory();
    }, 12000);
  }
};

// ═══════════════════════════════════════════════════════
//  Facts Selectors & Filtering
// ═══════════════════════════════════════════════════════
const filteredFacts = computed(() => {
  const list = Array.isArray(gateway.memoryData.value?.facts) ? gateway.memoryData.value.facts : [];
  const validList = list.filter((f: any) => f && typeof f === 'object' && f.key);
  if (!factQuery.value.trim()) return validList;
  const q = factQuery.value.toLowerCase();
  return validList.filter((f: any) => 
    String(f.key || "").toLowerCase().includes(q) || 
    String(f.value || "").toLowerCase().includes(q) ||
    (f.category && String(f.category).toLowerCase().includes(q))
  );
});

const deleteFact = (key: string) => {
  if (confirm(currentLang.value === 'vi-VN' 
    ? `Bạn có chắc chắn muốn xóa sự thật "${key}" khỏi trí nhớ không?` 
    : `Are you sure you want to delete the fact "${key}" from memory?`
  )) {
    gateway.sendMsg("delete_memory_fact", { key });
  }
};

// Formatting helpers
const formatPercent = (val: number | undefined | null) => {
  if (val === undefined || val === null) return "100%";
  return `${Math.round(val * 100)}%`;
};

const formatTime = (ts: number | undefined | null) => {
  if (!ts) return "—";
  return new Date(ts).toLocaleString(currentLang.value === 'vi-VN' ? 'vi-VN' : 'en-US');
};

const formatISO = (isoStr: string | undefined | null) => {
  if (!isoStr) return "—";
  return new Date(isoStr).toLocaleString(currentLang.value === 'vi-VN' ? 'vi-VN' : 'en-US');
};

// ═══════════════════════════════════════════════════════
//  Event Timeline Selectors & Filtering
// ═══════════════════════════════════════════════════════
const filteredEvents = computed(() => {
  const list = Array.isArray(gateway.memoryData.value?.events) ? gateway.memoryData.value.events : [];
  const validList = list.filter((e: any) => e && typeof e === 'object' && e.eventId);
  if (!eventQuery.value.trim()) return validList;
  const q = eventQuery.value.toLowerCase();
  return validList.filter((e: any) => 
    String(e.rawUserMsg || "").toLowerCase().includes(q) || 
    String(e.rawAiReply || "").toLowerCase().includes(q) ||
    (e.psi?.intent && String(e.psi.intent).toLowerCase().includes(q)) ||
    (e.psi?.sentiment && String(e.psi.sentiment).toLowerCase().includes(q)) ||
    String(e.domain || "").toLowerCase().includes(q) ||
    String(e.category || "").toLowerCase().includes(q)
  );
});

// ═══════════════════════════════════════════════════════
//  Vector Index Selectors & Filtering
// ═══════════════════════════════════════════════════════
const filteredVectors = computed(() => {
  const list = Array.isArray(gateway.memoryData.value?.vectors) ? gateway.memoryData.value.vectors : [];
  const validList = list.filter((v: any) => v && typeof v === 'object' && v.vecId);
  if (!vectorQuery.value.trim()) return validList;
  const q = vectorQuery.value.toLowerCase();
  return validList.filter((v: any) => 
    String(v.content || "").toLowerCase().includes(q) || 
    String(v.type || "").toLowerCase().includes(q) ||
    String(v.domain || "").toLowerCase().includes(q) ||
    String(v.category || "").toLowerCase().includes(q)
  );
});

// Statistics
const filteredL0 = computed(() => {
  const list = Array.isArray(gateway.memoryData.value?.l0) ? gateway.memoryData.value.l0 : [];
  const validList = list.filter((m: any) => m && typeof m === 'object');
  if (!l0Query.value.trim()) return validList;
  const q = l0Query.value.toLowerCase();
  return validList.filter((m: any) => 
    String(m.content || "").toLowerCase().includes(q) || 
    String(m.role || "").toLowerCase().includes(q)
  );
});

const l0Count = computed(() => {
  const l = gateway.memoryData.value?.l0;
  return Array.isArray(l) ? l.length : 0;
});

const l0_5Size = computed(() => {
  const content = String(gateway.memoryData.value?.l0_5 || "");
  if (!content) return "0 B";
  const bytes = new Blob([content]).size;
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(1)} KB`;
});

const factsCount = computed(() => {
  const l = gateway.memoryData.value?.facts;
  return Array.isArray(l) ? l.length : 0;
});
const eventsCount = computed(() => {
  const l = gateway.memoryData.value?.events;
  return Array.isArray(l) ? l.length : 0;
});
const vectorsCount = computed(() => {
  const l = gateway.memoryData.value?.vectors;
  return Array.isArray(l) ? l.length : 0;
});

const totalMemories = computed(() => l0Count.value + factsCount.value + eventsCount.value + vectorsCount.value);

onActivated(() => {
  refreshMemory();
  refreshTimer = setInterval(() => {
    gateway.sendMsg("get_memory_data");
  }, 10000);
});

onDeactivated(() => {
  if (refreshTimer) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }
});


</script>

<template>
  <div class="memory-viewer animate-fadeIn">
    <!-- Header -->
    <div class="page-header">
      <div class="header-row">
        <div>
          <h1 class="section-title">🧠 {{ currentLang === 'vi-VN' ? 'Không gian Trí nhớ' : 'Memory Space' }}</h1>
          <p class="page-desc">
            {{ currentLang === 'vi-VN' 
              ? `Hệ thống ký ức hợp nhất — ${totalMemories} mục nhớ trên 5 tầng (L0 RAM, L0.5 Phiên, L1 Vector, L2 Sự kiện, L3 Sự thật).`
              : `Unified Hierarchical Memory — ${totalMemories} memories across 5 layers (L0 RAM, L0.5 Session, L1 Vectors, L2 Events, L3 Facts).`
            }}
          </p>
        </div>
        <div class="header-actions">
          <button class="btn btn-secondary btn-sm" @click="triggerConsolidation" :disabled="isConsolidating">
            <span v-if="isConsolidating" class="spinner"></span>
            <span v-else>⚡ {{ currentLang === 'vi-VN' ? 'Hợp nhất' : 'Consolidate' }}</span>
          </button>
          <button class="btn btn-secondary btn-sm" @click="refreshMemory" :disabled="isRefreshing">
            <span v-if="isRefreshing" class="spinner"></span>
            <span v-else>🔄 {{ currentLang === 'vi-VN' ? 'Làm mới' : 'Refresh' }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Quick Stats Grid -->
    <div class="stats-grid five-cols">
      <div class="stat-card l0-stat" @click="activeTab = 'l0'" :class="{ active: activeTab === 'l0' }">
        <div class="stat-icon">🧠</div>
        <div class="stat-info">
          <h3>{{ l0Count }}</h3>
          <p>{{ currentLang === 'vi-VN' ? 'Trí nhớ RAM L0' : 'L0 RAM Cache' }}</p>
        </div>
      </div>
      <div class="stat-card l0-5-stat" @click="activeTab = 'l0_5'" :class="{ active: activeTab === 'l0_5' }">
        <div class="stat-icon">📑</div>
        <div class="stat-info">
          <h3>{{ l0_5Size }}</h3>
          <p>{{ currentLang === 'vi-VN' ? 'Phiên L0.5' : 'L0.5 Session' }}</p>
        </div>
      </div>
      <div class="stat-card facts-stat" @click="activeTab = 'facts'" :class="{ active: activeTab === 'facts' }">
        <div class="stat-icon">💾</div>
        <div class="stat-info">
          <h3>{{ factsCount }}</h3>
          <p>{{ currentLang === 'vi-VN' ? 'Sự thật L3' : 'L3 Facts' }}</p>
        </div>
      </div>
      <div class="stat-card events-stat" @click="activeTab = 'events'" :class="{ active: activeTab === 'events' }">
        <div class="stat-icon">⚡</div>
        <div class="stat-info">
          <h3>{{ eventsCount }}</h3>
          <p>{{ currentLang === 'vi-VN' ? 'Sự kiện L2' : 'L2 Events' }}</p>
        </div>
      </div>
      <div class="stat-card vectors-stat" @click="activeTab = 'vectors'" :class="{ active: activeTab === 'vectors' }">
        <div class="stat-icon">🌐</div>
        <div class="stat-info">
          <h3>{{ vectorsCount }}</h3>
          <p>{{ currentLang === 'vi-VN' ? 'Vector L1' : 'L1 Vectors' }}</p>
        </div>
      </div>
    </div>

    <!-- Tab Selection Navigation -->
    <div class="tab-nav">
      <button 
        class="tab-btn" 
        :class="{ active: activeTab === 'l0' }" 
        @click="activeTab = 'l0'"
      >
        🧠 {{ currentLang === 'vi-VN' ? 'L0 RAM Cache' : 'L0 RAM Cache' }}
      </button>
      <button 
        class="tab-btn" 
        :class="{ active: activeTab === 'l0_5' }" 
        @click="activeTab = 'l0_5'"
      >
        📑 {{ currentLang === 'vi-VN' ? 'L0.5 Phiên' : 'L0.5 Session' }}
      </button>
      <button 
        class="tab-btn" 
        :class="{ active: activeTab === 'vectors' }" 
        @click="activeTab = 'vectors'"
      >
        🌐 {{ currentLang === 'vi-VN' ? 'L1 Vector' : 'L1 Vectors' }}
      </button>
      <button 
        class="tab-btn" 
        :class="{ active: activeTab === 'events' }" 
        @click="activeTab = 'events'"
      >
        🕰️ {{ currentLang === 'vi-VN' ? 'L2 Sự kiện' : 'L2 Events' }}
      </button>
      <button 
        class="tab-btn" 
        :class="{ active: activeTab === 'facts' }" 
        @click="activeTab = 'facts'"
      >
        💾 {{ currentLang === 'vi-VN' ? 'L3 Sự thật' : 'L3 Facts' }}
      </button>
    </div>



    <!-- ========================================== -->
    <!-- TAB 0: RAM Working Memory Cache (L0)       -->
    <!-- ========================================== -->
    <div v-if="activeTab === 'l0'" class="tab-content animate-fadeIn">
      <div class="filter-bar">
        <div class="search-box">
          <span class="search-icon">🔍</span>
          <input
            v-model="l0Query"
            class="input search-input"
            :placeholder="currentLang === 'vi-VN' ? 'Tìm kiếm trong tin nhắn RAM Cache...' : 'Search RAM Cache messages...'"
          />
        </div>
      </div>

      <div v-if="filteredL0.length === 0" class="empty-state">
        <div class="empty-icon">🧠</div>
        <p>{{ currentLang === 'vi-VN' ? 'Không có tin nhắn nào trong bộ nhớ đệm RAM hiện tại.' : 'No messages in active RAM working memory.' }}</p>
      </div>

      <div v-else class="l0-timeline">
        <div 
          v-for="(msg, idx) in filteredL0" 
          :key="idx" 
          class="l0-message-card"
          :class="msg.role || 'system'"
        >
          <div class="msg-header">
            <span class="msg-role-badge" :class="String(msg.role || 'system')">
              {{ String(msg.role || 'SYSTEM').toUpperCase() }}
            </span>
            <span class="msg-time">{{ formatTime(msg.timestamp) }}</span>
          </div>
          <div class="msg-content">
            {{ msg.content }}
          </div>
        </div>
      </div>
    </div>

    <!-- ========================================== -->
    <!-- TAB 0.5: Session State (L0.5)              -->
    <!-- ========================================== -->
    <div v-if="activeTab === 'l0_5'" class="tab-content animate-fadeIn">
      <div class="session-state-container card">
        <div class="session-state-header">
          <div class="file-name">📄 SESSION-STATE.md</div>
          <div class="file-status">{{ currentLang === 'vi-VN' ? 'Bộ nhớ đệm Phiên làm việc (Active)' : 'Active Session State Buffer' }}</div>
        </div>
        <div class="session-state-body">
          <pre class="markdown-code">{{ gateway.memoryData.value?.l0_5 || '# SESSION STATE\n(Empty)' }}</pre>
        </div>
      </div>
    </div>

    <!-- ========================================== -->
    <!-- TAB 1: Structured Facts Board             -->
    <!-- ========================================== -->
    <div v-if="activeTab === 'facts'" class="tab-content animate-fadeIn">
      <div class="filter-bar">
        <div class="search-box">
          <span class="search-icon">🔍</span>
          <input
            v-model="factQuery"
            class="input search-input"
            :placeholder="currentLang === 'vi-VN' ? 'Tìm kiếm sự thật theo từ khóa hoặc nhãn...' : 'Search facts by keyword or category...'"
          />
        </div>
      </div>

      <div v-if="filteredFacts.length === 0" class="empty-state">
        <div class="empty-icon">📭</div>
        <p>{{ currentLang === 'vi-VN' ? 'Không tìm thấy sự thật nào khớp với bộ lọc.' : 'No structured facts found.' }}</p>
      </div>

      <div v-else class="facts-grid">
        <div 
          v-for="fact in filteredFacts" 
          :key="fact.key" 
          class="card fact-card"
        >
          <div class="fact-header">
            <span class="fact-category" :class="fact.category ? 'has-cat' : 'no-cat'">
              {{ fact.category || (currentLang === 'vi-VN' ? 'Chung' : 'General') }}
            </span>
            <button class="btn-delete" @click="deleteFact(fact.key)" :title="currentLang === 'vi-VN' ? 'Xóa sự thật này' : 'Delete this fact'">
              🗑️
            </button>
          </div>

          <div class="fact-body">
            <h3 class="fact-key">{{ fact.key }}</h3>
            <p class="fact-value">{{ fact.value }}</p>
          </div>

          <div class="fact-footer">
            <!-- Ebbinghaus strength decay curve meter -->
            <div class="strength-meter">
              <div class="meter-label">
                <span>{{ currentLang === 'vi-VN' ? 'Độ bền trí nhớ' : 'Memory strength' }}:</span>
                <span class="strength-value">{{ formatPercent(fact.memoryStrength) }}</span>
              </div>
              <div class="meter-bar-bg">
                <div 
                  class="meter-bar-fill" 
                  :style="{ 
                    width: formatPercent(fact.memoryStrength),
                    backgroundColor: fact.memoryStrength >= 0.7 ? '#10B981' : fact.memoryStrength >= 0.4 ? '#F59E0B' : '#EF4444'
                  }"
                ></div>
              </div>
            </div>

            <!-- Importance indicator -->
            <div class="importance-badge">
              <span>{{ currentLang === 'vi-VN' ? 'Tầm quan trọng' : 'Importance' }}:</span>
              <span class="importance-stars">{{ '★'.repeat(Math.ceil((fact.importance || 0.5) * 5)) }}</span>
            </div>

            <!-- Meta telemetry data -->
            <div class="fact-meta">
              <span>👤 Source: {{ fact.source }}</span>
              <span>🕒 Created: {{ formatISO(fact.createdAt) }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- ========================================== -->
    <!-- TAB 2: Event Telemetry Timeline           -->
    <!-- ========================================== -->
    <div v-if="activeTab === 'events'" class="tab-content animate-fadeIn">
      <div class="filter-bar">
        <div class="search-box">
          <span class="search-icon">🔍</span>
          <input
            v-model="eventQuery"
            class="input search-input"
            :placeholder="currentLang === 'vi-VN' ? 'Tìm kiếm trong lịch sử sự kiện...' : 'Search event logs timeline...'"
          />
        </div>
      </div>

      <div v-if="filteredEvents.length === 0" class="empty-state">
        <div class="empty-icon">⏳</div>
        <p>{{ currentLang === 'vi-VN' ? 'Không có sự kiện nhận thức nào trong cơ sở dữ liệu.' : 'No cognitive event telemetry logs found.' }}</p>
      </div>

      <div v-else class="event-timeline">
        <div 
          v-for="event in filteredEvents" 
          :key="event.eventId" 
          class="timeline-item"
          :class="{ consolidated: event.consolidationStatus === 'consolidated' }"
        >
          <div class="timeline-badge"></div>
          <div class="card timeline-card">
            <div class="card-header event-header">
              <span class="event-domain">{{ event.domain }} · {{ event.category }}</span>
              <span class="event-time">{{ formatTime(event.timestamp) }}</span>
            </div>
            
            <div class="event-content">
              <div class="msg-bubble user-bubble">
                <span class="bubble-sender">👤 {{ currentLang === 'vi-VN' ? 'Người dùng' : 'User' }}</span>
                <p>{{ event.rawUserMsg }}</p>
              </div>
              <div class="msg-bubble ai-bubble">
                <span class="bubble-sender">🤖 LIVA</span>
                <p>{{ event.rawAiReply }}</p>
              </div>
            </div>

            <!-- Cognitive Insights Extracted Layer -->
            <div class="event-insights">
              <h4 class="insight-title">👁️ {{ currentLang === 'vi-VN' ? 'Phân tích Nhận thức (Dual-Perspective Φ/Ψ)' : 'Cognitive Insights (Dual-Perspective)' }}</h4>
              
              <div class="insight-row">
                <div class="insight-col">
                  <strong>Φ Factual Core:</strong>
                  <div class="insight-tag-list">
                    <span v-for="fact in event.phi?.facts || []" :key="fact" class="tag tag-phi">💡 {{ fact }}</span>
                    <span v-for="ent in event.phi?.entities || []" :key="ent" class="tag tag-ent">🔑 {{ ent }}</span>
                    <span v-if="!(event.phi?.facts?.length) && !(event.phi?.entities?.length)" class="no-insights">
                      {{ currentLang === 'vi-VN' ? 'Không có dữ liệu sự kiện cụ thể' : 'No core facts extracted' }}
                    </span>
                  </div>
                </div>

                <div class="insight-col">
                  <strong>Ψ Psychological Intent:</strong>
                  <div class="insight-metrics">
                    <span v-if="event.psi?.sentiment" class="tag tag-psi-s">🎭 {{ event.psi.sentiment }}</span>
                    <span v-if="event.psi?.intent" class="tag tag-psi-i">🎯 {{ event.psi.intent }}</span>
                    <span v-if="event.psi?.relational" class="tag tag-psi-r">💞 {{ event.psi.relational }}</span>
                  </div>
                </div>
              </div>

              <!-- Keywords and state tag list -->
              <div class="insight-tags">
                <span v-for="kw in event.traceKeywords" :key="kw" class="keyword-tag">#{{ kw }}</span>
                <span class="status-badge" :class="String(event.consolidationStatus || 'pending')">
                  {{ String(event.consolidationStatus || 'PENDING').toUpperCase() }}
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- ========================================== -->
    <!-- TAB 3: L1 Vector Embeddings               -->
    <!-- ========================================== -->
    <div v-if="activeTab === 'vectors'" class="tab-content animate-fadeIn">
      <div class="filter-bar">
        <div class="search-box">
          <span class="search-icon">🔍</span>
          <input
            v-model="vectorQuery"
            class="input search-input"
            :placeholder="currentLang === 'vi-VN' ? 'Tìm kiếm trong không gian vector tương đồng...' : 'Search semantic vectors index...'"
          />
        </div>
      </div>

      <div v-if="filteredVectors.length === 0" class="empty-state">
        <div class="empty-icon">🌀</div>
        <p>{{ currentLang === 'vi-VN' ? 'Không có dữ liệu vector embeddings nào được ghi nhận.' : 'No semantic vectors cached.' }}</p>
      </div>

      <div v-else class="vectors-grid">
        <div 
          v-for="vec in filteredVectors" 
          :key="vec.vecId" 
          class="card vector-card"
        >
          <div class="vector-header">
            <span class="vector-type">{{ vec.type }}</span>
            <span class="vector-domain">{{ vec.domain }} · {{ vec.category }}</span>
          </div>

          <div class="vector-body">
            <p class="vector-content">"{{ vec.content }}"</p>
          </div>

          <div class="vector-footer">
            <div class="vector-keywords">
              <span v-for="kw in vec.traceKeywords" :key="kw" class="kw-pill">{{ kw }}</span>
            </div>
            
            <div class="vector-meta">
              <span class="vec-id-short">ID: {{ String(vec.vecId || '').substring(0, 8) }}...</span>
              <span class="vec-time">📅 {{ formatTime(vec.createdAt) }}</span>
            </div>

            <!-- Position pointer link to parent consolidated event -->
            <div v-if="vec.sourceEventIds && vec.sourceEventIds.length > 0" class="vector-pointers">
              <span>🔗 Pointers: </span>
              <span 
                v-for="pId in vec.sourceEventIds" 
                :key="pId" 
                class="pointer-link" 
                @click="activeTab = 'events'; eventQuery = String(pId)"
                title="Jump to Parent L2 Event"
              >
                {{ String(pId || '').substring(0, 6) }}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.memory-viewer {
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  height: 100%;
  overflow-y: auto;
  color: #e2e8f0;
}

/* Animations */
.animate-fadeIn {
  animation: fadeIn 0.4s ease-out;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(10px); }
  to { opacity: 1; transform: translateY(0); }
}

/* Header */
.page-header {
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  padding-bottom: 1rem;
}

.header-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.header-actions {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.section-title {
  font-size: 1.75rem;
  font-weight: 700;
  background: linear-gradient(135deg, #a855f7 0%, #3b82f6 100%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
  margin-bottom: 0.25rem;
}

.page-desc {
  font-size: 0.875rem;
  color: #94a3b8;
}

.spinner {
  display: inline-block;
  width: 1rem;
  height: 1rem;
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-radius: 50%;
  border-top-color: #fff;
  animation: spin 1s ease-in-out infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* Stats Grid */
.stats-grid {
  display: grid;
  gap: 0.85rem;
}

.stats-grid.five-cols {
  grid-template-columns: repeat(5, 1fr);
}

@media (max-width: 900px) {
  .stats-grid.five-cols {
    grid-template-columns: repeat(3, 1fr);
  }
}

@media (max-width: 600px) {
  .stats-grid.five-cols {
    grid-template-columns: repeat(2, 1fr);
  }
}

.stat-card {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.85rem 1rem;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 12px;
  cursor: pointer;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  backdrop-filter: blur(10px);
}

.stat-card:hover {
  background: rgba(255, 255, 255, 0.06);
  border-color: rgba(255, 255, 255, 0.15);
  transform: translateY(-2px);
}

.stat-card.active.l0-stat {
  border-color: rgba(59, 130, 246, 0.6);
  box-shadow: 0 0 15px rgba(59, 130, 246, 0.2);
  background: rgba(59, 130, 246, 0.08);
}

.stat-card.active.l0-5-stat {
  border-color: rgba(52, 211, 153, 0.6);
  box-shadow: 0 0 15px rgba(52, 211, 153, 0.2);
  background: rgba(52, 211, 153, 0.08);
}

.stat-card.active.facts-stat {
  border-color: rgba(168, 85, 247, 0.6);
  box-shadow: 0 0 15px rgba(168, 85, 247, 0.2);
  background: rgba(168, 85, 247, 0.08);
}

.stat-card.active.events-stat {
  border-color: rgba(236, 72, 153, 0.6);
  box-shadow: 0 0 15px rgba(236, 72, 153, 0.2);
  background: rgba(236, 72, 153, 0.08);
}

.stat-card.active.vectors-stat {
  border-color: rgba(245, 158, 11, 0.6);
  box-shadow: 0 0 15px rgba(245, 158, 11, 0.2);
  background: rgba(245, 158, 11, 0.08);
}

.stat-icon {
  font-size: 1.5rem;
}

.stat-info h3 {
  font-size: 1.25rem;
  font-weight: 700;
  margin: 0;
  line-height: 1;
}

.stat-info p {
  font-size: 0.7rem;
  color: #94a3b8;
  margin: 0.25rem 0 0 0;
  white-space: nowrap;
}

/* Tabs Navigation */
.tab-nav {
  display: flex;
  gap: 0.4rem;
  background: rgba(255, 255, 255, 0.02);
  padding: 0.35rem;
  border-radius: 10px;
  border: 1px solid rgba(255, 255, 255, 0.04);
}

.tab-btn {
  flex: 1;
  background: transparent;
  border: none;
  color: #94a3b8;
  padding: 0.65rem 0.85rem;
  font-size: 0.8rem;
  font-weight: 600;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s ease;
  white-space: nowrap;
}

.tab-btn:hover {
  color: #fff;
  background: rgba(255, 255, 255, 0.04);
}

.tab-btn.active {
  color: #fff;
  background: rgba(255, 255, 255, 0.08);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

/* Tab contents base styling */
.tab-content {
  display: flex;
  flex-direction: column;
}


/* Filtering & Searches */
.filter-bar {
  margin-bottom: 1.25rem;
}

.search-box {
  display: flex;
  align-items: center;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 8px;
  padding: 0.5rem 0.875rem;
  transition: all 0.3s ease;
}

.search-box:focus-within {
  border-color: rgba(168, 85, 247, 0.5);
  box-shadow: 0 0 10px rgba(168, 85, 247, 0.1);
  background: rgba(255, 255, 255, 0.05);
}

.search-icon {
  margin-right: 0.5rem;
  font-size: 1rem;
}

.search-input {
  background: transparent;
  border: none;
  outline: none;
  color: #fff;
  width: 100%;
  font-size: 0.875rem;
}

/* Empty States */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 4rem 2rem;
  background: rgba(255, 255, 255, 0.01);
  border: 1px dashed rgba(255, 255, 255, 0.08);
  border-radius: 12px;
  text-align: center;
  color: #64748b;
}

.empty-icon {
  font-size: 3rem;
  margin-bottom: 1rem;
}

/* Grid Layouts for Facts and Vectors */
.facts-grid, .vectors-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 1.25rem;
}

.card {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 12px;
  padding: 1.25rem;
  transition: all 0.3s ease;
  backdrop-filter: blur(5px);
}

.card:hover {
  transform: translateY(-2px);
  border-color: rgba(255, 255, 255, 0.12);
  box-shadow: 0 8px 20px rgba(0, 0, 0, 0.15);
}

/* Fact Cards */
.fact-card {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.fact-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.fact-category {
  font-size: 0.7rem;
  font-weight: 700;
  text-transform: uppercase;
  padding: 0.2rem 0.5rem;
  border-radius: 6px;
  letter-spacing: 0.05em;
}

.fact-category.has-cat {
  background: rgba(168, 85, 247, 0.15);
  color: #c084fc;
  border: 1px solid rgba(168, 85, 247, 0.25);
}

.fact-category.no-cat {
  background: rgba(255, 255, 255, 0.05);
  color: #94a3b8;
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.btn-delete {
  background: transparent;
  border: none;
  color: #ef4444;
  cursor: pointer;
  padding: 0.25rem;
  border-radius: 6px;
  transition: all 0.2s ease;
  opacity: 0.6;
}

.btn-delete:hover {
  opacity: 1;
  background: rgba(239, 68, 68, 0.1);
}

.fact-key {
  font-size: 0.95rem;
  font-weight: 600;
  color: #f1f5f9;
  margin: 0 0 0.5rem 0;
}

.fact-value {
  font-size: 0.85rem;
  color: #cbd5e1;
  line-height: 1.5;
  margin: 0;
  word-break: break-word;
}

.fact-footer {
  margin-top: auto;
  border-top: 1px solid rgba(255, 255, 255, 0.05);
  padding-top: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  font-size: 0.75rem;
}

.strength-meter {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.meter-label {
  display: flex;
  justify-content: space-between;
  color: #64748b;
}

.strength-value {
  font-weight: 600;
}

.meter-bar-bg {
  height: 4px;
  background: rgba(255, 255, 255, 0.05);
  border-radius: 2px;
  overflow: hidden;
}

.meter-bar-fill {
  height: 100%;
  border-radius: 2px;
  transition: width 0.5s ease;
}

.importance-badge {
  display: flex;
  justify-content: space-between;
  color: #64748b;
}

.importance-stars {
  color: #f59e0b;
}

.fact-meta {
  display: flex;
  justify-content: space-between;
  color: #475569;
  font-size: 0.7rem;
}

/* Event Timeline */
.event-timeline {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  position: relative;
  padding-left: 1.5rem;
  margin-left: 0.5rem;
}

.event-timeline::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 2px;
  background: rgba(255, 255, 255, 0.05);
}

.timeline-item {
  position: relative;
}

.timeline-badge {
  position: absolute;
  left: -1.75rem;
  top: 1.25rem;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #475569;
  border: 2px solid #0f172a;
  transition: all 0.3s ease;
}

.timeline-item.consolidated .timeline-badge {
  background: #3b82f6;
  box-shadow: 0 0 8px rgba(59, 130, 246, 0.5);
}

.timeline-card {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.event-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.75rem;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  padding-bottom: 0.5rem;
}

.event-domain {
  font-weight: 700;
  color: #3b82f6;
}

.event-time {
  color: #64748b;
}

.event-content {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.msg-bubble {
  padding: 0.75rem 1rem;
  border-radius: 8px;
  font-size: 0.85rem;
  line-height: 1.5;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.user-bubble {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.04);
}

.ai-bubble {
  background: rgba(59, 130, 246, 0.03);
  border: 1px solid rgba(59, 130, 246, 0.08);
}

.bubble-sender {
  font-size: 0.7rem;
  font-weight: 700;
  text-transform: uppercase;
  color: #64748b;
  letter-spacing: 0.05em;
}

.msg-bubble p {
  margin: 0;
  color: #cbd5e1;
}

/* Dual-Perspective Insights */
.event-insights {
  background: rgba(0, 0, 0, 0.15);
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.03);
  padding: 0.75rem 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.insight-title {
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  color: #a855f7;
  letter-spacing: 0.05em;
  margin: 0;
}

.insight-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;
}

@media (max-width: 768px) {
  .insight-row {
    grid-template-columns: 1fr;
  }
}

.insight-col {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  font-size: 0.75rem;
}

.insight-col strong {
  color: #94a3b8;
}

.insight-tag-list, .insight-metrics {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
}

.tag {
  font-size: 0.7rem;
  padding: 0.15rem 0.4rem;
  border-radius: 4px;
  font-weight: 500;
}

.tag-phi {
  background: rgba(16, 185, 129, 0.1);
  color: #34d399;
  border: 1px solid rgba(16, 185, 129, 0.2);
}

.tag-ent {
  background: rgba(245, 158, 11, 0.1);
  color: #fb923c;
  border: 1px solid rgba(245, 158, 11, 0.2);
}

.tag-psi-s {
  background: rgba(239, 68, 68, 0.1);
  color: #f87171;
  border: 1px solid rgba(239, 68, 68, 0.2);
}

.tag-psi-i {
  background: rgba(59, 130, 246, 0.1);
  color: #60a5fa;
  border: 1px solid rgba(59, 130, 246, 0.2);
}

.tag-psi-r {
  background: rgba(236, 72, 153, 0.1);
  color: #f472b6;
  border: 1px solid rgba(236, 72, 153, 0.2);
}

.no-insights {
  color: #475569;
  font-style: italic;
}

.insight-tags {
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-top: 1px solid rgba(255, 255, 255, 0.04);
  padding-top: 0.5rem;
}

.keyword-tag {
  font-size: 0.7rem;
  color: #475569;
}

.status-badge.consolidated {
  background: rgba(16, 185, 129, 0.15);
  color: #34d399;
}

.status-badge.pending {
  background: rgba(245, 158, 11, 0.15);
  color: #fbbf24;
}

.status-badge.dlq {
  background: rgba(239, 68, 68, 0.15);
  color: #f87171;
}

/* Vector Cards */
.vector-card {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.vector-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.75rem;
}

.vector-type {
  font-weight: 700;
  color: #10b981;
  background: rgba(16, 185, 129, 0.1);
  padding: 0.2rem 0.4rem;
  border-radius: 4px;
}

.vector-domain {
  color: #64748b;
}

.vector-content {
  font-size: 0.85rem;
  line-height: 1.5;
  color: #cbd5e1;
  margin: 0;
  font-style: italic;
}

.vector-footer {
  margin-top: auto;
  border-top: 1px solid rgba(255, 255, 255, 0.05);
  padding-top: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  font-size: 0.75rem;
}

.vector-keywords {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
}

.kw-pill {
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.06);
  color: #94a3b8;
  font-size: 0.7rem;
  padding: 0.15rem 0.4rem;
  border-radius: 4px;
}

.vector-meta {
  display: flex;
  justify-content: space-between;
  color: #475569;
  font-size: 0.7rem;
}

.vector-pointers {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  color: #64748b;
  font-size: 0.7rem;
}

.pointer-link {
  color: #3b82f6;
  text-decoration: underline;
  cursor: pointer;
  transition: color 0.2s ease;
}

.pointer-link:hover {
  color: #60a5fa;
}

/* L0 Working Memory Styles */
.l0-timeline {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  max-height: 60vh;
  overflow-y: auto;
  padding-right: 0.5rem;
}

.l0-message-card {
  padding: 1rem;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.04);
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.l0-message-card:hover {
  transform: translateY(-2px);
  background: rgba(255, 255, 255, 0.04);
}

.l0-message-card.user {
  border-left: 4px solid #3b82f6;
  background: rgba(59, 130, 246, 0.02);
}

.l0-message-card.assistant {
  border-left: 4px solid #a855f7;
  background: rgba(168, 85, 247, 0.02);
}

.l0-message-card.system {
  border-left: 4px solid #64748b;
  background: rgba(100, 116, 139, 0.02);
}

.msg-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.5rem;
}

.msg-role-badge {
  font-size: 0.65rem;
  font-weight: 700;
  padding: 0.2rem 0.5rem;
  border-radius: 4px;
  letter-spacing: 0.05em;
}

.msg-role-badge.user {
  background: rgba(59, 130, 246, 0.15);
  color: #60a5fa;
}

.msg-role-badge.assistant {
  background: rgba(168, 85, 247, 0.15);
  color: #c084fc;
}

.msg-role-badge.system {
  background: rgba(100, 116, 139, 0.15);
  color: #94a3b8;
}

.msg-time {
  font-size: 0.7rem;
  color: #475569;
}

.msg-content {
  font-size: 0.85rem;
  line-height: 1.5;
  color: #cbd5e1;
  white-space: pre-wrap;
}

/* L0.5 Session State Styles */
.session-state-container {
  display: flex;
  flex-direction: column;
  background: rgba(0, 0, 0, 0.2) !important;
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 12px;
  overflow: hidden;
}

.session-state-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.75rem 1.25rem;
  background: rgba(255, 255, 255, 0.02);
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.session-state-header .file-name {
  font-family: 'Fira Code', monospace;
  font-size: 0.85rem;
  color: #38bdf8;
  font-weight: 600;
}

.session-state-header .file-status {
  font-size: 0.7rem;
  color: #34d399;
  background: rgba(52, 211, 153, 0.1);
  padding: 0.15rem 0.4rem;
  border-radius: 4px;
}

.session-state-body {
  padding: 1.25rem;
  max-height: 60vh;
  overflow-y: auto;
}

.markdown-code {
  font-family: 'Fira Code', 'Courier New', Courier, monospace;
  font-size: 0.85rem;
  line-height: 1.6;
  color: #a7f3d0;
  margin: 0;
  white-space: pre-wrap;
}

/* Button & Badge Utilities */
.btn {
  background: linear-gradient(135deg, #a855f7 0%, #3b82f6 100%);
  border: none;
  color: white;
  border-radius: 6px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.3s ease;
}

.btn:hover:not(:disabled) {
  opacity: 0.9;
  transform: translateY(-1px);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: #e2e8f0;
}

.btn-secondary:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.12);
}

.btn-sm {
  padding: 0.4rem 0.8rem;
  font-size: 0.75rem;
}
</style>
