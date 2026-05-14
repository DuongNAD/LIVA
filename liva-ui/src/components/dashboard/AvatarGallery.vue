<script setup lang="ts">
/**
 * AvatarGallery.vue — 3D/2D Model Manager
 * ==========================================
 * - Kho 2D (Live2D models) + Kho 3D (VRM + FBX models)
 * - Engine Mode selector: Auto | Force 2D | Force 3D
 * - Preview + Activate model → notify widget via IPC
 * - FBX support: auto-scale/center, AnimationMixer
 */
import { ref, onMounted } from "vue";

const electronAPI = (globalThis as any).electronAPI;

// Engine mode
type EnginePreference = 'auto' | '2D' | '3D';
const engineMode = ref<EnginePreference>('auto');

// Active tab
const activeTab = ref<'3d' | '2d'>('3d');

// Model lists
interface ModelInfo {
  name: string;
  filename: string;
  size: string;
  isActive: boolean;
  type: '2d' | '3d';
  format?: 'vrm' | 'fbx' | 'live2d';
  thumbnail?: string;
}

const models3D = ref<ModelInfo[]>([
  { name: 'Default Avatar', filename: 'default.vrm', size: '~10 MB', isActive: true, type: '3d', format: 'vrm' },
]);

const models2D = ref<ModelInfo[]>([
  { name: 'Pio (Phù Thủy)', filename: 'pio/index.json', size: '~2 MB', isActive: true, type: '2d', format: 'live2d' },
]);

const currentModels = () => activeTab.value === '3d' ? models3D.value : models2D.value;

// Loading state
const isLoading = ref(false);
const loadProgress = ref(0);
const uploadError = ref('');

// Max file size: 50MB
const MAX_FILE_SIZE = 50 * 1024 * 1024;

// Activate model
const activateModel = (model: ModelInfo) => {
  const list = model.type === '3d' ? models3D : models2D;
  list.value.forEach(m => m.isActive = false);
  model.isActive = true;

  if (electronAPI) {
    electronAPI.setIgnoreMouse(false);
  }

  // TODO: Send update_config to Gateway via WebSocket
};

// Set engine mode
const setEngineMode = (mode: EnginePreference) => {
  engineMode.value = mode;
  // TODO: Send update_config to Gateway
};

// File upload handler
const fileInput = ref<HTMLInputElement | null>(null);
const triggerUpload = () => {
  uploadError.value = '';
  fileInput.value?.click();
};

const handleFileUpload = (event: Event) => {
  const target = event.target as HTMLInputElement;
  const files = target.files;
  if (!files || files.length === 0) return;

  const file = files[0];
  const ext = file.name.split('.').pop()?.toLowerCase();

  // Validate file size
  if (file.size > MAX_FILE_SIZE) {
    uploadError.value = `File quá lớn (${(file.size / 1024 / 1024).toFixed(1)} MB). Giới hạn: 50 MB.`;
    target.value = '';
    return;
  }

  // Validate extension
  if (ext !== 'vrm' && ext !== 'fbx') {
    uploadError.value = 'Định dạng không hỗ trợ. Chỉ chấp nhận .vrm và .fbx';
    target.value = '';
    return;
  }

  const format = ext === 'vrm' ? 'vrm' : 'fbx';
  const modelName = file.name.replace(/\.(vrm|fbx)$/i, '');

  models3D.value.push({
    name: modelName,
    filename: file.name,
    size: `${(file.size / 1024 / 1024).toFixed(1)} MB`,
    isActive: false,
    type: '3d',
    format,
  });

  uploadError.value = '';
  target.value = '';
  // TODO: Actually copy file to models directory via IPC
};

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

import { useI18n } from "../../composables/useI18n";
const { t } = useI18n();

onMounted(() => {
  // TODO: Load actual models from filesystem via Gateway
});
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

    <!-- Tab Switcher -->
    <div class="tab-bar">
      <button
        :class="['tab-btn', { active: activeTab === '3d' }]"
        @click="activeTab = '3d'"
      >
        {{ t('av_tab_3d') }}
      </button>
      <button
        :class="['tab-btn', { active: activeTab === '2d' }]"
        @click="activeTab = '2d'"
      >
        {{ t('av_tab_2d') }}
      </button>
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
          <h3 class="model-name">{{ model.name }}</h3>
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
        </div>
      </div>

      <!-- Add Model Card -->
      <div class="model-card card add-card" @click="triggerUpload">
        <div class="add-icon">+</div>
        <p class="add-text">{{ t('av_add') }}</p>
        <p class="add-hint">{{ t('av_hint') }}</p>
        <input
          ref="fileInput"
          type="file"
          accept=".vrm,.fbx"
          style="display: none;"
          @change="handleFileUpload"
        />
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
}

.model-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.model-file {
  font-size: 11px;
  color: var(--text-muted);
  font-family: monospace;
}

.model-size {
  font-size: 11px;
  color: var(--text-secondary);
}

.model-actions {
  padding: 0 4px;
}

.btn-sm {
  padding: 6px 12px;
  font-size: 12px;
  width: 100%;
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
