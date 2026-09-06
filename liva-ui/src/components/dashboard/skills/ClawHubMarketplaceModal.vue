<script setup lang="ts">
/**
 * ClawHubMarketplaceModal.vue — ClawHub Skills Discovery & 1-Click Installer
 * ===========================================================================
 * Search and install verified skills from the ClawHub community ecosystem.
 */
import { ref, computed } from "vue";
import { useGateway } from "../../../composables/useGateway";
import { logger } from "../../../utils/logger";

const emit = defineEmits<{
  (e: "close"): void;
  (e: "installed", skillId: string): void;
}>();

const gateway = useGateway();
const searchQuery = ref("");
const installingId = ref<string | null>(null);
const installedIds = ref<Set<string>>(new Set());

interface HubSkill {
  id: string;
  name: string;
  category: string;
  description: string;
  author: string;
  stars: number;
  downloads: number;
  verified: boolean;
  repoUrl: string;
}

const hubSkills: HubSkill[] = [
  {
    id: "clawhub-weather-radar",
    name: "Weather & Radar Pro",
    category: "web",
    description: "High-resolution satellite weather forecasts and real-time precipitation radar mapping.",
    author: "MeteoClaw",
    stars: 342,
    downloads: 12400,
    verified: true,
    repoUrl: "https://hub.openclaw.ai/skills/clawhub-weather-radar",
  },
  {
    id: "clawhub-github-triage",
    name: "GitHub PR & Issue Triager",
    category: "devops",
    description: "Automated triage, conflict detection, semantic labeling, and changelog drafting for GitHub repos.",
    author: "GitOpsHub",
    stars: 520,
    downloads: 18900,
    verified: true,
    repoUrl: "https://hub.openclaw.ai/skills/clawhub-github-triage",
  },
  {
    id: "clawhub-notion-syncer",
    name: "Notion Knowledge Syncer",
    category: "personal",
    description: "Bidirectional sync between LIVA episodic memory facts and your Notion database workspace.",
    author: "ProductivityAI",
    stars: 289,
    downloads: 9400,
    verified: true,
    repoUrl: "https://hub.openclaw.ai/skills/clawhub-notion-syncer",
  },
  {
    id: "clawhub-crypto-ticker",
    name: "Crypto & Stock Realtime Ticker",
    category: "data",
    description: "Real-time cryptocurrency orderbook telemetry, DEX volume analysis, and candlestick analytics.",
    author: "FinTechClaw",
    stars: 415,
    downloads: 15300,
    verified: true,
    repoUrl: "https://hub.openclaw.ai/skills/clawhub-crypto-ticker",
  },
];

const filteredHubSkills = computed(() => {
  if (!searchQuery.value.trim()) return hubSkills;
  const q = searchQuery.value.toLowerCase();
  return hubSkills.filter(
    (s) =>
      s.name.toLowerCase().includes(q) ||
      s.description.toLowerCase().includes(q) ||
      s.category.toLowerCase().includes(q)
  );
});

const handleInstall = async (skill: HubSkill) => {
  installingId.value = skill.id;
  try {
    await gateway.installSkillFromHub(skill.id, skill.repoUrl);
    installedIds.value.add(skill.id);
    emit("installed", skill.id);
  } catch (err) {
    logger.error("[ClawHubMarketplaceModal]", "Installation failed:", err);
  } finally {
    installingId.value = null;
  }
};
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <div class="modal-card animate-scaleUp">
      <!-- Modal Header -->
      <div class="modal-header">
        <div class="header-title">
          <span class="icon">🦞</span>
          <div>
            <h3>ClawHub Skill Marketplace</h3>
            <span class="subtitle">Discover, preview and install verified community skills</span>
          </div>
        </div>
        <button class="btn-close" @click="emit('close')">✕</button>
      </div>

      <!-- Search Bar -->
      <div class="modal-search">
        <span class="search-icon">🔍</span>
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search verified skills (weather, github, notion, crypto)..."
        />
      </div>

      <!-- Modal Body -->
      <div class="modal-body">
        <div class="skills-grid">
          <div v-for="skill in filteredHubSkills" :key="skill.id" class="hub-card">
            <div class="hub-card-header">
              <div class="hub-title-box">
                <h4>{{ skill.name }}</h4>
                <span class="author-tag">by {{ skill.author }}</span>
              </div>
              <span v-if="skill.verified" class="verified-badge" title="Verified by ClawHub Security Audit">
                ✓ Verified
              </span>
            </div>

            <p class="hub-desc">{{ skill.description }}</p>

            <div class="hub-card-footer">
              <div class="stats-row">
                <span>⭐ {{ skill.stars }}</span>
                <span>⬇️ {{ skill.downloads.toLocaleString() }}</span>
              </div>
              <button
                class="btn btn-sm"
                :class="installedIds.has(skill.id) ? 'btn-success' : 'btn-primary'"
                :disabled="installingId === skill.id || installedIds.has(skill.id)"
                @click="handleInstall(skill)"
              >
                <span v-if="installingId === skill.id" class="spinner"></span>
                <span v-else-if="installedIds.has(skill.id)">✓ Installed</span>
                <span v-else>Install Skill</span>
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="modal-footer">
        <button class="btn btn-secondary" @click="emit('close')">Close</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(5px);
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
}

.modal-card {
  width: 720px;
  max-width: 90vw;
  max-height: 85vh;
  background: #0f121d;
  border: 1px solid var(--border-default, #262a3d);
  border-radius: 12px;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.5);
  display: flex;
  flex-direction: column;
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.header-title {
  display: flex;
  align-items: center;
  gap: 10px;
}

.header-title .icon {
  font-size: 24px;
}

.header-title h3 {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0;
}

.subtitle {
  font-size: 11px;
  color: var(--text-secondary, #94a3b8);
}

.btn-close {
  background: transparent;
  border: none;
  color: var(--text-secondary, #94a3b8);
  font-size: 16px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
}

.btn-close:hover {
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-primary, #ffffff);
}

.modal-search {
  padding: 14px 20px;
  position: relative;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
}

.modal-search .search-icon {
  position: absolute;
  left: 32px;
  top: 50%;
  transform: translateY(-50%);
  font-size: 13px;
  color: var(--text-muted, #64748b);
}

.modal-search input {
  width: 100%;
  background: #080a10;
  border: 1px solid var(--border-default, #262a3d);
  border-radius: 6px;
  padding: 8px 12px 8px 36px;
  color: #ffffff;
  font-size: 13px;
  box-sizing: border-box;
}

.modal-search input:focus {
  outline: none;
  border-color: #818cf8;
}

.modal-body {
  padding: 20px;
  overflow-y: auto;
  flex: 1;
}

.skills-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 14px;
}

.hub-card {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 8px;
  padding: 14px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 10px;
}

.hub-card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}

.hub-title-box h4 {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0 0 2px 0;
}

.author-tag {
  font-size: 11px;
  color: var(--text-muted, #64748b);
}

.verified-badge {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 4px;
  background: rgba(56, 189, 248, 0.15);
  color: #38bdf8;
}

.hub-desc {
  font-size: 12px;
  color: var(--text-secondary, #94a3b8);
  margin: 0;
  line-height: 1.4;
}

.hub-card-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding-top: 6px;
  border-top: 1px solid rgba(255, 255, 255, 0.04);
}

.stats-row {
  display: flex;
  gap: 10px;
  font-size: 11px;
  color: var(--text-muted, #64748b);
}

.btn-success {
  background: rgba(34, 197, 94, 0.15);
  color: #4ade80;
  border: 1px solid rgba(34, 197, 94, 0.3);
  cursor: default;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  padding: 16px 20px;
  border-top: 1px solid rgba(255, 255, 255, 0.05);
}

@keyframes scaleUp {
  from { transform: scale(0.95); opacity: 0; }
  to { transform: scale(1); opacity: 1; }
}

.animate-scaleUp {
  animation: scaleUp 0.2s cubic-bezier(0.16, 1, 0.3, 1);
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.2);
  border-top-color: #ffffff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
