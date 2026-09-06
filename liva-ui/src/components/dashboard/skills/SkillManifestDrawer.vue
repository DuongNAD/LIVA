<script setup lang="ts">
/**
 * SkillManifestDrawer.vue — Skill Manifest & Markdown Viewer Drawer
 * =================================================================
 * Displays the complete YAML frontmatter, tools, triggers, permissions,
 * and Markdown instructions for a selected skill.
 */
import { ref, onMounted } from "vue";
import { useGateway } from "../../../composables/useGateway";
import { logger } from "../../../utils/logger";
import type { SkillManifestInfo } from "liva-common";

const props = defineProps<{
  skillId: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const gateway = useGateway();
const loading = ref(true);
const manifest = ref<SkillManifestInfo | null>(null);
const activeTab = ref<"parsed" | "raw">("parsed");
const copied = ref(false);

const loadManifest = async () => {
  loading.value = true;
  try {
    const res = await gateway.getSkillManifest(props.skillId);
    manifest.value = res;
  } catch (err) {
    logger.error("[SkillManifestDrawer]", "Failed to load skill manifest:", err);
  } finally {
    loading.value = false;
  }
};

const copyRawContent = () => {
  if (manifest.value?.rawContent) {
    navigator.clipboard?.writeText(manifest.value.rawContent);
    copied.value = true;
    setTimeout(() => {
      copied.value = false;
    }, 2000);
  }
};

onMounted(() => {
  loadManifest();
});
</script>

<template>
  <div class="drawer-overlay" @click.self="emit('close')">
    <div class="manifest-drawer animate-slideLeft">
      <!-- Drawer Header -->
      <div class="drawer-header">
        <div class="header-info">
          <div class="title-row">
            <span class="skill-icon">📜</span>
            <h3 class="drawer-title">{{ manifest?.name || props.skillId }}</h3>
            <span v-if="manifest?.version" class="version-badge">v{{ manifest.version }}</span>
          </div>
          <p class="drawer-desc">{{ manifest?.description || 'Skill Manifest & Instructions' }}</p>
        </div>
        <button class="btn-close" @click="emit('close')" title="Close">✕</button>
      </div>

      <!-- Drawer Tabs -->
      <div class="drawer-tabs">
        <button
          :class="['tab-btn', { active: activeTab === 'parsed' }]"
          @click="activeTab = 'parsed'"
        >
          ⚙️ Manifest & Tools
        </button>
        <button
          :class="['tab-btn', { active: activeTab === 'raw' }]"
          @click="activeTab = 'raw'"
        >
          📄 Raw SKILL.md
        </button>
      </div>

      <!-- Drawer Content -->
      <div class="drawer-body">
        <div v-if="loading" class="drawer-loading">
          <div class="spinner"></div>
          <p>Loading skill manifest...</p>
        </div>

        <div v-else-if="!manifest" class="drawer-error">
          <p>Failed to load manifest for skill "{{ props.skillId }}".</p>
        </div>

        <!-- Parsed View -->
        <div v-else-if="activeTab === 'parsed'" class="parsed-view">
          <!-- Metadata Grid -->
          <div class="meta-section">
            <h4 class="section-heading">Metadata & Runtime</h4>
            <div class="meta-grid">
              <div class="meta-item">
                <span class="meta-label">Author</span>
                <span class="meta-value">{{ manifest.author || 'LIVA Community' }}</span>
              </div>
              <div class="meta-item">
                <span class="meta-label">License</span>
                <span class="meta-value">{{ manifest.license || 'MIT' }}</span>
              </div>
              <div class="meta-item">
                <span class="meta-label">Runtime</span>
                <span class="meta-value runtime-pill">{{ manifest.runtimeType || 'native_rust' }}</span>
              </div>
              <div class="meta-item">
                <span class="meta-label">Content SHA-256</span>
                <span class="meta-value mono-text" :title="manifest.contentHash">
                  {{ manifest.contentHash ? manifest.contentHash.substring(0, 12) + '...' : 'N/A' }}
                </span>
              </div>
            </div>
          </div>

          <!-- Tools Section -->
          <div v-if="manifest.tools && manifest.tools.length > 0" class="tools-section">
            <h4 class="section-heading">Exposed Tools ({{ manifest.tools.length }})</h4>
            <div class="tools-list">
              <div v-for="tool in manifest.tools" :key="tool.name" class="tool-card">
                <div class="tool-header">
                  <span class="tool-name">🔧 {{ tool.name }}</span>
                  <span class="risk-badge" :class="tool.risk_level || 'read_only_safe'">
                    {{ tool.risk_level || 'read_only_safe' }}
                  </span>
                </div>
                <p class="tool-desc">{{ tool.description }}</p>
              </div>
            </div>
          </div>

          <!-- Markdown Instructions -->
          <div class="instructions-section">
            <h4 class="section-heading">Instructions & Guidelines</h4>
            <div class="markdown-preview">
              <pre>{{ manifest.markdownInstructions }}</pre>
            </div>
          </div>
        </div>

        <!-- Raw View -->
        <div v-else class="raw-view">
          <div class="raw-actions">
            <span class="file-path">📁 {{ manifest.dirPath }}/SKILL.md</span>
            <button class="btn btn-sm btn-secondary" @click="copyRawContent">
              {{ copied ? '✓ Copied' : '📋 Copy Raw' }}
            </button>
          </div>
          <pre class="raw-code"><code>{{ manifest.rawContent }}</code></pre>
        </div>
      </div>

      <!-- Drawer Footer -->
      <div class="drawer-footer">
        <button class="btn btn-secondary" @click="emit('close')">Close</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.drawer-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.65);
  backdrop-filter: blur(4px);
  z-index: 999;
  display: flex;
  justify-content: flex-end;
}

.manifest-drawer {
  width: 580px;
  max-width: 90vw;
  height: 100%;
  background: #0f121d;
  border-left: 1px solid var(--border-default, #262a3d);
  display: flex;
  flex-direction: column;
  box-shadow: -10px 0 30px rgba(0, 0, 0, 0.5);
}

.drawer-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding: 20px 24px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.header-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.title-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.skill-icon {
  font-size: 18px;
}

.drawer-title {
  font-size: 16px;
  font-weight: 700;
  color: var(--text-primary, #ffffff);
  margin: 0;
}

.version-badge {
  padding: 2px 8px;
  border-radius: 999px;
  background: rgba(129, 140, 248, 0.15);
  color: #a5b4fc;
  font-size: 11px;
  font-weight: 600;
}

.drawer-desc {
  font-size: 12px;
  color: var(--text-secondary, #94a3b8);
  margin: 0;
}

.btn-close {
  background: transparent;
  border: none;
  color: var(--text-secondary, #94a3b8);
  font-size: 16px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  transition: all 0.15s;
}

.btn-close:hover {
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-primary, #ffffff);
}

.drawer-tabs {
  display: flex;
  padding: 0 24px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  gap: 16px;
}

.tab-btn {
  background: transparent;
  border: none;
  padding: 12px 4px;
  color: var(--text-secondary, #94a3b8);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  position: relative;
  transition: color 0.15s;
}

.tab-btn:hover {
  color: var(--text-primary, #ffffff);
}

.tab-btn.active {
  color: #818cf8;
  font-weight: 600;
}

.tab-btn.active::after {
  content: "";
  position: absolute;
  bottom: -1px;
  left: 0;
  right: 0;
  height: 2px;
  background: #818cf8;
}

.drawer-body {
  flex: 1;
  overflow-y: auto;
  padding: 24px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.drawer-loading, .drawer-error {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 200px;
  gap: 12px;
  color: var(--text-secondary, #94a3b8);
}

.section-heading {
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-secondary, #94a3b8);
  margin: 0 0 10px 0;
}

.meta-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 8px;
  padding: 12px;
}

.meta-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.meta-label {
  font-size: 11px;
  color: var(--text-muted, #64748b);
}

.meta-value {
  font-size: 12px;
  color: var(--text-primary, #ffffff);
  font-weight: 500;
}

.runtime-pill {
  color: #38bdf8;
}

.mono-text {
  font-family: monospace;
  font-size: 11px;
}

.tools-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.tool-card {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 8px;
  padding: 12px;
}

.tool-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 4px;
}

.tool-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
}

.risk-badge {
  font-size: 10px;
  padding: 2px 6px;
  border-radius: 4px;
  text-transform: uppercase;
  font-weight: 600;
}

.risk-badge.read_only_safe {
  background: rgba(34, 197, 94, 0.15);
  color: #4ade80;
}

.tool-desc {
  font-size: 12px;
  color: var(--text-secondary, #94a3b8);
  margin: 0;
}

.markdown-preview pre {
  background: #080a10;
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 8px;
  padding: 14px;
  color: #e2e8f0;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  margin: 0;
}

.raw-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
}

.file-path {
  font-size: 11px;
  color: var(--text-muted, #64748b);
  font-family: monospace;
}

.raw-code {
  background: #080a10;
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 8px;
  padding: 14px;
  color: #cbd5e1;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
  margin: 0;
  max-height: 500px;
  overflow-y: auto;
}

.drawer-footer {
  padding: 16px 24px;
  border-top: 1px solid rgba(255, 255, 255, 0.05);
  display: flex;
  justify-content: flex-end;
}

@keyframes slideLeft {
  from { transform: translateX(100%); }
  to { transform: translateX(0); }
}

.animate-slideLeft {
  animation: slideLeft 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.spinner {
  width: 24px;
  height: 24px;
  border: 2px solid rgba(255, 255, 255, 0.1);
  border-top-color: #818cf8;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
