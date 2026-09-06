<script setup lang="ts">
/**
 * SkillConfigModal.vue — Skill Parameters Configuration Modal
 * =============================================================
 * Allows configuring custom execution parameters (timeout, retries,
 * verbosity, sandbox options) per skill.
 */
import { ref, onMounted } from "vue";
import { useGateway } from "../../../composables/useGateway";
import { logger } from "../../../utils/logger";

const props = defineProps<{
  skillId: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "saved", params: Record<string, unknown>): void;
}>();

const gateway = useGateway();
const loading = ref(true);
const saving = ref(false);
const params = ref<Record<string, unknown>>({
  timeoutSeconds: 30,
  maxRetries: 3,
  logVerbosity: "info",
  sandboxEnabled: true,
});

const loadConfig = async () => {
  loading.value = true;
  try {
    const res = await gateway.getSkillConfig(props.skillId);
    if (res?.params) {
      params.value = { ...params.value, ...res.params };
    }
  } catch (err) {
    logger.error("[SkillConfigModal]", "Failed to load skill config:", err);
  } finally {
    loading.value = false;
  }
};

const handleSave = async () => {
  saving.value = true;
  try {
    await gateway.saveSkillConfig(props.skillId, params.value);
    emit("saved", params.value);
    emit("close");
  } catch (err) {
    logger.error("[SkillConfigModal]", "Failed to save skill config:", err);
  } finally {
    saving.value = false;
  }
};

onMounted(() => {
  loadConfig();
});
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <div class="modal-card animate-scaleUp">
      <!-- Modal Header -->
      <div class="modal-header">
        <div class="header-title">
          <span class="icon">⚙️</span>
          <h3>Configure {{ props.skillId }}</h3>
        </div>
        <button class="btn-close" @click="emit('close')">✕</button>
      </div>

      <!-- Modal Body -->
      <div class="modal-body">
        <div v-if="loading" class="modal-loading">
          <div class="spinner"></div>
          <p>Loading parameters...</p>
        </div>

        <div v-else class="form-container">
          <div class="form-group">
            <label>Execution Timeout (seconds)</label>
            <input
              type="number"
              v-model.number="params.timeoutSeconds"
              min="5"
              max="300"
              placeholder="30"
            />
            <span class="field-hint">Maximum time permitted for skill tool execution</span>
          </div>

          <div class="form-group">
            <label>Maximum Auto-Retries</label>
            <input
              type="number"
              v-model.number="params.maxRetries"
              min="0"
              max="10"
              placeholder="3"
            />
            <span class="field-hint">Number of retry attempts upon transient tool failures</span>
          </div>

          <div class="form-group">
            <label>Log Verbosity</label>
            <select v-model="params.logVerbosity">
              <option value="debug">Debug (Detailed payload traces)</option>
              <option value="info">Info (Standard milestones)</option>
              <option value="warn">Warn (Only warnings & errors)</option>
              <option value="error">Error (Only fatal execution exceptions)</option>
            </select>
          </div>

          <div class="form-group toggle-row">
            <div class="toggle-info">
              <label>Strict Sandbox Enforcement</label>
              <span class="field-hint">Execute tool logic within sandboxed memory & restricted I/O</span>
            </div>
            <label class="switch">
              <input type="checkbox" v-model="params.sandboxEnabled" />
              <span class="slider"></span>
            </label>
          </div>
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="modal-footer">
        <button class="btn btn-secondary" @click="emit('close')">Cancel</button>
        <button class="btn btn-primary" :disabled="saving || loading" @click="handleSave">
          {{ saving ? 'Saving...' : 'Save Configuration' }}
        </button>
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
  width: 480px;
  max-width: 90vw;
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
  gap: 8px;
}

.header-title .icon {
  font-size: 18px;
}

.header-title h3 {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
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
}

.btn-close:hover {
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-primary, #ffffff);
}

.modal-body {
  padding: 20px;
}

.modal-loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 180px;
  gap: 12px;
  color: var(--text-secondary, #94a3b8);
}

.form-container {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-group label {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary, #94a3b8);
}

.form-group input, .form-group select {
  background: #080a10;
  border: 1px solid var(--border-default, #262a3d);
  border-radius: 6px;
  padding: 8px 12px;
  color: #ffffff;
  font-size: 13px;
}

.form-group input:focus, .form-group select:focus {
  outline: none;
  border-color: #818cf8;
}

.field-hint {
  font-size: 11px;
  color: var(--text-muted, #64748b);
}

.toggle-row {
  flex-direction: row;
  justify-content: space-between;
  align-items: center;
  padding-top: 6px;
}

.toggle-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
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
