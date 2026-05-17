<script setup lang="ts">
/**
 * AvatarGallery.vue — 3D/2D Model Manager
 * ==========================================
 * Đồng bộ SSOT: Gateway liva-config.json + quét thư mục models/ + hot-swap Widget
 */
import { ref, onMounted, onActivated, watch } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";
import {
  type AvatarModelInfo,
  type EnginePreference,
  applyActiveFlags,
  buildAvatarConfigPatch,
  normalizeEngineMode,
} from "../../utils/avatarSync";
import { detectOptimalEngine } from "../../utils/HardwareDetector";
import { logger } from "../../utils/logger";

const electronAPI = (globalThis as {
  electronAPI?: {
    setIgnoreMouse?: (v: boolean) => void;
    changeAvatarConfig?: (c: unknown) => void;
    importAvatarModel?: (p: unknown) => Promise<{ success: boolean; filename?: string; type?: string; format?: string; error?: string }>;
    importAvatarModelFolder?: (p: unknown) => Promise<{ success: boolean; folderPath?: string; filename?: string; type?: string; format?: string; error?: string }>;
    selectAndImportAvatarFolder?: () => Promise<{ success: boolean; folderPath?: string; filename?: string; type?: string; format?: string; error?: string }>;
    deleteAvatarModel?: (p: unknown) => Promise<{ success: boolean; error?: string }>;
  };
}).electronAPI;
const gateway = useGateway();
const { t } = useI18n();

const engineMode = ref<EnginePreference>('auto');
const activeTab = ref<'3d' | '2d'>('3d');
const models3D = ref<AvatarModelInfo[]>([]);
const models2D = ref<AvatarModelInfo[]>([]);
const isLoading = ref(false);
const loadProgress = ref(0);
const uploadError = ref('');
const selectFolderError = ref('');
const MAX_FILE_SIZE = 50 * 1024 * 1024;

const mapGatewayModels = (raw: Record<string, unknown>[], type: '2d' | '3d'): AvatarModelInfo[] =>
  raw.map((m) => ({
    name: String(m.name ?? m.filename ?? 'Model'),
    filename: String(m.filename ?? ''),
    size: String(m.size ?? ''),
    isActive: Boolean(m.isActive),
    type,
    format: (m.format as AvatarModelInfo['format']) ?? (type === '2d' ? 'live2d' : 'fbx'),
  }));

const resolveModelLabel = (model: AvatarModelInfo) => {
  const parts = model.filename.split('/').filter(Boolean);
  return parts.length > 1 ? parts[parts.length - 2] : model.name;
};

const syncFromGateway = () => {
  const cfg = gateway.configData.value;
  engineMode.value = normalizeEngineMode(cfg?.avatar ? (cfg.avatar as Record<string, unknown>).engineMode : 'auto');

  if (gateway.avatarModels3D.value.length) {
    models3D.value = applyActiveFlags(mapGatewayModels(gateway.avatarModels3D.value, '3d'), cfg);
  }
  const defaultPioModel: AvatarModelInfo = {
    name: 'Bé Phù Thủy Pio (Mặc định)',
    filename: 'pio/index.json',
    size: 'CDN',
    isActive: false,
    type: '2d',
    format: 'live2d',
  };

  let current2D = gateway.avatarModels2D.value.length 
    ? mapGatewayModels(gateway.avatarModels2D.value, '2d') 
    : [];

  if (!current2D.some(m => m.filename === 'pio/index.json')) {
    current2D.unshift(defaultPioModel);
  }
  
  models2D.value = applyActiveFlags(current2D, cfg);

  const activeType = (cfg?.avatar as Record<string, unknown> | undefined)?.activeType;
  if (activeType === '2d') activeTab.value = '2d';
  else if (activeType === '3d') activeTab.value = '3d';
};

const refreshAvatarModels = () => {
  isLoading.value = true;
  loadProgress.value = 30;
  gateway.sendMsg('get_avatar_models');
  gateway.sendMsg('get_config');
  loadProgress.value = 100;
  setTimeout(() => { isLoading.value = false; loadProgress.value = 0; }, 400);
};

const currentModels = () => (activeTab.value === '3d' ? models3D.value : models2D.value);



const pushConfig = (patch: Record<string, unknown>) => {
  gateway.updateConfig(patch);
  const merged = {
    ...gateway.configData.value,
    avatar: { ...(gateway.configData.value.avatar as object || {}), ...(patch.avatar as object || {}) },
    ui: { ...(gateway.configData.value.ui as object || {}), ...(patch.ui as object || {}) },
  };
  electronAPI?.changeAvatarConfig?.(merged);
};

const activateModel = (model: AvatarModelInfo) => {
  models3D.value = models3D.value.map((m) => ({ ...m, isActive: m.filename === model.filename }));
  models2D.value = models2D.value.map((m) => ({ ...m, isActive: m.filename === model.filename }));
  electronAPI?.setIgnoreMouse?.(false);
  pushConfig(buildAvatarConfigPatch(model, engineMode.value));
};

const setEngineMode = (mode: EnginePreference) => {
  engineMode.value = mode;
  
  const optimalMode = mode === 'auto' ? detectOptimalEngine('auto') : mode;
  const nextTab = optimalMode === '2D' ? '2d' : '3d';
  activeTab.value = nextTab;
  
  const patch: Record<string, unknown> = {
    avatar: { engineMode: mode, activeType: nextTab },
    ui: { avatarMode: mode }
  };

  pushConfig(patch);
};


const triggerFolderPick = async () => {
  uploadError.value = '';
  selectFolderError.value = '';

  if (!electronAPI?.selectAndImportAvatarFolder) {
    selectFolderError.value = 'Chưa có API import folder.';
    return;
  }

  isLoading.value = true;
  loadProgress.value = 20;
  try {
    const result = await electronAPI.selectAndImportAvatarFolder();
    if (!result.success) {
      if (result.error !== 'canceled') {
        selectFolderError.value = result.error || 'Không thể import folder model.';
      }
      return;
    }
    loadProgress.value = 80;
    gateway.sendMsg('get_avatar_models');
    gateway.sendMsg('get_config');
    selectFolderError.value = '';
  } catch (e) {
    logger.error('[AvatarGallery]', 'Folder import failed', e instanceof Error ? e.message : String(e));
    selectFolderError.value = 'Không thể chọn folder model.';
  } finally {
    isLoading.value = false;
    loadProgress.value = 0;
  }
};

const deleteModel = async (model: AvatarModelInfo) => {
  if (model.isActive) {
    uploadError.value = 'Không thể xóa model đang được sử dụng!';
    return;
  }
  if (!electronAPI?.deleteAvatarModel) {
    uploadError.value = 'Chưa có API xóa model.';
    return;
  }
  if (!confirm(`Bạn có chắc chắn muốn xóa model ${resolveModelLabel(model)}?`)) return;

  isLoading.value = true;
  uploadError.value = '';
  try {
    const result = await electronAPI.deleteAvatarModel({ filename: model.filename });
    if (!result.success) {
      uploadError.value = result.error || 'Xóa model thất bại.';
      return;
    }
    gateway.sendMsg('get_avatar_models');
  } catch (e) {
    logger.error('[AvatarGallery]', 'Delete failed', e instanceof Error ? e.message : String(e));
    uploadError.value = 'Không thể xóa model.';
  } finally {
    isLoading.value = false;
  }
};

watch(() => gateway.avatarModels3D.value, () => syncFromGateway(), { deep: true });
watch(() => gateway.avatarModels2D.value, () => syncFromGateway(), { deep: true });
watch(() => gateway.configData.value, () => syncFromGateway(), { deep: true });

// VRoid Hub link
const openVRoidHub = () => {
  window.open('https://hub.vroid.com/', '_blank');
};

// Mixamo link (FBX source)
const openMixamo = () => {
  window.open('https://www.mixamo.com/', '_blank');
};

// Format badge class
const getFormatBadgeClass = (format?: string) => {
  if (format === 'vrm') return 'badge badge-vrm';
  if (format === 'fbx') return 'badge badge-fbx';
  return 'badge badge-info';
};

onMounted(() => refreshAvatarModels());
onActivated(() => syncFromGateway());
</script>

<template>
  <div class="avatar-gallery animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">🎭 {{ t('av_title') }}</h1>
      <p class="page-desc">{{ t('av_desc') }}</p>
    </div>

    <!-- Engine Mode Selector -->
    <div class="card engine-selector">
      <div class="engine-header">
        <span class="section-subtitle">{{ t('av_engine_mode') }}</span>
        <span class="badge badge-info">{{ engineMode.toLowerCase() === 'auto' ? t('av_badge_auto') : engineMode }}</span>
      </div>
      <div class="engine-buttons">
        <button
          v-for="mode in (['auto', '2D', '3D'] as EnginePreference[])"
          :key="mode"
          :class="['btn', engineMode === mode ? 'btn-primary' : 'btn-secondary']"
          @click="setEngineMode(mode)"
        >
          {{ mode === 'auto' ? t('av_engine_auto') : mode === '2D' ? t('av_engine_2d') : t('av_engine_3d') }}
        </button>
      </div>
      <p class="form-help" v-if="engineMode === 'auto'">
        {{ t('av_engine_auto_desc') }}
      </p>
    </div>




    <!-- Loading Bar -->
    <div v-if="isLoading" class="loading-bar-container">
      <div class="loading-bar" :style="{ width: loadProgress + '%' }"></div>
      <span class="loading-text">{{ t('av_loading', { progress: loadProgress }) }}</span>
    </div>

    <!-- Upload Error -->
    <div v-if="uploadError" class="upload-error">
      ⚠️ {{ uploadError }}
    </div>
    <div v-if="selectFolderError" class="upload-error">
      ⚠️ {{ selectFolderError }}
    </div>



    <div class="active-model-banner" v-if="(gateway.configData.value?.ui as Record<string, unknown> | undefined)?.activeModel">
      <span class="active-model-label">Active model</span>
      <code class="active-model-code">{{ String((gateway.configData.value?.ui as Record<string, unknown> | undefined)?.activeModel?.filename ?? '') }}</code>
    </div>

    <!-- Model Grid -->
    <div class="model-grid">
      <div
        v-for="model in currentModels()"
        :key="model.filename"
        :class="['model-card card', { 'model-active': model.isActive }]"
      >
        <!-- Preview -->
        <div class="model-preview">
          <div class="model-preview-placeholder">
            {{ model.type === '3d' ? (model.format === 'fbx' ? '📦' : '🧊') : '🎨' }}
          </div>
          <div v-if="model.isActive" class="model-active-badge badge badge-success">{{ t('av_active') }}</div>
          <!-- Format Badge -->
          <div v-if="model.format" :class="['model-format-badge', getFormatBadgeClass(model.format)]">
            {{ (model.format || '').toUpperCase() }}
          </div>
        </div>

        <!-- Info -->
        <div class="model-info">
          <h3 class="model-name">{{ resolveModelLabel(model) }}</h3>
          <p class="model-file">{{ model.filename }}</p>
          <p class="model-size">{{ model.size }}</p>
        </div>

        <!-- Actions -->
        <div class="model-actions">
          <button
            v-if="!model.isActive"
            class="btn btn-primary btn-sm"
            @click="activateModel(model)"
          >
            {{ t('av_select') }}
          </button>
          <button v-else class="btn btn-secondary btn-sm" disabled>
            {{ t('av_selected') }}
          </button>
          <button
            v-if="!model.isActive && model.filename !== 'default_avatar/tripo_convert_648e4371-4299-44d8-94d8-e6a63e0e07a3.fbx' && model.filename !== 'pio/index.json'"
            class="btn btn-danger btn-sm"
            @click.stop="deleteModel(model)"
            title="Xóa model"
          >
            🗑️
          </button>
        </div>
      </div>

      <!-- Add Model Card -->
      <div class="model-card card add-card" @click="triggerFolderPick">
        <div class="add-icon">+</div>
        <p class="add-text">Thêm Folder Model</p>
        <p class="add-hint">Chọn thư mục chứa .fbx/.vrm</p>
      </div>
    </div>

    <!-- FBX Notice -->
    <div v-if="activeTab === '3d'" class="fbx-notice card">
      <span class="notice-icon">💡</span>
      <div class="notice-content" v-html="t('av_notice_fbx')"></div>
    </div>

    <!-- Quick Links -->
    <div class="quick-links">
      <button class="btn btn-secondary" @click="openVRoidHub">
        {{ t('av_link_vroid_hub') }}
      </button>
      <button class="btn btn-secondary" @click="openMixamo">
        {{ t('av_link_mixamo') }}
      </button>
      <button class="btn btn-secondary">
        {{ t('av_link_vroid_studio') }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.avatar-gallery {
  padding: var(--space-lg);
  overflow-y: auto;
  height: 100%;
}

.page-header {
  margin-bottom: var(--space-lg);
}

.page-desc {
  color: var(--text-secondary);
  font-size: 13px;
  margin-top: 4px;
}

/* Engine Selector */
.engine-selector {
  margin-bottom: var(--space-lg);
}

.engine-header {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  margin-bottom: var(--space-sm);
}

.engine-buttons {
  display: flex;
  gap: var(--space-sm);
}

/* Tab Bar */
.tab-bar {
  display: flex;
  gap: 4px;
  margin-bottom: var(--space-md);
  background: var(--bg-tertiary);
  padding: 4px;
  border-radius: var(--radius-sm);
  width: fit-content;
  border: 1px solid var(--border-default);
}

.tab-btn {
  padding: 8px 20px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  border-radius: 4px;
  transition: all var(--transition-fast);
}

.tab-btn.active {
  background: var(--bg-secondary);
  color: var(--accent-start);
  font-weight: 600;
  box-shadow: var(--shadow-sm);
}

.tab-btn:hover:not(.active) {
  color: var(--text-primary);
  background: var(--bg-hover);
}

/* Loading Bar */
.loading-bar-container {
  position: relative;
  height: 24px;
  background: var(--bg-tertiary);
  border-radius: var(--radius-sm);
  margin-bottom: var(--space-md);
  overflow: hidden;
}

.loading-bar {
  height: 100%;
  background: var(--accent-gradient);
  border-radius: var(--radius-sm);
  transition: width 0.3s ease;
}

.loading-text {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  font-size: 11px;
  font-weight: 600;
  color: var(--text-primary);
}

/* Upload Error */
.upload-error {
  background: rgba(248, 81, 73, 0.12);
  border: 1px solid rgba(248, 81, 73, 0.3);
  color: var(--color-danger);
  padding: 8px 12px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  margin-bottom: var(--space-md);
}

/* Model Grid */
.active-model-banner {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: var(--space-md);
  padding: 10px 12px;
  border-radius: 12px;
  background: rgba(124, 58, 237, 0.08);
  border: 1px solid rgba(124, 58, 237, 0.18);
}

.active-model-label {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: .8px;
  color: var(--text-muted);
  font-weight: 700;
}

.active-model-code {
  font-size: 12px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.folder-action-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  margin-bottom: var(--space-md);
}

.folder-hint {
  font-size: 12px;
  color: var(--text-secondary);
}

.model-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: var(--space-md);
  margin-bottom: var(--space-lg);
}

.model-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.model-active {
  border-color: var(--color-success) !important;
  box-shadow: 0 0 12px rgba(63, 185, 80, 0.15);
}

.model-preview {
  position: relative;
  height: 140px;
  background: var(--bg-tertiary);
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
}

.model-preview-placeholder {
  font-size: 48px;
  opacity: 0.5;
}

.model-active-badge {
  position: absolute;
  top: 8px;
  right: 8px;
}

.model-format-badge {
  position: absolute;
  bottom: 8px;
  left: 8px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.5px;
}

/* Format-specific badge colors */
.badge-vrm {
  background: rgba(63, 185, 80, 0.15) !important;
  color: #3fb950 !important;
  border: 1px solid rgba(63, 185, 80, 0.3);
}

.badge-fbx {
  background: rgba(255, 166, 43, 0.15) !important;
  color: #ffa62b !important;
  border: 1px solid rgba(255, 166, 43, 0.3);
}

.model-info {
  padding: 0 4px;
  flex: 1;
}

.model-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-file {
  font-size: 11px;
  color: var(--text-muted);
  font-family: monospace;
  word-break: break-all;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.model-size {
  font-size: 11px;
  color: var(--text-secondary);
}

.model-actions {
  padding: 0 4px;
  display: flex;
  gap: 8px;
  margin-top: auto;
}

.btn-sm {
  padding: 6px 12px;
  font-size: 12px;
  flex: 1;
}

.btn-danger {
  background: rgba(248, 81, 73, 0.1) !important;
  color: var(--color-danger) !important;
  border-color: rgba(248, 81, 73, 0.3) !important;
  width: auto;
  flex: 0 0 auto;
}

.btn-danger:hover {
  background: rgba(248, 81, 73, 0.2) !important;
}

/* Add Card */
.add-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 220px;
  border-style: dashed;
  opacity: 0.6;
  transition: opacity var(--transition-fast);
}

.add-card:hover {
  opacity: 1;
  border-color: var(--accent-start);
}

.add-icon {
  font-size: 32px;
  color: var(--text-muted);
  margin-bottom: var(--space-sm);
}

.add-text {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
}

.add-hint {
  font-size: 11px;
  color: var(--text-muted);
}

/* FBX Notice */
.fbx-notice {
  display: flex;
  gap: var(--space-sm);
  align-items: flex-start;
  background: rgba(255, 166, 43, 0.06);
  border-color: rgba(255, 166, 43, 0.2);
  margin-bottom: var(--space-lg);
}

.notice-icon {
  font-size: 18px;
  flex-shrink: 0;
  margin-top: 2px;
}

.notice-content {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.6;
}

.notice-content strong {
  color: #ffa62b;
}

/* Quick Links */
.quick-links {
  display: flex;
  gap: var(--space-sm);
  flex-wrap: wrap;
}
</style>
