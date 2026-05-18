<script setup lang="ts">
import { ref, watch, onActivated, onMounted, onDeactivated } from 'vue';
import { useGateway } from '../../composables/useGateway';
import { useI18n } from '../../composables/useI18n';
import { logger } from '../../utils/logger';

// Track pending reset timeout for cleanup
let resetTimeout: ReturnType<typeof setTimeout> | null = null;

const gateway = useGateway();
const { t } = useI18n();
const isGeoEnabled = ref(false);
const digestInterestsEnabled = ref(false);
const digestInterestsHour = ref(7);
const digestInterestsMinute = ref(0);
const digestInterestsDeliverUI = ref(true);
const digestInterestsDeliverTelegram = ref(true);
const digestInterestsDeliverZalo = ref(false);
const digestInterestsDeliverEmail = ref(false);

const digestFocusEnabled = ref(false);
const digestFocusHour = ref(8);
const digestFocusMinute = ref(0);
const digestFocusDeliverUI = ref(true);
const digestFocusDeliverTelegram = ref(true);
const digestFocusDeliverZalo = ref(false);
const digestFocusDeliverEmail = ref(false);
const digestFocusTopics = ref("");

const isSaving = ref(false);

// [P5] Memory Reset State
const showResetConfirm = ref(false);
const isResetting = ref(false);
const resetResult = ref<{ success: boolean; error?: string } | null>(null);

const syncConfig = () => {
    const sys = gateway.configData.value?.system;
    isGeoEnabled.value = sys?.geolocationEnabled ?? false;
    digestInterestsEnabled.value = sys?.digestInterestsEnabled ?? false;
    digestInterestsHour.value = sys?.digestInterestsHour ?? 7;
    digestInterestsMinute.value = sys?.digestInterestsMinute ?? 0;
    digestInterestsDeliverUI.value = sys?.digestInterestsDeliverUI ?? true;
    digestInterestsDeliverTelegram.value = sys?.digestInterestsDeliverTelegram ?? true;
    digestInterestsDeliverZalo.value = sys?.digestInterestsDeliverZalo ?? false;
    digestInterestsDeliverEmail.value = sys?.digestInterestsDeliverEmail ?? false;

    digestFocusEnabled.value = sys?.digestFocusEnabled ?? false;
    digestFocusHour.value = sys?.digestFocusHour ?? 8;
    digestFocusMinute.value = sys?.digestFocusMinute ?? 0;
    digestFocusDeliverUI.value = sys?.digestFocusDeliverUI ?? true;
    digestFocusDeliverTelegram.value = sys?.digestFocusDeliverTelegram ?? true;
    digestFocusDeliverZalo.value = sys?.digestFocusDeliverZalo ?? false;
    digestFocusDeliverEmail.value = sys?.digestFocusDeliverEmail ?? false;
    digestFocusTopics.value = sys?.digestFocusTopics ?? "";
};

// ANTI-ZOMBIE RAM (Rule 4.8): Component trong <KeepAlive> phải sync dữ liệu khi quay lại tab
onMounted(() => {
    // If config isn't loaded yet, request it
    if (!gateway.configData.value || Object.keys(gateway.configData.value).length === 0) {
        gateway.sendMsg('get_config');
    } else {
        syncConfig();
    }
});
onActivated(syncConfig);
onDeactivated(() => {
    // Cancel any pending reset timeout when tab is deactivated
    if (resetTimeout) {
        clearTimeout(resetTimeout);
        resetTimeout = null;
    }
});

// Lắng nghe thay đổi lỡ config bị update ngầm qua WebSocket
watch(() => gateway.configData.value?.system, (newVal) => {
    if (newVal) {
        if (newVal.geolocationEnabled !== undefined) isGeoEnabled.value = newVal.geolocationEnabled;
        if (newVal.digestInterestsEnabled !== undefined) digestInterestsEnabled.value = newVal.digestInterestsEnabled;
        if (newVal.digestInterestsHour !== undefined) digestInterestsHour.value = newVal.digestInterestsHour;
        if (newVal.digestInterestsMinute !== undefined) digestInterestsMinute.value = newVal.digestInterestsMinute;
        if (newVal.digestInterestsDeliverUI !== undefined) digestInterestsDeliverUI.value = newVal.digestInterestsDeliverUI;
        if (newVal.digestInterestsDeliverTelegram !== undefined) digestInterestsDeliverTelegram.value = newVal.digestInterestsDeliverTelegram;
        if (newVal.digestInterestsDeliverZalo !== undefined) digestInterestsDeliverZalo.value = newVal.digestInterestsDeliverZalo;
        if (newVal.digestInterestsDeliverEmail !== undefined) digestInterestsDeliverEmail.value = newVal.digestInterestsDeliverEmail;

        if (newVal.digestFocusEnabled !== undefined) digestFocusEnabled.value = newVal.digestFocusEnabled;
        if (newVal.digestFocusHour !== undefined) digestFocusHour.value = newVal.digestFocusHour;
        if (newVal.digestFocusMinute !== undefined) digestFocusMinute.value = newVal.digestFocusMinute;
        if (newVal.digestFocusDeliverUI !== undefined) digestFocusDeliverUI.value = newVal.digestFocusDeliverUI;
        if (newVal.digestFocusDeliverTelegram !== undefined) digestFocusDeliverTelegram.value = newVal.digestFocusDeliverTelegram;
        if (newVal.digestFocusDeliverZalo !== undefined) digestFocusDeliverZalo.value = newVal.digestFocusDeliverZalo;
        if (newVal.digestFocusDeliverEmail !== undefined) digestFocusDeliverEmail.value = newVal.digestFocusDeliverEmail;
        if (newVal.digestFocusTopics !== undefined) digestFocusTopics.value = newVal.digestFocusTopics;
    }
}, { deep: true });

const saveSettings = async () => {
    isSaving.value = true;
    try {
        const payload = {
            system: { 
                geolocationEnabled: isGeoEnabled.value,
                digestInterestsEnabled: digestInterestsEnabled.value,
                digestInterestsHour: Number(digestInterestsHour.value),
                digestInterestsMinute: Number(digestInterestsMinute.value),
                digestInterestsDeliverUI: digestInterestsDeliverUI.value,
                digestInterestsDeliverTelegram: digestInterestsDeliverTelegram.value,
                digestInterestsDeliverZalo: digestInterestsDeliverZalo.value,
                digestInterestsDeliverEmail: digestInterestsDeliverEmail.value,
                
                digestFocusEnabled: digestFocusEnabled.value,
                digestFocusHour: Number(digestFocusHour.value),
                digestFocusMinute: Number(digestFocusMinute.value),
                digestFocusDeliverUI: digestFocusDeliverUI.value,
                digestFocusDeliverTelegram: digestFocusDeliverTelegram.value,
                digestFocusDeliverZalo: digestFocusDeliverZalo.value,
                digestFocusDeliverEmail: digestFocusDeliverEmail.value,
                digestFocusTopics: digestFocusTopics.value
            }
        };

        logger.info('[SettingsView]', 'Saving settings', payload);
        gateway.updateConfig(payload);
        // Simulate network delay for UX
        await new Promise(r => setTimeout(r, 600));
        logger.info('[SettingsView]', 'Settings save request sent');
    } catch (error) {
        logger.error('[SettingsView]', 'Cập nhật config thất bại', error instanceof Error ? error.message : String(error));
        // Rollback state trên UI nếu lỗi
        isGeoEnabled.value = !isGeoEnabled.value;
    } finally {
        isSaving.value = false;
    }
};

// [P5] Memory Reset
const openResetConfirm = () => {
    resetResult.value = null;
    showResetConfirm.value = true;
};

const onMemoryResetResult = (payload: any) => {
    resetResult.value = payload;
    isResetting.value = false;
    gateway.offMemoryResetResult();
    if (resetTimeout) { clearTimeout(resetTimeout); resetTimeout = null; }
    // Auto-close modal after 2s on success
    if (payload.success) {
        resetTimeout = setTimeout(() => { showResetConfirm.value = false; resetTimeout = null; }, 2000);
    }
};

const confirmReset = () => {
    isResetting.value = true;
    resetResult.value = null;

    // Register callback
    gateway.onMemoryResetResult(onMemoryResetResult);

    // Send reset command via WebSocket
    gateway.sendMsg('reset_memory');

    // Cleanup after 15s (timeout)
    resetTimeout = setTimeout(() => {
        gateway.offMemoryResetResult();
        if (resetTimeout) { clearTimeout(resetTimeout); resetTimeout = null; }
        if (isResetting.value) {
            isResetting.value = false;
            resetResult.value = { success: false, error: 'Timeout — không nhận được phản hồi từ Gateway.' };
        }
    }, 15000);
};
const cancelReset = () => {
    showResetConfirm.value = false;
    resetResult.value = null;
};
</script>

<template>
  <div class="settings-view animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">⚙️ {{ t('set_title') }}</h1>
      <p class="page-desc">{{ t('set_desc') }}</p>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-2 gap-4 mb-4">
        <!-- Bản tin Quan tâm (Bên trái) -->
        <div class="card settings-section animate-fadeIn h-full mb-0">
            <h2 class="section-subtitle mb-4">{{ t('set_digest_focus') }}</h2>
            <div class="flex flex-col gap-4">
                <label class="toggle-label cursor-pointer">
                    <input 
                        type="checkbox" 
                        v-model="digestFocusEnabled" 
                        @change="saveSettings"
                        :disabled="isSaving"
                        class="form-checkbox h-5 w-5 disabled-opacity"
                    />
                    <div class="flex flex-col">
                        <span class="font-medium">{{ t('set_digest_focus_en') }}</span>
                        <span class="text-xs text-gray-400">{{ t('set_digest_focus_desc') }}</span>
                    </div>
                    <span v-if="isSaving" class="saving-text">({{ t('set_saving') }})</span>
                </label>

                <div v-if="digestFocusEnabled" class="proactive-settings-panel animate-fadeIn">
                    <!-- Thời gian -->
                    <div class="flex items-center gap-2 mb-4">
                        <span class="text-sm font-medium">{{ t('set_schedule') }}</span>
                        <input type="number" v-model="digestFocusHour" @change="saveSettings" min="0" max="23" class="input time-input" :disabled="isSaving" />
                        <span class="text-sm">:</span>
                        <input type="number" v-model="digestFocusMinute" @change="saveSettings" min="0" max="59" step="5" class="input time-input" :disabled="isSaving" />
                    </div>

                    <!-- Phương thức nhận tin -->
                    <div class="mb-4">
                        <span class="text-sm font-medium block mb-2">{{ t('set_delivery') }}</span>
                        <div class="flex flex-wrap gap-4">
                            <label class="toggle-label cursor-pointer text-sm">
                                <input type="checkbox" v-model="digestFocusDeliverUI" @change="saveSettings" :disabled="isSaving" class="form-checkbox h-4 w-4" />
                                Liva UI
                            </label>
                            <label class="toggle-label cursor-pointer text-sm">
                                <input type="checkbox" v-model="digestFocusDeliverTelegram" @change="saveSettings" :disabled="isSaving" class="form-checkbox h-4 w-4" />
                                Telegram
                            </label>
                            <label class="toggle-label cursor-pointer text-sm">
                                <input type="checkbox" v-model="digestFocusDeliverZalo" @change="saveSettings" :disabled="isSaving" class="form-checkbox h-4 w-4" />
                                Zalo
                            </label>
                            <label class="toggle-label cursor-pointer text-sm">
                                <input type="checkbox" v-model="digestFocusDeliverEmail" @change="saveSettings" :disabled="isSaving" class="form-checkbox h-4 w-4" />
                                Email
                            </label>
                        </div>
                    </div>

                    <!-- Phân tách Nội dung -->
                    <div class="form-group mb-0">
                        <label class="form-label text-sm">{{ t('set_topics') }}</label>
                        <textarea 
                            v-model="digestFocusTopics" 
                            @blur="saveSettings"
                            :disabled="isSaving"
                            class="input w-full h-16 text-sm resize-none" 
                            :placeholder="t('set_topics_ph')"></textarea>
                        <span class="form-help text-xs">{{ t('set_topics_desc') }}</span>
                    </div>
                </div>
            </div>
        </div>

        <!-- Bản tin Sở thích (Bên phải) -->
        <div class="card settings-section animate-fadeIn h-full mb-0">
            <h2 class="section-subtitle mb-4">{{ t('set_digest_interest') }}</h2>
            
            <div class="flex flex-col gap-4">
                <label class="toggle-label cursor-pointer">
                    <input 
                        type="checkbox" 
                        v-model="digestInterestsEnabled" 
                        @change="saveSettings"
                        :disabled="isSaving"
                        class="form-checkbox h-5 w-5 disabled-opacity"
                    />
                    <div class="flex flex-col">
                        <span class="font-medium">{{ t('set_digest_interest_en') }}</span>
                        <span class="text-xs text-gray-400">{{ t('set_digest_interest_desc') }}</span>
                    </div>
                    <span v-if="isSaving" class="saving-text">({{ t('set_saving') }})</span>
                </label>

                <div v-if="digestInterestsEnabled" class="proactive-settings-panel animate-fadeIn">
                    <!-- Thời gian -->
                    <div class="flex items-center gap-2 mb-4">
                        <span class="text-sm font-medium">{{ t('set_schedule') }}</span>
                        <input type="number" v-model="digestInterestsHour" @change="saveSettings" min="0" max="23" class="input time-input" :disabled="isSaving" />
                        <span class="text-sm">:</span>
                        <input type="number" v-model="digestInterestsMinute" @change="saveSettings" min="0" max="59" step="5" class="input time-input" :disabled="isSaving" />
                    </div>

                    <!-- Phương thức nhận tin -->
                    <div class="mb-4">
                        <span class="text-sm font-medium block mb-2">{{ t('set_delivery') }}</span>
                        <div class="flex flex-wrap gap-4">
                            <label class="toggle-label cursor-pointer text-sm">
                                <input type="checkbox" v-model="digestInterestsDeliverUI" @change="saveSettings" :disabled="isSaving" class="form-checkbox h-4 w-4" />
                                Liva UI
                            </label>
                            <label class="toggle-label cursor-pointer text-sm">
                                <input type="checkbox" v-model="digestInterestsDeliverTelegram" @change="saveSettings" :disabled="isSaving" class="form-checkbox h-4 w-4" />
                                Telegram
                            </label>
                            <label class="toggle-label cursor-pointer text-sm">
                                <input type="checkbox" v-model="digestInterestsDeliverZalo" @change="saveSettings" :disabled="isSaving" class="form-checkbox h-4 w-4" />
                                Zalo
                            </label>
                            <label class="toggle-label cursor-pointer text-sm">
                                <input type="checkbox" v-model="digestInterestsDeliverEmail" @change="saveSettings" :disabled="isSaving" class="form-checkbox h-4 w-4" />
                                Email
                            </label>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <div class="card settings-section animate-fadeIn">
        <h2 class="section-subtitle mb-4">{{ t('set_privacy') }}</h2>
        
        <div class="flex flex-col gap-2">
            <label class="toggle-label cursor-pointer">
                <input 
                    type="checkbox" 
                    v-model="isGeoEnabled" 
                    @change="saveSettings"
                    :disabled="isSaving"
                    class="form-checkbox h-5 w-5 disabled-opacity"
                />
                <span class="font-medium">{{ t('set_geo') }}</span>
                <span v-if="isSaving" class="saving-text">({{ t('set_saving') }})</span>
            </label>
            
            <p class="warning-text" v-html="t('set_geo_warn')"></p>
        </div>
    </div>

    <!-- [P5] Danger Zone — Memory Reset -->
    <div class="card danger-section animate-fadeIn">
        <h2 class="section-subtitle danger-title">{{ t('set_danger') }}</h2>

        <div class="danger-item">
            <div class="danger-info">
                <h3 class="danger-label">{{ t('set_wipe') }}</h3>
                <p class="danger-desc" v-html="t('set_wipe_desc')"></p>
            </div>
            <button 
                class="btn-danger" 
                @click="openResetConfirm"
                :disabled="isResetting"
            >
                {{ isResetting ? t('set_wiping') : t('set_btn_wipe') }}
            </button>
        </div>
    </div>

    <!-- Confirmation Modal -->
    <Teleport to="body">
      <div v-if="showResetConfirm" class="modal-overlay" @click.self="cancelReset">
        <div class="modal-card animate-fadeIn">
          <div class="modal-icon">🧠</div>
          <h3 class="modal-title">{{ t('set_wipe_modal') }}</h3>
          <p class="modal-desc">{{ t('set_wipe_modal_desc') }}</p>

          <!-- Result message -->
          <div v-if="resetResult?.success" class="result-success">
            {{ t('set_wipe_success') }}
          </div>
          <div v-else-if="resetResult?.error" class="result-error">
            {{ t('set_wipe_error', { error: resetResult.error }) }}
          </div>

          <div v-if="!resetResult" class="modal-actions">
            <button class="btn-cancel" @click="cancelReset" :disabled="isResetting">{{ t('set_cancel') }}</button>
            <button class="btn-confirm-danger" @click="confirmReset" :disabled="isResetting">
              <span v-if="isResetting" class="loading-dots">{{ t('set_wiping') }}</span>
              <span v-else>{{ t('set_confirm_wipe') }}</span>
            </button>
          </div>
          <div v-else class="modal-actions">
            <button class="btn-cancel" @click="cancelReset">{{ t('set_close') }}</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.settings-view {
  padding: var(--space-lg);
  color: var(--text-primary);
  height: 100%;
  overflow-y: auto;
}
.page-header {
  margin-bottom: var(--space-lg);
}
.section-title {
  font-size: 24px;
  font-weight: 700;
}
.page-desc {
  color: var(--text-secondary);
  font-size: 13px;
  margin-top: 4px;
}
.section-subtitle {
  font-size: 16px;
  font-weight: 600;
  margin-bottom: var(--space-md);
  color: var(--text-primary);
}
.settings-section {
  margin-bottom: var(--space-md);
}
.toggle-label {
    display: flex;
    align-items: center;
    gap: 12px;
}
.form-checkbox {
    width: 20px;
    height: 20px;
    accent-color: var(--accent-start);
}
.font-medium {
    font-weight: 500;
    font-size: 14px;
}
.saving-text {
    font-size: 13px;
    color: var(--text-muted);
    animation: pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
}
.warning-text {
    font-size: 12.5px;
    color: #d97706;
    margin-top: 8px;
    margin-left: 32px;
    max-width: 600px;
    line-height: 1.5;
    background: rgba(217, 119, 6, 0.05);
    padding: 10px;
    border-radius: var(--radius-sm);
    border-left: 3px solid #d97706;
}
.disabled-opacity {
    opacity: 0.5;
}
.proactive-settings-panel {
    margin-left: 32px;
    padding: 16px;
    background: var(--bg-inset);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-md);
}
.time-input {
    width: 60px;
    padding: 6px;
    text-align: center;
    border-radius: var(--radius-sm);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-default);
    color: var(--text-primary);
}
.info-text {
    font-size: 12.5px;
    color: #3b82f6;
    margin-top: 4px;
    max-width: 600px;
    line-height: 1.5;
    background: rgba(59, 130, 246, 0.05);
    padding: 10px;
    border-radius: var(--radius-sm);
    border-left: 3px solid #3b82f6;
}

/* ═══ Danger Zone ═══ */
.danger-section {
    border: 1px solid rgba(239, 68, 68, 0.3);
    background: rgba(239, 68, 68, 0.03);
    margin-top: var(--space-md);
}
.danger-title {
    color: #ef4444;
}
.danger-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-md);
}
.danger-info {
    flex: 1;
}
.danger-label {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 4px;
}
.danger-desc {
    font-size: 12.5px;
    color: var(--text-secondary);
    line-height: 1.5;
    max-width: 500px;
}
.btn-danger {
    padding: 8px 20px;
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: var(--radius-sm);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
    white-space: nowrap;
}
.btn-danger:hover {
    background: rgba(239, 68, 68, 0.25);
    border-color: #ef4444;
}
.btn-danger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

/* ═══ Modal ═══ */
.modal-overlay {
    position: fixed;
    top: 0; left: 0; right: 0; bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(4px);
    z-index: 10000;
    display: flex;
    align-items: center;
    justify-content: center;
}
.modal-card {
    background: var(--bg-secondary, #1a1a2e);
    border: 1px solid var(--border-default, #333);
    border-radius: 16px;
    padding: 32px;
    max-width: 440px;
    width: 90%;
    text-align: center;
    box-shadow: var(--shadow-lg);
}
.modal-icon {
    font-size: 48px;
    margin-bottom: 16px;
}
.modal-title {
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary, #fff);
    margin-bottom: 12px;
}
.modal-desc {
    font-size: 13px;
    color: var(--text-secondary, #999);
    line-height: 1.6;
    margin-bottom: 24px;
}
.modal-actions {
    display: flex;
    gap: 12px;
    justify-content: center;
}
.btn-cancel {
    padding: 10px 24px;
    background: var(--bg-tertiary, #2a2a3e);
    color: var(--text-primary, #fff);
    border: 1px solid var(--border-default, #333);
    border-radius: var(--radius-sm, 8px);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
}
.btn-cancel:hover {
    background: var(--bg-hover, #333);
}
.btn-confirm-danger {
    padding: 10px 24px;
    background: #ef4444;
    color: #fff;
    border: none;
    border-radius: var(--radius-sm, 8px);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
}
.btn-confirm-danger:hover {
    background: #dc2626;
}
.btn-confirm-danger:disabled {
    opacity: 0.6;
    cursor: not-allowed;
}
.result-success {
    padding: 12px;
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(34, 197, 94, 0.3);
    border-radius: var(--radius-sm, 8px);
    color: #22c55e;
    font-size: 14px;
    font-weight: 600;
    margin-bottom: 16px;
}
.result-error {
    padding: 12px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: var(--radius-sm, 8px);
    color: #ef4444;
    font-size: 13px;
    margin-bottom: 16px;
}
.loading-dots::after {
    content: '';
    animation: dots 1.5s infinite;
}
@keyframes dots {
    0% { content: ''; }
    25% { content: '.'; }
    50% { content: '..'; }
    75% { content: '...'; }
}
</style>
