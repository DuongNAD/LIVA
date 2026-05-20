<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { useGateway } from '../../composables/useGateway';
import { useI18n } from '../../composables/useI18n';

const gateway = useGateway();
const { t } = useI18n();

const form = ref({
  name: '',
  birthYear: '',
  nationality: 'Việt Nam',
  language: 'vi-VN',
  hobbies: '',
  preferences: 'Friendly' // Maps to AI Tone
});

const isSubmitting = ref(false);
const errorMsg = ref('');

/**
 * Normalize browser locale to supported i18n codes.
 * e.g. 'vi' | 'vi-VN' → 'vi-VN', 'en' | 'en-US' | 'en-GB' → 'en-US'
 */
function normalizeLocale(raw: string): 'vi-VN' | 'en-US' {
  const lower = raw.toLowerCase();
  if (lower.startsWith('vi')) return 'vi-VN';
  return 'en-US';
}

onMounted(() => {
  // Auto-detect language from browser locale
  if (navigator.language) {
    form.value.language = normalizeLocale(navigator.language);
  }
  // Also default nationality based on language
  if (form.value.language === 'en-US') {
    form.value.nationality = '';
  }
});

// Live-preview: when user changes language dropdown, immediately sync to profile
// so that useI18n's computed currentLang updates and all labels switch instantly
watch(() => form.value.language, (newLang, oldLang) => {
  if (newLang && oldLang && newLang !== oldLang) {
    if (!gateway.userProfile.value || gateway.userProfile.value.language !== newLang) {
      gateway.saveUserProfile({ ...form.value, language: newLang });
    }
  }
});

const submitForm = async () => {
  errorMsg.value = '';
  
  if (!form.value.name.trim()) {
    errorMsg.value = t('ob_name_err');
    return;
  }
  if (!String(form.value.birthYear).trim()) {
    errorMsg.value = t('ob_year_err');
    return;
  }
  if (!form.value.nationality.trim()) {
    errorMsg.value = t('ob_nat_err');
    return;
  }

  isSubmitting.value = true;
  gateway.saveUserProfile({ ...form.value });
  setTimeout(() => {
    isSubmitting.value = false;
  }, 1200);
};
</script>

<template>
  <div class="onboarding-container animate-fadeIn">
    <div class="onboarding-card">
      <div class="onboarding-header">
        <div class="logo-icon">✨</div>
        <h2 class="onboarding-title">{{ t('ob_title') }}</h2>
        <p class="onboarding-subtitle">{{ t('ob_subtitle') }}</p>
      </div>
      
      <form @submit.prevent="submitForm" class="onboarding-form">
        <div class="form-group">
          <label>{{ t('pr_name') }} <span class="required">*</span></label>
          <input 
            v-model="form.name" 
            type="text" 
            :placeholder="t('pr_name_ph')" 
            :disabled="isSubmitting"
          />
        </div>

        <div class="form-row">
          <div class="form-group half">
            <label>{{ t('pr_year') }} <span class="required">*</span></label>
            <input 
              v-model="form.birthYear" 
              type="number" 
              :placeholder="t('pr_year_ph')" 
              :disabled="isSubmitting"
            />
          </div>

          <div class="form-group half">
            <label>{{ t('pr_nat') }} <span class="required">*</span></label>
            <input 
              v-model="form.nationality" 
              type="text" 
              :placeholder="t('pr_nat_ph')" 
              :disabled="isSubmitting"
            />
          </div>
        </div>

        <div class="form-row">
          <div class="form-group half">
            <label>{{ t('pr_language') }}</label>
            <select v-model="form.language" :disabled="isSubmitting">
              <option value="vi-VN">{{ t('pr_lang_vi') }}</option>
              <option value="en-US">{{ t('pr_lang_en') }}</option>
            </select>
            <span class="hint">🌐 {{ t('ob_lang_hint') }}</span>
          </div>

          <div class="form-group half">
            <label>{{ t('pr_tone') }}</label>
            <select v-model="form.preferences" :disabled="isSubmitting">
              <option value="Friendly">{{ t('pr_tone_friendly') }}</option>
              <option value="Concise">{{ t('pr_tone_concise') }}</option>
              <option value="Professional">{{ t('pr_tone_prof') }}</option>
            </select>
          </div>
        </div>

        <div class="form-group">
          <label>{{ t('pr_hobbies') }}</label>
          <textarea 
            v-model="form.hobbies" 
            rows="3"
            :placeholder="t('pr_hobbies_ph')" 
            :disabled="isSubmitting"
          ></textarea>
        </div>

        <div v-if="errorMsg" class="error-msg">
          {{ errorMsg }}
        </div>

        <button type="submit" class="submit-btn" :disabled="isSubmitting">
          <span v-if="isSubmitting" class="spinner"></span>
          <span v-else>{{ t('ob_submit') }}</span>
        </button>
      </form>
    </div>
  </div>
</template>

<style scoped>
.onboarding-container {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100vh;
  width: 100vw;
  background: var(--bg-primary);
  color: var(--text-primary);
  overflow-y: auto;
  padding: 20px;
}

.onboarding-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
  padding: 40px;
  box-shadow: var(--shadow-lg);
  max-width: 500px;
  width: 100%;
}

.onboarding-header {
  text-align: center;
  margin-bottom: 30px;
}

.logo-icon {
  font-size: 40px;
  margin-bottom: 10px;
  animation: float 3s ease-in-out infinite;
}

@keyframes float {
  0% { transform: translateY(0px); }
  50% { transform: translateY(-10px); }
  100% { transform: translateY(0px); }
}

.onboarding-title {
  font-size: 28px;
  font-weight: 800;
  margin-bottom: 5px;
  background: linear-gradient(135deg, var(--accent-start), var(--accent-end));
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}

.onboarding-subtitle {
  color: var(--text-secondary);
  font-size: 14px;
}

.onboarding-form {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
  text-align: left;
}

.form-row {
  display: flex;
  gap: 20px;
}

.half {
  flex: 1;
}

label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
}

.required {
  color: #ef4444;
}

.hint {
  font-size: 12px;
  color: var(--text-muted);
}

input, select, textarea {
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  color: var(--text-primary);
  border-radius: var(--radius-md);
  padding: 12px 15px;
  font-size: 14px;
  transition: all 0.2s;
  font-family: inherit;
}

textarea {
  resize: vertical;
}

input:focus, select:focus, textarea:focus {
  outline: none;
  border-color: var(--accent-start);
  box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.2);
}

input:disabled, select:disabled, textarea:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.error-msg {
  color: #ef4444;
  font-size: 13px;
  text-align: center;
  background: rgba(239, 68, 68, 0.1);
  padding: 10px;
  border-radius: var(--radius-sm);
}

.submit-btn {
  background: linear-gradient(135deg, var(--accent-start), var(--accent-end));
  color: white;
  border: none;
  padding: 14px;
  border-radius: var(--radius-md);
  font-size: 16px;
  font-weight: 600;
  cursor: pointer;
  display: flex;
  justify-content: center;
  align-items: center;
  transition: opacity 0.2s, transform 0.1s;
  margin-top: 10px;
}

.submit-btn:hover:not(:disabled) {
  opacity: 0.9;
}

.submit-btn:active:not(:disabled) {
  transform: scale(0.98);
}

.submit-btn:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}

.spinner {
  width: 20px;
  height: 20px;
  border: 2px solid rgba(255,255,255,0.3);
  border-top-color: white;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
