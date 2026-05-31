<script setup lang="ts">
/**
 * SkillsView.vue — Skill Readiness Manager
 * ===========================================
 * Displays all LIVA skills with toggle controls to enable/disable.
 * Only enabled skills are available for LIVA to use.
 * Uses the existing dashboard design system (Frosted Acrylic).
 */
import { computed, ref, onActivated, onDeactivated } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";

interface Skill {
  name: string;
  description: string;
  category: string;
  isCoreSkill: boolean;
  status: "active" | "disabled" | "error";
  enabled: boolean;
  errorMsg?: string | null;
}

const gateway = useGateway();
const { t } = useI18n();
const searchQuery = ref("");
const filterMode = ref<"all" | "enabled" | "disabled">("all");

const skills = computed<Skill[]>(() => {
  if (!gateway.isConnected.value) return [];
  const list = gateway.skillsList.value || [];
  return list.map((s: any) => ({
    name: s.name,
    description: s.description || s.summary || "No description",
    category: s.category || (s.isCoreSkill ? "core" : "extension"),
    isCoreSkill: s.isCoreSkill || false,
    status: s.status || (s.enabled === false ? "disabled" : "active"),
    enabled: s.enabled !== false,
    errorMsg: s.errorMsg || null,
  }));
});

// Filter + search
const filteredSkills = computed(() => {
  let result = skills.value;

  // Filter mode
  if (filterMode.value === "enabled") result = result.filter(s => s.enabled);
  if (filterMode.value === "disabled") result = result.filter(s => !s.enabled);

  // Search
  if (searchQuery.value.trim()) {
    const q = searchQuery.value.toLowerCase();
    result = result.filter(s =>
      s.name.toLowerCase().includes(q) ||
      s.description.toLowerCase().includes(q) ||
      s.category.toLowerCase().includes(q)
    );
  }

  return result;
});

// Group by category
const categories = computed(() => {
  const cats = [...new Set(filteredSkills.value.map(s => s.category))];
  // Sort: core first, then alphabetical
  return cats.sort((a, b) => {
    if (a === "core") return -1;
    if (b === "core") return 1;
    return a.localeCompare(b);
  });
});
const skillsByCategory = (cat: string) => filteredSkills.value.filter(s => s.category === cat);

// Stats
const totalSkills = computed(() => skills.value.length);
const enabledCount = computed(() => skills.value.filter(s => s.enabled).length);
const disabledCount = computed(() => skills.value.filter(s => !s.enabled).length);
const errorCount = computed(() => skills.value.filter(s => s.status === "error").length);

const checkingSkill = ref<string | null>(null);
const checkResults = ref<Record<string, { success: boolean; message: string; details: string; time: number }>>({});

const onSkillCheckResult = (payload: any) => {
  if (payload && payload.name) {
    checkResults.value[payload.name] = {
      success: payload.success,
      message: payload.message,
      details: payload.details,
      time: Date.now()
    };
    if (checkingSkill.value === payload.name) {
      checkingSkill.value = null;
    }
  }
};

onActivated(() => {
  gateway.sendMsg('get_skills_list');
  gateway.onSkillCheckResult(onSkillCheckResult);
});

onDeactivated(() => {
  gateway.offSkillCheckResult();
});

const checkSkill = (name: string) => {
  checkingSkill.value = name;
  delete checkResults.value[name];
  gateway.sendMsg("test_skill", { name });
};

// Toggle skill
const toggleSkill = (name: string, currentEnabled: boolean) => {
  gateway.sendMsg("toggle_skill", { name, enabled: !currentEnabled });
  gateway.sendMsg('get_skills_list');
};

// Bulk actions
const enableAll = () => gateway.sendMsg("toggle_all_skills", { enabled: true });
const disableAll = () => gateway.sendMsg("toggle_all_skills", { enabled: false });

// Icon mapping by category
const categoryIcons: Record<string, string> = {
  core: "🧩",
  web: "🌐",
  social: "💬",
  personal: "👤",
  data: "📊",
  devops: "🔧",
  docs: "📄",
  agentic: "🤖",
  system: "⚙️",
  extension: "🔌",
};

const statusIcon = (skill: Skill): string => {
  if (skill.status === "error") return "🔴";
  if (!skill.enabled) return "⏸️";
  return "🟢";
};

const categoryLabel = (cat: string): string => {
  const labels: Record<string, string> = {
    core: t('sk_cat_core'),
    web: t('sk_cat_web'),
    social: t('sk_cat_social'),
    personal: t('sk_cat_personal'),
    data: t('sk_cat_data'),
    devops: t('sk_cat_devops'),
    docs: t('sk_cat_docs'),
    agentic: t('sk_cat_agentic'),
    system: t('sk_cat_system'),
    extension: t('sk_cat_extension'),
  };
  return labels[cat] || cat.toUpperCase();
};
</script>

<template>
  <div class="skills-view animate-fadeIn">
    <!-- Header -->
    <div class="page-header">
      <div class="header-row">
        <div>
          <h1 class="section-title">⚡ {{ t('sk_title') }}</h1>
          <p class="page-desc">{{ t('sk_desc').replace('{total}', totalSkills.toString()) }}</p>
        </div>
        <div class="header-actions">
          <button class="btn btn-secondary btn-sm" @click="enableAll" :title="t('sk_enable_all')">
            {{ t('sk_enable_all') }}
          </button>
          <button class="btn btn-danger btn-sm" @click="disableAll" :title="t('sk_disable_all')">
            {{ t('sk_disable_all') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Stats Bar -->
    <div class="stats-bar">
      <div class="stat-chip stat-active" @click="filterMode = filterMode === 'enabled' ? 'all' : 'enabled'" :class="{ selected: filterMode === 'enabled' }">
        <span class="stat-dot dot-active"></span>
        <span>{{ enabledCount }} {{ t('sk_active') }}</span>
      </div>
      <div class="stat-chip stat-disabled" @click="filterMode = filterMode === 'disabled' ? 'all' : 'disabled'" :class="{ selected: filterMode === 'disabled' }">
        <span class="stat-dot dot-disabled"></span>
        <span>{{ disabledCount }} {{ t('sk_disabled') }}</span>
      </div>
      <div v-if="errorCount > 0" class="stat-chip stat-error">
        <span class="stat-dot dot-error"></span>
        <span>{{ errorCount }} {{ t('sk_error') }}</span>
      </div>
      <div class="search-box">
        <span class="search-icon">🔍</span>
        <input
          v-model="searchQuery"
          class="input search-input"
          :placeholder="t('sk_search_ph')"
        />
      </div>
    </div>

    <!-- Skill Categories -->
    <div class="skills-container">
      <div v-for="category in categories" :key="category" class="skill-category animate-fadeIn">
        <h2 class="category-title">
          <span class="category-icon">{{ categoryIcons[category] || '📦' }}</span>
          {{ categoryLabel(category) }}
          <span class="category-count">{{ skillsByCategory(category).length }}</span>
        </h2>
        <div class="skill-grid">
          <div
            v-for="(skill, index) in skillsByCategory(category)"
            :key="`${skill.name}-${index}`"
            class="card skill-card"
            :class="{ 'skill-disabled': !skill.enabled, 'skill-error': skill.status === 'error' }"
          >
            <div class="skill-main">
              <div class="skill-status">{{ statusIcon(skill) }}</div>
              <div class="skill-info">
                <h3 class="skill-name">{{ skill.name }}</h3>
                <p class="skill-desc">{{ skill.description }}</p>
                <p v-if="skill.errorMsg" class="skill-error-msg">⚠️ {{ skill.errorMsg }}</p>
                
                <!-- Detailed Test Result -->
                <div v-if="checkResults[skill.name]" class="skill-test-result" :class="{ 'test-ok': checkResults[skill.name].success, 'test-fail': !checkResults[skill.name].success }">
                  <div class="test-header">
                    <span class="test-icon">{{ checkResults[skill.name].success ? '🟢' : '🔴' }}</span>
                    <span class="test-message">{{ checkResults[skill.name].message }}</span>
                  </div>
                  <p class="test-details" v-if="checkResults[skill.name].details">{{ checkResults[skill.name].details }}</p>
                </div>
              </div>
            </div>
            <div class="skill-actions">
              <button 
                class="btn-check-skill" 
                @click.stop="checkSkill(skill.name)" 
                :disabled="!skill.enabled || checkingSkill === skill.name"
                title="Kiểm tra chi tiết kĩ năng"
              >
                <span v-if="checkingSkill === skill.name" class="check-spinner"></span>
                <span v-else>🔍</span>
              </button>
              <div
                class="toggle"
                :class="{ active: skill.enabled }"
                @click="toggleSkill(skill.name, skill.enabled)"
                :title="skill.enabled ? t('sk_toggle_on') : t('sk_toggle_off')"
              ></div>
            </div>
          </div>
        </div>
      </div>

      <!-- Empty state -->
      <div v-if="filteredSkills.length === 0" class="empty-state">
        <span class="empty-icon">🔍</span>
        <p>{{ t('sk_empty') }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.skills-view { padding: var(--space-lg); overflow-y: auto; height: 100%; }
.page-header { margin-bottom: var(--space-md); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }

.header-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-md);
}

.header-actions {
  display: flex;
  gap: var(--space-sm);
  flex-shrink: 0;
}

.btn-sm {
  padding: 6px 12px;
  font-size: 12px;
}

/* Stats Bar */
.stats-bar {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  margin-bottom: var(--space-lg);
  flex-wrap: wrap;
}

.stat-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border-radius: 20px;
  font-size: 12px;
  font-weight: 600;
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  cursor: pointer;
  transition: all var(--transition-fast);
  user-select: none;
}
.stat-chip:hover { border-color: var(--text-muted); }
.stat-chip.selected {
  border-color: var(--accent-start);
  box-shadow: 0 0 0 2px rgba(124, 58, 237, 0.15);
}

.stat-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.dot-active { background: var(--color-success); }
.dot-disabled { background: var(--text-muted); }
.dot-error { background: var(--color-danger); }

.search-box {
  position: relative;
  flex: 1;
  min-width: 180px;
  max-width: 300px;
  margin-left: auto;
}
.search-icon {
  position: absolute;
  left: 10px;
  top: 50%;
  transform: translateY(-50%);
  font-size: 13px;
  pointer-events: none;
}
.search-input {
  padding-left: 32px !important;
  height: 34px;
  font-size: 12px;
}

/* Skill Categories */
.skills-container { padding-bottom: var(--space-xl); }

.skill-category { margin-bottom: var(--space-lg); }
.category-title {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  font-size: 13px;
  font-weight: 700;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 1px;
  margin-bottom: var(--space-sm);
  padding-bottom: var(--space-sm);
  border-bottom: 1px solid var(--border-default);
}
.category-icon { font-size: 16px; }
.category-count {
  margin-left: auto;
  background: var(--bg-hover);
  padding: 1px 8px;
  border-radius: 10px;
  font-size: 11px;
  color: var(--text-muted);
}

.skill-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
  gap: var(--space-sm);
}

.skill-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  transition: all var(--transition-fast);
}

.skill-card.skill-disabled {
  opacity: 0.5;
  background: var(--bg-tertiary);
}
.skill-card.skill-disabled:hover { opacity: 0.75; }

.skill-card.skill-error {
  border-color: rgba(248, 81, 73, 0.3);
}

.skill-main {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  flex: 1;
  min-width: 0;
}

.skill-status { font-size: 16px; flex-shrink: 0; }
.skill-info { flex: 1; min-width: 0; }
.skill-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
.skill-desc {
  font-size: 11px;
  color: var(--text-secondary);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.skill-error-msg {
  font-size: 10px;
  color: var(--color-danger);
  margin-top: 3px;
}

.skill-actions {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  flex-shrink: 0;
  margin-left: var(--space-md);
}

.btn-check-skill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  cursor: pointer;
  font-size: 11px;
  color: var(--text-secondary);
  transition: all var(--transition-fast);
  flex-shrink: 0;
  outline: none;
}
.btn-check-skill:hover:not(:disabled) {
  background: var(--bg-hover);
  border-color: var(--accent-start);
  color: var(--text-primary);
  transform: scale(1.05);
}
.btn-check-skill:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.check-spinner {
  width: 12px;
  height: 12px;
  border: 1.5px solid var(--border-default);
  border-top-color: var(--accent-start);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.skill-test-result {
  margin-top: 8px;
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 11px;
  border: 1px solid transparent;
  animation: fadeIn 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  max-width: 100%;
}
.skill-test-result.test-ok {
  background: rgba(16, 185, 129, 0.06);
  border-color: rgba(16, 185, 129, 0.15);
}
.skill-test-result.test-fail {
  background: rgba(239, 68, 68, 0.06);
  border-color: rgba(239, 68, 68, 0.15);
}
.test-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
}
.test-icon {
  font-size: 10px;
  flex-shrink: 0;
}
.test-message {
  color: var(--text-primary);
}
.test-details {
  margin-top: 4px;
  color: var(--text-secondary);
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  word-break: break-all;
  white-space: pre-wrap;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* Empty state */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: var(--space-xl) 0;
  color: var(--text-muted);
  font-size: 14px;
}
.empty-icon {
  font-size: 40px;
  margin-bottom: var(--space-md);
  opacity: 0.3;
}

/* Responsive */
@media screen and (max-width: 768px) {
  .header-row { flex-direction: column; }
  .header-actions { width: 100%; }
  .skill-grid { grid-template-columns: 1fr; }
  .stats-bar { flex-direction: column; align-items: stretch; }
  .search-box { max-width: none; margin-left: 0; }
}
</style>
