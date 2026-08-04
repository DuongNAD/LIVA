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
import { ref, computed, watch, onActivated, onDeactivated } from "vue";
import { useGateway } from "../../composables/useGateway";
import type {
  MemoryL0Item,
  MemoryFactItem,
  MemoryEventItem,
  MemoryVectorItem,
} from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";
import MemoryViewerHeader from "./memory/MemoryViewerHeader.vue";
import MemoryViewerStats from "./memory/MemoryViewerStats.vue";
import MemoryViewerTabs from "./memory/MemoryViewerTabs.vue";
import type { MemoryTab } from "./memory/memoryTypes";

const gateway = useGateway();
const { currentLang } = useI18n();

const activeTab = ref<MemoryTab>("facts");

// Search & Filtering State
const l0Query = ref("");
const factQuery = ref("");
const eventQuery = ref("");
const vectorQuery = ref("");

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
  const validList = list.filter((f: MemoryFactItem) => f && typeof f === 'object' && f.key);
  if (!factQuery.value.trim()) return validList;
  const q = factQuery.value.toLowerCase();
  return validList.filter((f: MemoryFactItem) =>
    String(f.key || "").toLowerCase().includes(q) || 
    String(f.value || "").toLowerCase().includes(q) ||
    (f.category && String(f.category).toLowerCase().includes(q))
  );
});

// Số ký ức không mở được (sai khóa) — hiện banner + badge 🔒.
const lockedCount = computed(() => {
  const list = Array.isArray(gateway.memoryData.value?.facts) ? gateway.memoryData.value.facts : [];
  return list.filter((f: MemoryFactItem) => f?.locked).length;
});

const deleteFact = (key: string) => {
  // Không cho xóa ký ức đang khóa (không đọc được thì không biết đang xóa gì).
  // Backend cũng từ chối; đây là guard UI phòng hờ.
  const list = Array.isArray(gateway.memoryData.value?.facts) ? gateway.memoryData.value.facts : [];
  if (list.find((f: MemoryFactItem) => f.key === key)?.locked) return;
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
  const validList = list.filter((e: MemoryEventItem) => e && typeof e === 'object' && e.eventId);
  if (!eventQuery.value.trim()) return validList;
  const q = eventQuery.value.toLowerCase();
  return validList.filter((e: MemoryEventItem) =>
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
  const validList = list.filter((v: MemoryVectorItem) => v && typeof v === 'object' && v.vecId);
  if (!vectorQuery.value.trim()) return validList;
  const q = vectorQuery.value.toLowerCase();
  return validList.filter((v: MemoryVectorItem) =>
    String(v.content || "").toLowerCase().includes(q) || 
    String(v.type || "").toLowerCase().includes(q) ||
    String(v.domain || "").toLowerCase().includes(q) ||
    String(v.category || "").toLowerCase().includes(q)
  );
});

// Statistics
const filteredL0 = computed(() => {
  const list = Array.isArray(gateway.memoryData.value?.l0) ? gateway.memoryData.value.l0 : [];
  const validList = list.filter((m: MemoryL0Item) => m && typeof m === 'object');
  if (!l0Query.value.trim()) return validList;
  const q = l0Query.value.toLowerCase();
  return validList.filter((m: MemoryL0Item) =>
    String(m.content || "").toLowerCase().includes(q) || 
    String(m.role || "").toLowerCase().includes(q)
  );
});

const l0Count = computed(() => {
  const l = gateway.memoryData.value?.l0;
  return Array.isArray(l) ? l.length : 0;
});

/**
 * L0.5 CHƯA CÓ WRITER — lõi trả `"l0_5": ""` đóng cứng trong `get_memory_data`.
 * Nối dây nó là việc của U13, không phải của màn hình này.
 */
const l0_5ChuaNoiDay = computed(
  () => !String(gateway.memoryData.value?.l0_5 || "").length,
);

/**
 * Trả `--` chứ KHÔNG "0 B" khi tầng chưa tồn tại.
 *
 * "0 B" đọc như *tầng này đang trống* — nghe như dữ liệu sẽ tới. Sự thật là
 * *tầng này chưa có ai ghi vào*. Đúng quy ước `sysinfo.rs` vừa dựng: `None` là
 * câu trả lời hợp lệ, và một ô trống nói thật có ích hơn một con số đẹp nói dối.
 */
const l0_5Size = computed(() => {
  const content = String(gateway.memoryData.value?.l0_5 || "");
  if (!content) return "--";
  const bytes = new Blob([content]).size;
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(1)} KB`;
});

/**
 * Mốc số sự kiện lúc mở panel — để chỉ ra thứ VỪA được ghi thêm.
 *
 * Vì sao đếm ở phía UI thay vì đợi lõi bắn sự kiện: `AppState` chưa có kênh
 * phát sự kiện nào, dựng một cái sẽ phải sửa cả hai điểm vào lẫn tầng
 * WebSocket. Mà câu hỏi ở đây chỉ là *"sổ sự kiện có dài thêm không"* — đọc
 * thẳng trạng thái DB thật qua `get_memory_data` trả lời được, không suy đoán.
 */
const mocSuKien = ref<number | null>(null);
const suKienMoi = computed(() =>
  mocSuKien.value === null ? 0 : Math.max(0, eventsCount.value - mocSuKien.value),
);

/**
 * Chốt mốc — nhưng KHÔNG chốt bằng 0 ở lần mở đầu tiên.
 *
 * Bản đầu đặt mốc = `eventsCount` ngay lúc `onActivated`, khi dữ liệu còn chưa
 * về nên luôn bằng 0. Hệ quả: lần mở đầu tiên sẽ khoe "LIVA vừa nhớ thêm N
 * điều" cho **toàn bộ sổ ký ức cũ** — đúng kiểu khoe thành tích không có thật
 * mà cả U18 này sinh ra để chống. Nên chưa có dữ liệu thì để `null`, và nhận
 * giá trị ĐẦU TIÊN về làm mốc.
 */
const datLaiMoc = () => {
  mocSuKien.value =
    gateway.memoryData.value?.events === undefined ? null : eventsCount.value;
};

/**
 * Khởi động lại LIVA ngay từ giao diện.
 *
 * Đây là phần chịu lực của U18. "Nhớ xuyên qua một lần khởi động lại" trước nay
 * chỉ chứng minh được bằng `scripts/e2e-memory.mjs` trong terminal — nơi không
 * người xem nào nhìn. Có nút này thì cả chuỗi *"nói một sự thật → khởi động lại
 * → hỏi lại"* diễn được bằng chuột.
 *
 * Xác nhận hai bước thay vì `window.confirm`: confirm gốc chặn cả webview và
 * trông lạc lõng trong vỏ Tauri, còn kéo thêm plugin dialog chỉ để hỏi một câu
 * thì không đáng.
 */
const dangHoiKhoiDongLai = ref(false);
const dangKhoiDongLai = ref(false);
const loiKhoiDongLai = ref("");
let hetHanHoi: ReturnType<typeof setTimeout> | null = null;

const khoiDongLai = async () => {
  loiKhoiDongLai.value = "";
  if (!dangHoiKhoiDongLai.value) {
    dangHoiKhoiDongLai.value = true;
    if (hetHanHoi) clearTimeout(hetHanHoi);
    hetHanHoi = setTimeout(() => {
      dangHoiKhoiDongLai.value = false;
    }, 5000);
    return;
  }
  dangKhoiDongLai.value = true;
  try {
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch {
    // Báo thẳng thay vì im lặng không làm gì: trong trình duyệt thuần thì
    // không có tiến trình nào để khởi động lại, và người dùng cần biết vì sao
    // bấm mà không thấy gì xảy ra.
    dangKhoiDongLai.value = false;
    dangHoiKhoiDongLai.value = false;
    loiKhoiDongLai.value =
      currentLang.value === "vi-VN"
        ? "Chỉ khởi động lại được trong ứng dụng desktop, không phải trong trình duyệt."
        : "Restart only works inside the desktop app, not in a browser.";
  }
};

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

// Đăng ký SAU khi `eventsCount` đã khai báo: `watch` đánh giá nguồn ngay lập
// tức (khác `computed` vốn lười), nên đặt nó ở trên sẽ dùng biến trước khi có.
watch(eventsCount, (n) => {
  if (mocSuKien.value === null) mocSuKien.value = n;
});

const onMemoryUpdated = () => {
  isConsolidating.value = false;
  refreshMemory();
};

onActivated(() => {
  // Chốt mốc TRƯỚC khi làm mới: mọi sự kiện dữ liệu mới mang về sẽ được tính
  // là "vừa nhớ", đúng nghĩa "thêm kể từ lần bạn nhìn gần nhất".
  datLaiMoc();
  refreshMemory();
  gateway.onMemoryUpdated(onMemoryUpdated);
});

onDeactivated(() => {
  gateway.offMemoryUpdated();
});


</script>

<template>
  <div class="memory-viewer animate-fadeIn">
    <MemoryViewerHeader
      :current-lang="currentLang"
      :total-memories="totalMemories"
      :is-consolidating="isConsolidating"
      :is-refreshing="isRefreshing"
      :is-restarting="dangKhoiDongLai"
      :is-restart-armed="dangHoiKhoiDongLai"
      :recent-memories="suKienMoi"
      :restart-error="loiKhoiDongLai"
      @consolidate="triggerConsolidation"
      @refresh="refreshMemory"
      @restart="khoiDongLai"
    />
    <MemoryViewerStats
      v-model:active-tab="activeTab"
      :current-lang="currentLang"
      :l0-count="l0Count"
      :l05-size="l0_5Size"
      :l05-not-wired="l0_5ChuaNoiDay"
      :facts-count="factsCount"
      :events-count="eventsCount"
      :vectors-count="vectorsCount"
    />
    <MemoryViewerTabs v-model:active-tab="activeTab" :current-lang="currentLang" />



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
          <pre v-if="!l0_5ChuaNoiDay" class="markdown-code">{{ gateway.memoryData.value?.l0_5 }}</pre>
          <!-- KHÔNG vẽ "(Empty)": tầng này không trống, nó CHƯA TỒN TẠI. Lõi
               trả `"l0_5": ""` đóng cứng và chưa có gì ghi vào. Vẽ một file
               rỗng ở đây là đúng kiểu đèn xanh giả mà `sysinfo.rs` vừa gỡ. -->
          <div v-else class="chua-noi-day-note">
            <p>
              <strong>{{ currentLang === 'vi-VN' ? 'Tầng này chưa có gì ghi vào.' : 'Nothing writes to this tier yet.' }}</strong>
            </p>
            <p>
              {{ currentLang === 'vi-VN'
                ? 'Lõi trả về một chuỗi rỗng cố định cho L0.5 — đây là thiết kế đã có nhưng chưa nối dây, không phải một phiên làm việc trống. Việc nối dây thuộc mục U13 (consolidation ngữ nghĩa L2 → L3).'
                : 'The core returns a hard-coded empty string for L0.5 — this tier is designed but not wired, not an empty session. Wiring it is tracked as U13.' }}
            </p>
            <p class="chua-noi-day-doi-chieu">
              {{ currentLang === 'vi-VN'
                ? 'Các tầng CÓ dữ liệu thật: L2 (sự kiện) và vector — xem hai tab bên cạnh.'
                : 'Tiers with real data: L2 events and vectors — see the tabs next to this one.' }}
            </p>
          </div>
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

      <div v-if="lockedCount > 0" class="locked-banner">
        🔒
        {{ currentLang === 'vi-VN'
          ? `${lockedCount} ký ức không mở được — sai LIVA_ENCRYPTION_KEY. Dữ liệu gốc vẫn còn nguyên; đặt đúng khóa để đọc lại.`
          : `${lockedCount} memories can't be decrypted — wrong LIVA_ENCRYPTION_KEY. Original data is intact; set the correct key to read them.` }}
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
          :class="{ 'fact-locked': fact.locked }"
        >
          <div class="fact-header">
            <span class="fact-category" :class="fact.category ? 'has-cat' : 'no-cat'">
              {{ fact.category || (currentLang === 'vi-VN' ? 'Chung' : 'General') }}
            </span>
            <button
              class="btn-delete"
              :disabled="fact.locked"
              @click="deleteFact(fact.key)"
              :title="fact.locked
                ? (currentLang === 'vi-VN' ? 'Không thể xóa ký ức đang khóa' : 'Cannot delete a locked memory')
                : (currentLang === 'vi-VN' ? 'Xóa sự thật này' : 'Delete this fact')"
            >
              🗑️
            </button>
          </div>

          <div class="fact-body">
            <h3 class="fact-key">
              <span v-if="fact.locked" class="lock-badge">🔒</span>
              {{ fact.key }}
            </h3>
            <p v-if="fact.locked" class="fact-value fact-value-locked">
              {{ currentLang === 'vi-VN'
                ? 'Không mở được — sai khóa mã hóa (dữ liệu gốc còn nguyên)'
                : 'Locked — wrong encryption key (original data intact)' }}
            </p>
            <p v-else class="fact-value">{{ fact.value }}</p>
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
  color: var(--text-primary);
  background-color: var(--bg-primary);
  transition: background-color var(--transition-normal), color var(--transition-normal);
}

/* Animations */
.animate-fadeIn {
  animation: fadeIn 0.4s cubic-bezier(0.4, 0, 0.2, 1);
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(10px); }
  to { opacity: 1; transform: translateY(0); }
}

/* Header */
.page-header {
  border-bottom: 1px solid var(--border-default);
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
  color: var(--text-secondary);
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
:global([data-theme="light"]) .spinner {
  border: 2px solid rgba(0, 0, 0, 0.1);
  border-top-color: var(--accent-start);
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
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all var(--transition-normal);
  box-shadow: var(--shadow-card);
}

.stat-card:hover {
  background: var(--bg-hover);
  border-color: var(--text-muted);
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

.stat-card.active.l0-stat {
  border-color: rgba(59, 130, 246, 0.6);
  box-shadow: 0 0 15px rgba(59, 130, 246, 0.2);
  background: rgba(59, 130, 246, 0.08);
}
:global([data-theme="light"]) .stat-card.active.l0-stat {
  border-color: #3b82f6;
  background: rgba(59, 130, 246, 0.06);
  box-shadow: 0 4px 12px rgba(59, 130, 246, 0.12);
}

.stat-card.active.l0-5-stat {
  border-color: rgba(52, 211, 153, 0.6);
  box-shadow: 0 0 15px rgba(52, 211, 153, 0.2);
  background: rgba(52, 211, 153, 0.08);
}
:global([data-theme="light"]) .stat-card.active.l0-5-stat {
  border-color: #10b981;
  background: rgba(16, 185, 129, 0.06);
  box-shadow: 0 4px 12px rgba(16, 185, 129, 0.12);
}

.stat-card.active.facts-stat {
  border-color: rgba(168, 85, 247, 0.6);
  box-shadow: 0 0 15px rgba(168, 85, 247, 0.2);
  background: rgba(168, 85, 247, 0.08);
}
:global([data-theme="light"]) .stat-card.active.facts-stat {
  border-color: #a855f7;
  background: rgba(168, 85, 247, 0.06);
  box-shadow: 0 4px 12px rgba(168, 85, 247, 0.12);
}

.stat-card.active.events-stat {
  border-color: rgba(236, 72, 153, 0.6);
  box-shadow: 0 0 15px rgba(236, 72, 153, 0.2);
  background: rgba(236, 72, 153, 0.08);
}
:global([data-theme="light"]) .stat-card.active.events-stat {
  border-color: #ec4899;
  background: rgba(236, 72, 153, 0.06);
  box-shadow: 0 4px 12px rgba(236, 72, 153, 0.12);
}

.stat-card.active.vectors-stat {
  border-color: rgba(245, 158, 11, 0.6);
  box-shadow: 0 0 15px rgba(245, 158, 11, 0.2);
  background: rgba(245, 158, 11, 0.08);
}
:global([data-theme="light"]) .stat-card.active.vectors-stat {
  border-color: #f59e0b;
  background: rgba(245, 158, 11, 0.06);
  box-shadow: 0 4px 12px rgba(245, 158, 11, 0.12);
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
  color: var(--text-secondary);
  margin: 0.25rem 0 0 0;
  white-space: nowrap;
}

/* Tabs Navigation */
.tab-nav {
  display: flex;
  gap: 0.4rem;
  background: var(--bg-tertiary);
  padding: 0.35rem;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-default);
}

.tab-btn {
  flex: 1;
  background: transparent;
  border: none;
  color: var(--text-secondary);
  padding: 0.65rem 0.85rem;
  font-size: 0.8rem;
  font-weight: 600;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--transition-fast);
  white-space: nowrap;
}

.tab-btn:hover {
  color: var(--text-primary);
  background: var(--bg-hover);
}

.tab-btn.active {
  color: var(--text-primary);
  background: var(--bg-secondary);
  box-shadow: var(--shadow-sm);
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
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  padding: 0.5rem 0.875rem;
  transition: all var(--transition-normal);
}

.search-box:focus-within {
  border-color: var(--accent-start);
  box-shadow: 0 0 10px rgba(168, 85, 247, 0.15);
}

.search-icon {
  margin-right: 0.5rem;
  font-size: 1rem;
}

.search-input {
  background: transparent;
  border: none;
  outline: none;
  color: var(--text-primary);
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
  background: var(--bg-tertiary);
  border: 1px dashed var(--border-default);
  border-radius: var(--radius-md);
  text-align: center;
  color: var(--text-secondary);
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
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  transition: all var(--transition-normal);
  box-shadow: var(--shadow-card);
}

.card:hover {
  transform: translateY(-2px);
  border-color: var(--accent-start);
  box-shadow: var(--shadow-md);
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
  background: rgba(168, 85, 247, 0.12);
  color: #a855f7;
  border: 1px solid rgba(168, 85, 247, 0.2);
}
:global([data-theme="light"]) .fact-category.has-cat {
  background: rgba(124, 58, 237, 0.08);
  color: #7c3aed;
  border: 1px solid rgba(124, 58, 237, 0.15);
}

.fact-category.no-cat {
  background: var(--bg-hover);
  color: var(--text-secondary);
  border: 1px solid var(--border-default);
}

.btn-delete {
  background: transparent;
  border: none;
  color: var(--color-danger);
  cursor: pointer;
  padding: 0.25rem;
  border-radius: 6px;
  transition: all var(--transition-fast);
  opacity: 0.6;
}

.btn-delete:hover {
  opacity: 1;
  background: rgba(239, 68, 68, 0.1);
}

.btn-delete:disabled {
  opacity: 0.25;
  cursor: not-allowed;
}

.btn-delete:disabled:hover {
  background: transparent;
}

/* Banner + thẻ cho ký ức không mở được (sai khóa mã hóa) */
.locked-banner {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  margin-bottom: 1rem;
  border-radius: 8px;
  background: rgba(245, 158, 11, 0.12);
  border: 1px solid rgba(245, 158, 11, 0.4);
  color: var(--text-primary);
  font-size: 0.85rem;
  line-height: 1.4;
}

.fact-card.fact-locked {
  border-style: dashed;
  border-color: rgba(245, 158, 11, 0.5);
  opacity: 0.85;
}

.lock-badge {
  margin-right: 0.35rem;
}

.fact-value-locked {
  font-size: 0.85rem;
  color: var(--color-warning, #F59E0B);
  font-style: italic;
  line-height: 1.5;
  margin: 0;
}

.fact-key {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 0.5rem 0;
}

.fact-value {
  font-size: 0.85rem;
  color: var(--text-secondary);
  line-height: 1.5;
  margin: 0;
  word-break: break-word;
}

.fact-footer {
  margin-top: auto;
  border-top: 1px solid var(--border-default);
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
  color: var(--text-secondary);
}

.strength-value {
  font-weight: 600;
}

.meter-bar-bg {
  height: 4px;
  background: var(--border-default);
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
  color: var(--text-secondary);
}

.importance-stars {
  color: #f59e0b;
}

.fact-meta {
  display: flex;
  justify-content: space-between;
  color: var(--text-muted);
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
  background: var(--border-default);
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
  background: var(--text-muted);
  border: 2px solid var(--bg-primary);
  transition: all var(--transition-normal);
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
  border-bottom: 1px solid var(--border-default);
  padding-bottom: 0.5rem;
}

.event-domain {
  font-weight: 700;
  color: #3b82f6;
}

.event-time {
  color: var(--text-muted);
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
  background: var(--bg-tertiary);
  border: 1px solid var(--border-default);
}

.ai-bubble {
  background: rgba(124, 58, 237, 0.03);
  border: 1px solid rgba(124, 58, 237, 0.08);
}

.bubble-sender {
  font-size: 0.7rem;
  font-weight: 700;
  text-transform: uppercase;
  color: var(--text-secondary);
  letter-spacing: 0.05em;
}

.msg-bubble p {
  margin: 0;
  color: var(--text-primary);
}

/* Dual-Perspective Insights */
.event-insights {
  background: var(--bg-tertiary);
  border-radius: var(--radius-md);
  border: 1px solid var(--border-default);
  padding: 0.75rem 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.insight-title {
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  color: var(--accent-start);
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
  color: var(--text-primary);
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
  color: #059669;
  border: 1px solid rgba(16, 185, 129, 0.2);
}
:global([data-theme="light"]) .tag-phi {
  background: rgba(5, 150, 105, 0.08);
  color: #047857;
}

.tag-ent {
  background: rgba(245, 158, 11, 0.1);
  color: #d97706;
  border: 1px solid rgba(245, 158, 11, 0.2);
}

.tag-psi-s {
  background: rgba(239, 68, 68, 0.1);
  color: #dc2626;
  border: 1px solid rgba(239, 68, 68, 0.2);
}

.tag-psi-i {
  background: rgba(59, 130, 246, 0.1);
  color: #2563eb;
  border: 1px solid rgba(59, 130, 246, 0.2);
}

.tag-psi-r {
  background: rgba(236, 72, 153, 0.1);
  color: #db2777;
  border: 1px solid rgba(236, 72, 153, 0.2);
}

.no-insights {
  color: var(--text-muted);
  font-style: italic;
}

.insight-tags {
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-top: 1px solid var(--border-default);
  padding-top: 0.5rem;
}

.keyword-tag {
  font-size: 0.7rem;
  color: var(--text-muted);
}

.status-badge.consolidated {
  background: rgba(16, 185, 129, 0.15);
  color: #059669;
}

.status-badge.pending {
  background: rgba(245, 158, 11, 0.15);
  color: #d97706;
}

.status-badge.dlq {
  background: rgba(239, 68, 68, 0.15);
  color: #dc2626;
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
  color: #059669;
  background: rgba(16, 185, 129, 0.1);
  padding: 0.2rem 0.4rem;
  border-radius: 4px;
}

.vector-domain {
  color: var(--text-secondary);
}

.vector-content {
  font-size: 0.85rem;
  line-height: 1.5;
  color: var(--text-primary);
  margin: 0;
  font-style: italic;
}

.vector-footer {
  margin-top: auto;
  border-top: 1px solid var(--border-default);
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
  background: var(--bg-tertiary);
  border: 1px solid var(--border-default);
  color: var(--text-secondary);
  font-size: 0.7rem;
  padding: 0.15rem 0.4rem;
  border-radius: var(--radius-sm);
}

.vector-meta {
  display: flex;
  justify-content: space-between;
  color: var(--text-muted);
  font-size: 0.7rem;
}

.vector-pointers {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  color: var(--text-secondary);
  font-size: 0.7rem;
}

.pointer-link {
  color: #3b82f6;
  text-decoration: underline;
  cursor: pointer;
  transition: color var(--transition-fast);
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
  border-radius: var(--radius-md);
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  transition: all var(--transition-normal);
}

.l0-message-card:hover {
  transform: translateY(-2px);
  background: var(--bg-hover);
  box-shadow: var(--shadow-sm);
}

.l0-message-card.user {
  border-left: 4px solid #3b82f6;
  background: rgba(59, 130, 246, 0.02);
}

.l0-message-card.assistant {
  border-left: 4px solid var(--accent-start);
  background: rgba(168, 85, 247, 0.02);
}

.l0-message-card.system {
  border-left: 4px solid var(--text-muted);
  background: var(--bg-tertiary);
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
  color: #3b82f6;
}

.msg-role-badge.assistant {
  background: rgba(168, 85, 247, 0.15);
  color: var(--accent-start);
}

.msg-role-badge.system {
  background: var(--bg-hover);
  color: var(--text-secondary);
}

.msg-time {
  font-size: 0.7rem;
  color: var(--text-muted);
}

.msg-content {
  font-size: 0.85rem;
  line-height: 1.5;
  color: var(--text-primary);
  white-space: pre-wrap;
}

/* L0.5 Session State Styles */
.session-state-container {
  display: flex;
  flex-direction: column;
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  overflow: hidden;
  box-shadow: var(--shadow-card);
}
:global([data-theme="light"]) .session-state-container {
  background: var(--bg-secondary) !important;
}

.session-state-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.75rem 1.25rem;
  background: var(--bg-tertiary);
  border-bottom: 1px solid var(--border-default);
}

.session-state-header .file-name {
  font-family: 'Fira Code', monospace;
  font-size: 0.85rem;
  color: var(--text-link);
  font-weight: 600;
}

.session-state-header .file-status {
  font-size: 0.7rem;
  color: #059669;
  background: rgba(16, 185, 129, 0.1);
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
  color: #34d399;
  margin: 0;
  white-space: pre-wrap;
}
:global([data-theme="light"]) .markdown-code {
  color: #065f46;
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
  background: var(--bg-elevated);
  border: 1px solid var(--border-default);
  color: var(--text-primary);
}

.btn-secondary:hover:not(:disabled) {
  background: var(--bg-hover);
}

.btn-sm {
  padding: 0.4rem 0.8rem;
  font-size: 0.75rem;
}

/* ── U18: trí nhớ nhìn thấy được ─────────────────────────────────────────── */

/* Bước xác nhận đổi màu để "bấm nhầm" không thành "khởi động lại nhầm". */
.restart-btn.arming {
  border-color: var(--warning, #e0a800);
  color: var(--warning, #e0a800);
}

.vua-nho-banner {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 8px;
  margin: 0 0 var(--space-md);
  padding: 10px 14px;
  border-radius: 12px;
  border: 1px solid var(--border-default);
  background: var(--bg-tertiary);
  font-size: 13px;
}

.vua-nho-banner.loi {
  border-color: var(--warning, #e0a800);
}

.vua-nho-hint {
  color: var(--text-secondary);
  font-size: 12px;
}

/* Tầng chưa nối dây: làm mờ để nó KHÔNG trông ngang hàng với tầng có dữ liệu
   thật. Vẫn giữ trong lưới vì nó là thiết kế đã có, chỉ chưa tồn tại. */
.stat-card.chua-noi-day {
  opacity: 0.55;
}

.chua-co-badge {
  margin-left: 6px;
  padding: 1px 6px;
  border-radius: 999px;
  border: 1px solid var(--border-default);
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-secondary);
}

.chua-noi-day-note {
  padding: 16px 18px;
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1.6;
}

.chua-noi-day-note strong {
  color: var(--text-primary);
}

.chua-noi-day-doi-chieu {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--border-default);
}
</style>
