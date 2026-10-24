<script setup lang="ts">
/**
 * SkillsView.vue — Scoped Agent Profiles & 3-Tier Tool Governance (Auto / Ask / Block)
 * ====================================================================================
 * Displays LIVA skills grouped by integration with:
 * - Specialized Agent Profile presets (Default, PKM/Vault, OS Operator, Strict HITL)
 * - Per-integration accordion headers showing `Per-user · N exceptions`
 * - Action-kind badges (`🔑` Read vs `✏️` Write)
 * - 3-tier segmented permission controls (`⚡ Auto` | `❓ Ask` | `🚫 Block`)
 */
import { computed, ref, onActivated, onDeactivated } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";

export type SkillPolicy = "auto" | "ask" | "block";
export type SkillActionKind = "read" | "write";

interface Skill {
  name: string;
  description: string;
  category: string;
  isCoreSkill: boolean;
  status: "active" | "disabled" | "error";
  enabled: boolean;
  policy: SkillPolicy;
  defaultPolicy: SkillPolicy;
  isException: boolean;
  actionKind: SkillActionKind;
  errorMsg?: string | null;
}

interface RawSkill {
  name: string;
  description?: string;
  summary?: string;
  short_desc?: string;
  category?: string;
  isCoreSkill?: boolean;
  status?: Skill["status"];
  enabled?: boolean;
  policy?: SkillPolicy;
  default_policy?: SkillPolicy;
  is_exception?: boolean;
  action_kind?: SkillActionKind;
  errorMsg?: string | null;
}

interface AgentProfilePreset {
  id: string;
  icon: string;
  titleKey: string;
  descKey: string;
  policies: Record<string, SkillPolicy>;
}

const READ_TOOLS = new Set(["read_markdown", "search_vault", "get_weather"]);
const DEFAULT_ASK_TOOLS = new Set(["write_markdown"]);

const inferDefaultPolicy = (name: string): SkillPolicy =>
  DEFAULT_ASK_TOOLS.has(name) ? "ask" : "auto";

const inferActionKind = (name: string, raw?: SkillActionKind): SkillActionKind => {
  if (raw === "read" || raw === "write") return raw;
  return READ_TOOLS.has(name) ? "read" : "write";
};

const gateway = useGateway();
const { t, currentLang } = useI18n();
const searchQuery = ref("");
const filterMode = ref<"all" | "enabled" | "disabled" | "auto" | "ask">("all");
const collapsedCategories = ref<Set<string>>(new Set());
const activeProfileId = ref<string>("default");

const agentProfiles: AgentProfilePreset[] = [
  {
    id: "default",
    icon: "🤖",
    titleKey: "sk_agent_default",
    descKey: "sk_agent_default_desc",
    policies: {
      read_markdown: "auto",
      search_vault: "auto",
      write_markdown: "ask",
      control_smarthome: "auto",
      control_volume: "auto",
      control_media: "auto",
      get_weather: "auto",
    },
  },
  {
    id: "pkm_vault",
    icon: "📚",
    titleKey: "sk_agent_pkm",
    descKey: "sk_agent_pkm_desc",
    policies: {
      read_markdown: "auto",
      search_vault: "auto",
      write_markdown: "ask",
      control_smarthome: "block",
      control_volume: "block",
      control_media: "block",
      get_weather: "auto",
    },
  },
  {
    id: "os_operator",
    icon: "🎛️",
    titleKey: "sk_agent_operator",
    descKey: "sk_agent_operator_desc",
    policies: {
      read_markdown: "ask",
      search_vault: "ask",
      write_markdown: "block",
      control_smarthome: "auto",
      control_volume: "auto",
      control_media: "auto",
      get_weather: "auto",
    },
  },
  {
    id: "strict_hitl",
    icon: "🛡️",
    titleKey: "sk_agent_strict",
    descKey: "sk_agent_strict_desc",
    policies: {
      read_markdown: "auto",
      search_vault: "auto",
      write_markdown: "ask",
      control_smarthome: "ask",
      control_volume: "ask",
      control_media: "ask",
      get_weather: "auto",
    },
  },
];

const skills = computed<Skill[]>(() => {
  if (!gateway.isConnected.value) return [];
  const list = gateway.skillsList.value || [];
  return list.map((s: RawSkill) => {
    const defaultPolicy = s.default_policy || inferDefaultPolicy(s.name);
    const enabled = s.enabled !== false && s.policy !== "block";
    const policy: SkillPolicy = s.policy
      ? s.policy
      : enabled
        ? defaultPolicy
        : "block";
    const isException =
      typeof s.is_exception === "boolean"
        ? s.is_exception
        : policy !== defaultPolicy;
    return {
      name: s.name,
      description: s.description || s.short_desc || s.summary || "No description",
      category: s.category || (s.isCoreSkill ? "core" : "extension"),
      isCoreSkill: s.isCoreSkill || false,
      status: s.status || (!enabled ? "disabled" : "active"),
      enabled,
      policy,
      defaultPolicy,
      isException,
      actionKind: inferActionKind(s.name, s.action_kind),
      errorMsg: s.errorMsg || null,
    };
  });
});

// Filter + search
const filteredSkills = computed(() => {
  let result = skills.value;

  if (filterMode.value === "enabled") result = result.filter(s => s.enabled);
  if (filterMode.value === "disabled") result = result.filter(s => !s.enabled);
  if (filterMode.value === "auto") result = result.filter(s => s.policy === "auto");
  if (filterMode.value === "ask") result = result.filter(s => s.policy === "ask");

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
  const order = ["docs", "core", "system", "web"];
  return cats.sort((a, b) => {
    const ia = order.indexOf(a);
    const ib = order.indexOf(b);
    if (ia !== -1 && ib !== -1) return ia - ib;
    if (ia !== -1) return -1;
    if (ib !== -1) return 1;
    return a.localeCompare(b);
  });
});

const skillsByCategory = (cat: string) =>
  filteredSkills.value.filter(s => s.category === cat);

const categoryExceptionsCount = (cat: string): number =>
  skillsByCategory(cat).filter(s => s.isException).length;

const formatExceptionsBadge = (count: number): string => {
  if (count <= 0) return "";
  if (count === 1) return t("sk_exception_one");
  return t("sk_exceptions").replace("{count}", count.toString());
};

const toggleCategoryCollapse = (cat: string) => {
  if (collapsedCategories.value.has(cat)) {
    collapsedCategories.value.delete(cat);
  } else {
    collapsedCategories.value.add(cat);
  }
};

// Stats
const totalSkills = computed(() => skills.value.length);
const enabledCount = computed(() => skills.value.filter(s => s.enabled).length);
const autoCount = computed(() => skills.value.filter(s => s.policy === "auto").length);
const askCount = computed(() => skills.value.filter(s => s.policy === "ask").length);
const disabledCount = computed(() => skills.value.filter(s => !s.enabled).length);
const errorCount = computed(() => skills.value.filter(s => s.status === "error").length);

const checkingSkill = ref<string | null>(null);
const checkingSkills = ref<Set<string>>(new Set());
const isCheckingAll = ref(false);
const checkResults = ref<Record<string, { success: boolean; message: string; details: string; time: number }>>({});

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const onSkillCheckResult = (payload: any) => {
  if (payload && payload.name) {
    checkResults.value[payload.name] = {
      success: payload.success,
      message: payload.message,
      details: payload.details,
      time: Date.now()
    };
    checkingSkills.value.delete(payload.name);
    if (checkingSkill.value === payload.name) {
      checkingSkill.value = null;
    }
  }
};

const onAllSkillsCheckComplete = () => {
  isCheckingAll.value = false;
  checkingSkills.value.clear();
};

onActivated(() => {
  gateway.sendMsg("get_skills_list");
  gateway.onSkillCheckResult(onSkillCheckResult);
  gateway.onAllSkillsCheckComplete(onAllSkillsCheckComplete);
});

onDeactivated(() => {
  gateway.offSkillCheckResult();
  gateway.offAllSkillsCheckComplete();
});

const checkSkill = (name: string) => {
  checkingSkill.value = name;
  checkingSkills.value.add(name);
  delete checkResults.value[name];

  const skill = skills.value.find(s => s.name === name);
  setTimeout(() => {
    checkResults.value[name] = {
      success: skill ? skill.status !== "error" : true,
      message: skill && skill.status === "error" ? "Lỗi cấu hình skill" : "Skill khả dụng & cú pháp hợp lệ",
      details: skill ? `${skill.name} (${skill.category} · ${skill.policy.toUpperCase()})` : name,
      time: Date.now(),
    };
    checkingSkills.value.delete(name);
    if (checkingSkill.value === name) {
      checkingSkill.value = null;
    }
  }, 400);
};

const checkAllSkills = () => {
  if (isCheckingAll.value) return;
  isCheckingAll.value = true;

  filteredSkills.value.forEach(s => {
    if (s.enabled) {
      delete checkResults.value[s.name];
      checkingSkills.value.add(s.name);
    }
  });

  setTimeout(() => {
    filteredSkills.value.forEach(s => {
      if (s.enabled) {
        checkResults.value[s.name] = {
          success: s.status !== "error",
          message: s.status === "error" ? "Lỗi cấu hình skill" : "Skill khả dụng & cú pháp hợp lệ",
          details: `${s.name} (${s.category} · ${s.policy.toUpperCase()})`,
          time: Date.now(),
        };
      }
    });
    isCheckingAll.value = false;
    checkingSkills.value.clear();
  }, 600);
};

// Set 3-tier policy for a single skill
const setSkillPolicy = (name: string, policy: SkillPolicy) => {
  gateway.sendMsg("toggle_skill", {
    name,
    policy,
    enabled: policy !== "block",
  });
  gateway.sendMsg("get_skills_list");
};

// Legacy boolean toggle helper
const toggleSkill = (name: string, currentEnabled: boolean) => {
  setSkillPolicy(name, currentEnabled ? "block" : inferDefaultPolicy(name));
};

// Apply a specialized Agent Profile preset
const applyAgentProfile = (profile: AgentProfilePreset) => {
  activeProfileId.value = profile.id;
  gateway.sendMsg("toggle_all_skills", {
    agentProfile: profile.id,
    policies: profile.policies,
  });
  gateway.sendMsg("get_skills_list");
};

// Bulk actions
const enableAll = () => {
  activeProfileId.value = "default";
  gateway.sendMsg("toggle_all_skills", { enabled: true });
  gateway.sendMsg("get_skills_list");
};

const disableAll = () => {
  gateway.sendMsg("toggle_all_skills", { enabled: false });
  gateway.sendMsg("get_skills_list");
};

const categoryIcons: Record<string, string> = {
  core: "🏠",
  web: "🌤️",
  social: "💬",
  personal: "👤",
  data: "📊",
  devops: "🔧",
  docs: "📓",
  agentic: "🤖",
  system: "🎛️",
  extension: "🔌",
};

const categoryLabel = (cat: string): string => {
  const labels: Record<string, string> = {
    core: `SmartHome · ${t("sk_cat_core")}`,
    web: `Open-Meteo · ${t("sk_cat_web")}`,
    social: t("sk_cat_social"),
    personal: t("sk_cat_personal"),
    data: t("sk_cat_data"),
    devops: t("sk_cat_devops"),
    docs: `Obsidian Vault · ${t("sk_cat_docs")}`,
    agentic: t("sk_cat_agentic"),
    system: `OS Audio & Media · ${t("sk_cat_system")}`,
    extension: t("sk_cat_extension"),
  };
  return labels[cat] || cat.toUpperCase();
};

const toolDisplayName = (name: string): string => {
  const namesVi: Record<string, string> = {
    read_markdown: "Read Markdown note",
    search_vault: "Search Vault notes",
    write_markdown: "Create or overwrite Markdown note",
    control_smarthome: "Control SmartHome device",
    control_volume: "Adjust or mute system volume",
    control_media: "Play / Pause / Skip media track",
    get_weather: "Get current outdoor weather",
  };
  return namesVi[name] || name;
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
          <button class="btn btn-secondary btn-sm" @click="checkAllSkills" :disabled="isCheckingAll">
            <span v-if="isCheckingAll" class="spinner"></span>
            <span v-else>🔍 {{ currentLang === 'vi-VN' ? 'Kiểm tra tất cả' : 'Check All' }}</span>
          </button>
          <button class="btn btn-secondary btn-sm" @click="enableAll" :title="t('sk_enable_all')">
            {{ t('sk_enable_all') }}
          </button>
          <button class="btn btn-danger btn-sm" @click="disableAll" :title="t('sk_disable_all')">
            {{ t('sk_disable_all') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Specialized Agent Profiles Bar -->
    <div class="agent-profiles-panel" data-testid="agent-profiles-panel">
      <div class="profiles-header">
        <span class="profiles-title">🛡️ {{ t('sk_agent_profiles_title') }}</span>
      </div>
      <div class="profiles-grid">
        <button
          v-for="profile in agentProfiles"
          :key="profile.id"
          type="button"
          class="profile-preset-card"
          :class="{ active: activeProfileId === profile.id }"
          :data-testid="`agent-profile-${profile.id}`"
          @click="applyAgentProfile(profile)"
        >
          <div class="profile-preset-top">
            <span class="profile-preset-icon">{{ profile.icon }}</span>
            <span class="profile-preset-name">{{ t(profile.titleKey) }}</span>
          </div>
          <p class="profile-preset-desc">{{ t(profile.descKey) }}</p>
        </button>
      </div>
    </div>

    <!-- Stats Bar -->
    <div class="stats-bar">
      <div
        class="stat-chip stat-active"
        @click="filterMode = filterMode === 'enabled' ? 'all' : 'enabled'"
        :class="{ selected: filterMode === 'enabled' }"
      >
        <span class="stat-dot dot-active"></span>
        <span>{{ enabledCount }} {{ t('sk_active') }}</span>
      </div>
      <div
        class="stat-chip stat-auto"
        @click="filterMode = filterMode === 'auto' ? 'all' : 'auto'"
        :class="{ selected: filterMode === 'auto' }"
      >
        <span class="stat-dot dot-auto"></span>
        <span>⚡ {{ autoCount }} {{ t('sk_policy_auto') }}</span>
      </div>
      <div
        class="stat-chip stat-ask"
        @click="filterMode = filterMode === 'ask' ? 'all' : 'ask'"
        :class="{ selected: filterMode === 'ask' }"
      >
        <span class="stat-dot dot-ask"></span>
        <span>❓ {{ askCount }} {{ t('sk_policy_ask') }}</span>
      </div>
      <div
        class="stat-chip stat-disabled"
        @click="filterMode = filterMode === 'disabled' ? 'all' : 'disabled'"
        :class="{ selected: filterMode === 'disabled' }"
      >
        <span class="stat-dot dot-disabled"></span>
        <span>🚫 {{ disabledCount }} {{ t('sk_policy_block') }}</span>
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

    <!-- Per-Integration Accordion Groups -->
    <div class="skills-container">
      <div
        v-for="category in categories"
        :key="category"
        class="integration-accordion animate-fadeIn"
      >
        <button
          type="button"
          class="accordion-header"
          @click="toggleCategoryCollapse(category)"
        >
          <div class="accordion-title-group">
            <span class="category-icon">{{ categoryIcons[category] || '📦' }}</span>
            <span class="category-name">{{ categoryLabel(category) }}</span>
            <span class="scope-meta">· {{ t('sk_per_user') }}</span>
            <span
              v-if="categoryExceptionsCount(category) > 0"
              class="exceptions-badge"
              data-testid="exceptions-badge"
            >
              · {{ formatExceptionsBadge(categoryExceptionsCount(category)) }}
            </span>
          </div>
          <div class="accordion-right">
            <span class="category-count">{{ skillsByCategory(category).length }}</span>
            <span class="accordion-chevron" :class="{ collapsed: collapsedCategories.has(category) }">⌄</span>
          </div>
        </button>

        <div v-show="!collapsedCategories.has(category)" class="integration-tools-list">
          <div
            v-for="(skill, index) in skillsByCategory(category)"
            :key="`${skill.name}-${index}`"
            class="tool-row"
            :class="{
              'tool-row-blocked': skill.policy === 'block',
              'tool-row-ask': skill.policy === 'ask',
              'tool-row-error': skill.status === 'error'
            }"
          >
            <div class="tool-row-main">
              <span
                class="action-kind-badge"
                :class="skill.actionKind === 'read' ? 'kind-read' : 'kind-write'"
                :title="skill.actionKind === 'read' ? t('sk_action_read') : t('sk_action_write')"
              >
                {{ skill.actionKind === 'read' ? '🔑' : '✏️' }}
              </span>
              <div class="skill-info">
                <div class="skill-title-line">
                  <h3 class="skill-name">{{ toolDisplayName(skill.name) }}</h3>
                  <code class="skill-code-id">{{ skill.name }}</code>
                  <span v-if="skill.isException" class="exception-dot" title="Custom policy override"></span>
                </div>
                <p class="skill-desc">{{ skill.description }}</p>
                <p v-if="skill.errorMsg" class="skill-error-msg">⚠️ {{ skill.errorMsg }}</p>

                <!-- Detailed Test Result -->
                <div
                  v-if="checkResults[skill.name]"
                  class="skill-test-result"
                  :class="{ 'test-ok': checkResults[skill.name].success, 'test-fail': !checkResults[skill.name].success }"
                >
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
                type="button"
                class="btn-check-skill"
                @click.stop="checkSkill(skill.name)"
                :disabled="!skill.enabled || checkingSkill === skill.name || checkingSkills.has(skill.name)"
                title="Kiểm tra chi tiết kĩ năng"
              >
                <span v-if="checkingSkill === skill.name || checkingSkills.has(skill.name)" class="check-spinner"></span>
                <span v-else>🔍</span>
              </button>

              <!-- 3-Tier Segmented Control: Auto | Ask | Block -->
              <div class="policy-segment" role="group" :aria-label="`Permission policy for ${skill.name}`">
                <button
                  type="button"
                  class="policy-btn policy-auto"
                  :class="{ active: skill.policy === 'auto' }"
                  :data-testid="`policy-auto-${skill.name}`"
                  @click="setSkillPolicy(skill.name, 'auto')"
                >
                  <span class="policy-icon">⚡</span>
                  <span>{{ t('sk_policy_auto') }}</span>
                </button>
                <button
                  type="button"
                  class="policy-btn policy-ask"
                  :class="{ active: skill.policy === 'ask' }"
                  :data-testid="`policy-ask-${skill.name}`"
                  @click="setSkillPolicy(skill.name, 'ask')"
                >
                  <span class="policy-icon">❓</span>
                  <span>{{ t('sk_policy_ask') }}</span>
                </button>
                <button
                  type="button"
                  class="policy-btn policy-block"
                  :class="{ active: skill.policy === 'block' }"
                  :data-testid="`policy-block-${skill.name}`"
                  @click="setSkillPolicy(skill.name, 'block')"
                >
                  <span class="policy-icon">🚫</span>
                  <span>{{ t('sk_policy_block') }}</span>
                </button>
              </div>
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

/* Specialized Agent Profiles Bar */
.agent-profiles-panel {
  margin-bottom: var(--space-md);
  padding: 12px 14px;
  border-radius: 12px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
}

.profiles-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}

.profiles-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

.profiles-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
  gap: 8px;
}

.profile-preset-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  text-align: left;
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-default);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.profile-preset-card:hover {
  border-color: var(--text-muted);
  background: var(--bg-hover);
}

.profile-preset-card.active {
  border-color: #10b981;
  background: rgba(16, 185, 129, 0.08);
  box-shadow: 0 0 0 1px rgba(16, 185, 129, 0.25);
}

.profile-preset-top {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
}

.profile-preset-icon {
  font-size: 15px;
}

.profile-preset-name {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
}

.profile-preset-desc {
  font-size: 11px;
  color: var(--text-secondary);
  line-height: 1.35;
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
.dot-auto { background: #10b981; }
.dot-ask { background: #f59e0b; }
.dot-disabled { background: #ef4444; }
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

/* Per-Integration Accordion */
.skills-container { padding-bottom: var(--space-xl); }

.integration-accordion {
  margin-bottom: var(--space-md);
  border-radius: 12px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  overflow: hidden;
}

.accordion-header {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 13px 16px;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border-default);
  cursor: pointer;
  text-align: left;
  transition: background var(--transition-fast);
}

.accordion-header:hover {
  background: var(--bg-hover);
}

.accordion-title-group {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.category-icon { font-size: 17px; }
.category-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.scope-meta {
  font-size: 12.5px;
  color: var(--text-muted);
}

.exceptions-badge {
  font-size: 12.5px;
  font-weight: 600;
  color: #f59e0b;
}

.accordion-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.category-count {
  background: var(--bg-hover);
  padding: 2px 8px;
  border-radius: 10px;
  font-size: 11px;
  color: var(--text-muted);
}

.accordion-chevron {
  font-size: 16px;
  color: var(--text-muted);
  transition: transform 0.2s ease;
}

.accordion-chevron.collapsed {
  transform: rotate(-90deg);
}

.integration-tools-list {
  display: flex;
  flex-direction: column;
}

.tool-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  transition: background var(--transition-fast);
}

.tool-row:last-child {
  border-bottom: none;
}

.tool-row:hover {
  background: rgba(255, 255, 255, 0.02);
}

.tool-row.tool-row-blocked {
  opacity: 0.68;
}

.tool-row.tool-row-error {
  border-left: 3px solid var(--color-danger);
}

.tool-row-main {
  display: flex;
  align-items: center;
  gap: 12px;
  flex: 1;
  min-width: 0;
}

.action-kind-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: 8px;
  font-size: 14px;
  flex-shrink: 0;
  border: 1px solid var(--border-default);
  background: var(--bg-tertiary);
}

.action-kind-badge.kind-read {
  border-color: rgba(245, 158, 11, 0.25);
  background: rgba(245, 158, 11, 0.08);
}

.action-kind-badge.kind-write {
  border-color: rgba(59, 130, 246, 0.25);
  background: rgba(59, 130, 246, 0.08);
}

.skill-info { flex: 1; min-width: 0; }

.skill-title-line {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.skill-name {
  font-size: 13.5px;
  font-weight: 500;
  color: var(--text-primary);
}

.skill-code-id {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--bg-tertiary);
  color: var(--text-muted);
}

.exception-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #f59e0b;
}

.skill-desc {
  font-size: 11.5px;
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
  gap: 10px;
  flex-shrink: 0;
  margin-left: var(--space-md);
}

/* 3-Tier Segmented Pill Control (Auto / Ask / Block) */
.policy-segment {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px;
  border-radius: 9px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-default);
}

.policy-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 10px;
  border-radius: 6px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.15s ease;
}

.policy-btn:hover {
  color: var(--text-primary);
  background: var(--bg-hover);
}

.policy-icon {
  font-size: 11px;
}

.policy-btn.policy-auto.active {
  background: rgba(16, 185, 129, 0.16);
  border-color: rgba(16, 185, 129, 0.45);
  color: #10b981;
}

.policy-btn.policy-ask.active {
  background: rgba(245, 158, 11, 0.18);
  border-color: rgba(245, 158, 11, 0.5);
  color: #f59e0b;
}

.policy-btn.policy-block.active {
  background: rgba(239, 68, 68, 0.16);
  border-color: rgba(239, 68, 68, 0.45);
  color: #ef4444;
}

.btn-check-skill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  background: var(--bg-tertiary);
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

.spinner {
  display: inline-block;
  width: 12px;
  height: 12px;
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-radius: 50%;
  border-top-color: var(--text-primary);
  animation: spin 1s ease-in-out infinite;
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
  .tool-row { flex-direction: column; align-items: flex-start; gap: 10px; }
  .skill-actions { margin-left: 0; width: 100%; justify-content: flex-end; }
  .stats-bar { flex-direction: column; align-items: stretch; }
  .search-box { max-width: none; margin-left: 0; }
}
</style>

