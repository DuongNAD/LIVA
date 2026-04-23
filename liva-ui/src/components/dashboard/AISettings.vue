<script setup lang="ts">
/**
 * AISettings.vue — AI Provider & Model Configuration
 * =====================================================
 * Switch between Local (GGUF) and Cloud (API) providers.
 * Configure model parameters, API keys, temperatures.
 * Dynamic UI: disables VRM-only features when FBX model is active.
 */
import { ref, computed, onMounted, watch } from "vue";
import type { ModelFormat } from "../../composables/use3DModel";
import { useGateway } from "../../composables/useGateway";

// Props from parent (DashboardApp passes currentModelFormat)
const props = defineProps<{
  currentModelFormat?: ModelFormat;
}>();

// Computed: is current model FBX? (disables VRM-only features)
const isFBX = computed(() => props.currentModelFormat === 'fbx');

// AI Provider
type AIProvider = 'local' | 'cloud';
const provider = ref<AIProvider>('local');

// Cloud Settings
const cloudBaseUrl = ref('');
const cloudApiKey = ref('');
const cloudModel = ref('');
const showApiKey = ref(false);

// Local Settings
const localModelsDir = ref('E:\\AI_Models');
const routerModel = ref('gemma-4-E4B-it-Q4_K_M.gguf');
const expertModel = ref('');

// Parameters
const temperature = ref(0.7);
const maxTokens = ref(4096);
const topP = ref(0.9);

// State
const isSaving = ref(false);
const saveMessage = ref('');

// Toggle provider
const toggleProvider = (p: AIProvider) => {
  provider.value = p;
};

// Open file picker for local models
let currentPickerTarget: 'router' | 'expert' = 'router';
const fileInputRef = ref<HTMLInputElement | null>(null);

const openModelPicker = (target: 'router' | 'expert') => {
  currentPickerTarget = target;
  if (fileInputRef.value) {
    fileInputRef.value.click();
  }
};

const onFileSelected = (e: Event) => {
  const target = e.target as HTMLInputElement;
  const file = target.files?.[0];
  if (file) {
    // In Electron, file has a 'path' property
    const fullPath = (file as any).path;
    if (fullPath) {
      const lastSlash = Math.max(fullPath.lastIndexOf('\\'), fullPath.lastIndexOf('/'));
      if (lastSlash !== -1) {
        localModelsDir.value = fullPath.substring(0, lastSlash);
        if (currentPickerTarget === 'router') {
          routerModel.value = fullPath.substring(lastSlash + 1);
        } else {
          expertModel.value = fullPath.substring(lastSlash + 1);
        }
      }
    } else {
      // Fallback if path is blocked by web security
      if (currentPickerTarget === 'router') {
        routerModel.value = file.name;
      } else {
        expertModel.value = file.name;
      }
    }
  }
  // Reset input so the same file can be selected again
  target.value = '';
};

const gateway = useGateway();

// Watch for external config updates (e.g. from backend on initial load)
watch(() => gateway.configData.value, (newVal) => {
  if (newVal && newVal.ai) {
    provider.value = newVal.ai.provider || 'local';
    cloudBaseUrl.value = newVal.ai.cloudBaseUrl || '';
    cloudApiKey.value = newVal.ai.cloudApiKey || '';
    cloudModel.value = newVal.ai.cloudModel || '';
    localModelsDir.value = newVal.ai.localModelsDir || 'E:\\AI_Models';
    routerModel.value = newVal.ai.routerModel || 'gemma-4-E4B-it-Q4_K_M.gguf';
    expertModel.value = newVal.ai.expertModel || '';
    temperature.value = newVal.ai.temperature || 0.7;
    maxTokens.value = newVal.ai.maxTokens || 4096;
    topP.value = newVal.ai.topP || 0.9;
  }
}, { immediate: true, deep: true });

// Save config
const saveConfig = async () => {
  isSaving.value = true;
  saveMessage.value = '';

  const config = {
    ai: {
      provider: provider.value,
      cloudBaseUrl: cloudBaseUrl.value,
      cloudApiKey: cloudApiKey.value,
      cloudModel: cloudModel.value,
      localModelsDir: localModelsDir.value,
      routerModel: routerModel.value,
      expertModel: expertModel.value,
      temperature: temperature.value,
      maxTokens: maxTokens.value,
      topP: topP.value,
    }
  };

  // Push to Gateway
  gateway.updateConfig(config);

  // Simulate save delay for UX
  await new Promise(r => setTimeout(r, 500));
  isSaving.value = false;
  saveMessage.value = '✓ Đã lưu thành công xuống liva-config.json!';
  setTimeout(() => { saveMessage.value = ''; }, 3000);
};

onMounted(() => {
  // If config isn't loaded yet, request it
  if (!gateway.configData.value || Object.keys(gateway.configData.value).length === 0) {
    gateway.sendMsg('get_config');
  }
});
</script>

<template>
  <div class="ai-settings animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">🤖 AI Settings</h1>
      <p class="page-desc">Cấu hình AI Provider, Model và tham số suy luận</p>
    </div>

    <!-- Provider Toggle -->
    <div class="card provider-card">
      <span class="section-subtitle">AI Provider</span>
      <div class="provider-toggle">
        <button
          :class="['provider-btn', { active: provider === 'local' }]"
          @click="toggleProvider('local')"
        >
          <span class="provider-icon">💻</span>
          <span class="provider-name">Local (GGUF)</span>
          <span class="provider-desc">llama.cpp trên máy</span>
        </button>
        <button
          :class="['provider-btn', { active: provider === 'cloud' }]"
          @click="toggleProvider('cloud')"
        >
          <span class="provider-icon">☁️</span>
          <span class="provider-name">Cloud (API)</span>
          <span class="provider-desc">OpenAI-compatible API</span>
        </button>
      </div>
    </div>

    <!-- Cloud Settings -->
    <div v-if="provider === 'cloud'" class="card settings-section animate-fadeIn">
      <span class="section-subtitle">Cloud Configuration</span>

      <div class="form-group">
        <label class="form-label" for="cloud-base-url">API Base URL</label>
        <input id="cloud-base-url" v-model="cloudBaseUrl" class="input" placeholder="https://api.openai.com/v1" />
        <span class="form-help">Endpoint tương thích OpenAI API</span>
      </div>

      <div class="form-group">
        <label class="form-label" for="cloud-api-key">API Key</label>
        <div class="input-with-toggle">
          <input
            id="cloud-api-key"
            v-model="cloudApiKey"
            :type="showApiKey ? 'text' : 'password'"
            class="input"
            placeholder="sk-..."
          />
          <button class="btn btn-ghost toggle-visibility" @click="showApiKey = !showApiKey">
            {{ showApiKey ? '🙈' : '👁️' }}
          </button>
        </div>
      </div>

      <div class="form-group">
        <label class="form-label" for="cloud-model">Model Name</label>
        <input id="cloud-model" v-model="cloudModel" class="input" placeholder="gpt-4o-mini" />
      </div>
    </div>

    <!-- Local Settings -->
    <div v-if="provider === 'local'" class="card settings-section animate-fadeIn">
      <span class="section-subtitle">Local Configuration</span>

      <div class="form-group">
        <label class="form-label" for="local-models-dir">Models Directory</label>
        <input id="local-models-dir" v-model="localModelsDir" class="input" placeholder="E:\AI_Models" />
      </div>

      <div class="grid-2">
        <div class="form-group">
          <label class="form-label" for="router-model">Router Model (Light)</label>
          <div class="input-with-btn">
            <input id="router-model" v-model="routerModel" class="input" placeholder="gemma-4..." />
            <button class="btn btn-secondary" @click="openModelPicker('router')" title="Chọn file .gguf từ máy tính">📂</button>
          </div>
          <span class="form-help">Model nhẹ cho routing (Port 8000)</span>
        </div>

        <div class="form-group">
          <label class="form-label" for="expert-model">Expert Model (Heavy)</label>
          <div class="input-with-btn">
            <input id="expert-model" v-model="expertModel" class="input" placeholder="(optional)" />
            <button class="btn btn-secondary" @click="openModelPicker('expert')" title="Chọn file .gguf từ máy tính">📂</button>
          </div>
          <span class="form-help">Model nặng cho deep tasks (Port 8001)</span>
        </div>
      </div>
    </div>

    <!-- Hidden File Input for Model Selection -->
    <input 
      type="file" 
      ref="fileInputRef" 
      accept=".gguf" 
      style="display: none" 
      @change="onFileSelected" 
    />

    <!-- Parameters -->
    <div class="card settings-section">
      <span class="section-subtitle">Inference Parameters</span>

      <div class="grid-3">
        <div class="form-group">
          <label class="form-label" for="temp-slider">Temperature</label>
          <div class="slider-group">
            <input
              id="temp-slider"
              type="range"
              v-model.number="temperature"
              min="0"
              max="2"
              step="0.1"
              class="slider"
            />
            <span class="slider-value">{{ temperature }}</span>
          </div>
          <span class="form-help">Độ sáng tạo (0 = deterministic)</span>
        </div>

        <div class="form-group">
          <label class="form-label" for="max-tokens">Max Tokens</label>
          <input id="max-tokens" v-model.number="maxTokens" type="number" class="input" min="256" max="32768" />
        </div>

        <div class="form-group">
          <label class="form-label" for="top-p-slider">Top-P</label>
          <div class="slider-group">
            <input
              id="top-p-slider"
              type="range"
              v-model.number="topP"
              min="0"
              max="1"
              step="0.05"
              class="slider"
            />
            <span class="slider-value">{{ topP }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- VRM-Only Features (disabled when FBX is active) -->
    <div class="card settings-section">
      <div class="vrm-features-header">
        <span class="section-subtitle">Avatar Features</span>
        <span v-if="isFBX" class="badge badge-fbx-disabled">FBX — Hạn chế</span>
        <span v-else class="badge badge-success">VRM — Đầy đủ</span>
      </div>

      <div :class="['feature-grid', { 'features-disabled': isFBX }]">
        <div class="feature-item">
          <div class="feature-info">
            <span class="feature-icon">👄</span>
            <div>
              <span class="feature-name">Lip-sync</span>
              <span class="feature-desc">Nhép môi theo giọng nói</span>
            </div>
          </div>
          <div class="feature-status">
            <span v-if="isFBX" class="badge badge-muted" title="Chỉ hỗ trợ trên định dạng VRM">Không khả dụng</span>
            <span v-else class="badge badge-success">✓ Hoạt động</span>
          </div>
        </div>

        <div class="feature-item">
          <div class="feature-info">
            <span class="feature-icon">👁️</span>
            <div>
              <span class="feature-name">Auto-Blink</span>
              <span class="feature-desc">Chớp mắt tự nhiên</span>
            </div>
          </div>
          <div class="feature-status">
            <span v-if="isFBX" class="badge badge-muted" title="Chỉ hỗ trợ trên định dạng VRM">Không khả dụng</span>
            <span v-else class="badge badge-success">✓ Hoạt động</span>
          </div>
        </div>

        <div class="feature-item">
          <div class="feature-info">
            <span class="feature-icon">📸</span>
            <div>
              <span class="feature-name">Face Tracking</span>
              <span class="feature-desc">Theo dõi khuôn mặt qua webcam</span>
            </div>
          </div>
          <div class="feature-status">
            <span v-if="isFBX" class="badge badge-muted" title="Chỉ hỗ trợ trên định dạng VRM">Không khả dụng</span>
            <span v-else class="badge badge-success">✓ Hoạt động</span>
          </div>
        </div>

        <div class="feature-item">
          <div class="feature-info">
            <span class="feature-icon">🫁</span>
            <div>
              <span class="feature-name">Idle Animation</span>
              <span class="feature-desc">Thở + micro-sway</span>
            </div>
          </div>
          <div class="feature-status">
            <span v-if="isFBX" class="badge badge-info">FBX Mixer</span>
            <span v-else class="badge badge-success">✓ Procedural</span>
          </div>
        </div>
      </div>

      <p v-if="isFBX" class="fbx-hint">
        💡 Định dạng FBX không có chuẩn blendshapes cho khuôn mặt. Chuyển sang VRM để sử dụng đầy đủ tính năng.
      </p>
    </div>

    <!-- Save Button -->
    <div class="save-bar">
      <button class="btn btn-primary" @click="saveConfig" :disabled="isSaving">
        {{ isSaving ? '⏳ Đang lưu...' : '💾 Lưu cấu hình' }}
      </button>
      <span v-if="saveMessage" class="save-message animate-fadeIn">{{ saveMessage }}</span>
    </div>
  </div>
</template>

<style scoped>
.ai-settings {
  padding: var(--space-lg);
  overflow-y: auto;
  height: 100%;
}

.page-header { margin-bottom: var(--space-lg); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }

.settings-section { margin-bottom: var(--space-md); }

/* Provider Toggle */
.provider-card { margin-bottom: var(--space-md); }

.provider-toggle {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-sm);
  margin-top: var(--space-sm);
}

.provider-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: var(--space-md);
  background: var(--bg-tertiary);
  border: 2px solid var(--border-default);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all var(--transition-fast);
  color: var(--text-secondary);
}

.provider-btn:hover {
  border-color: var(--text-muted);
}

.provider-btn.active {
  border-color: var(--accent-start);
  background: rgba(124, 58, 237, 0.08);
  color: var(--text-primary);
}

.provider-icon { font-size: 24px; }
.provider-name { font-size: 14px; font-weight: 600; }
.provider-desc { font-size: 11px; color: var(--text-muted); }

/* Input with toggle */
.input-with-toggle {
  position: relative;
  display: flex;
}

.input-with-toggle .input {
  padding-right: 48px;
}

.toggle-visibility {
  position: absolute;
  right: 4px;
  top: 50%;
  transform: translateY(-50%);
  padding: 4px 8px;
  font-size: 16px;
}

/* Input with button (File Picker) */
.input-with-btn {
  display: flex;
  gap: 8px;
}
.input-with-btn .input {
  flex: 1;
}
.input-with-btn .btn {
  padding: 0 16px;
  font-size: 16px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-default);
  color: var(--text-primary);
}
.input-with-btn .btn:hover {
  background: var(--bg-hover);
}

/* Slider */
.slider-group {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.slider {
  flex: 1;
  -webkit-appearance: none;
  height: 4px;
  background: var(--bg-hover);
  border-radius: 2px;
  outline: none;
}

.slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 16px;
  height: 16px;
  background: var(--accent-start);
  border-radius: 50%;
  cursor: pointer;
}

.slider-value {
  font-size: 13px;
  font-weight: 600;
  color: var(--accent-start);
  min-width: 32px;
  text-align: right;
}

/* Save Bar */
.save-bar {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  padding-top: var(--space-md);
  border-top: 1px solid var(--border-default);
  margin-top: var(--space-md);
}

.save-message {
  font-size: 13px;
  color: var(--color-success);
  font-weight: 500;
}

/* VRM-Only Features Section */
.vrm-features-header {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  margin-bottom: var(--space-md);
}

.feature-grid {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.features-disabled {
  opacity: 0.6;
}

.feature-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: var(--bg-tertiary);
  border-radius: var(--radius-sm);
  transition: all var(--transition-fast);
}

.feature-info {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.feature-icon {
  font-size: 18px;
  width: 28px;
  text-align: center;
}

.feature-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  display: block;
}

.feature-desc {
  font-size: 11px;
  color: var(--text-muted);
}

.badge-fbx-disabled {
  background: rgba(255, 166, 43, 0.12);
  color: #ffa62b;
  border: 1px solid rgba(255, 166, 43, 0.25);
}

.badge-muted {
  background: var(--bg-hover);
  color: var(--text-muted);
}

.fbx-hint {
  margin-top: var(--space-sm);
  font-size: 11px;
  color: var(--text-muted);
  padding: 8px;
  background: rgba(255, 166, 43, 0.06);
  border-radius: var(--radius-sm);
  border-left: 3px solid rgba(255, 166, 43, 0.4);
}
</style>
