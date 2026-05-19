<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { useGateway } from '../../composables/useGateway';
import { useI18n } from '../../composables/useI18n';

const gateway = useGateway();
const { t } = useI18n();

const form = ref({
  name: '',
  birthYear: '',
  nationality: '',
  language: '',
  hobbies: '',
  preferences: 'Friendly'
});

const isSaving = ref(false);
const saveMessage = ref('');

onMounted(() => {
  if (gateway.userProfile.value) {
    Object.assign(form.value, gateway.userProfile.value);
  } else {
    gateway.sendMsg('get_user_profile');
  }
});

watch(() => gateway.userProfile.value, (newVal) => {
  if (newVal) {
    Object.assign(form.value, newVal);
  }
}, { deep: true });

const saveProfile = async () => {
  isSaving.value = true;
  saveMessage.value = '';

  gateway.saveUserProfile({ ...form.value });

  // Simulate network delay for UX
  await new Promise(r => setTimeout(r, 600));
  isSaving.value = false;
  saveMessage.value = t('pr_saved');
  setTimeout(() => { saveMessage.value = ''; }, 3000);
};

// Live-sync: When user changes language dropdown, immediately update
// the gateway profile so useI18n re-computes and all labels switch in real-time
watch(() => form.value.language, (newLang, oldLang) => {
  if (newLang && newLang !== oldLang) {
    gateway.saveUserProfile({ ...form.value, language: newLang });
  }
});
</script>

<template>
  <div class="profile-settings animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">👤 {{ t('pr_title') }}</h1>
      <p class="page-desc">{{ t('pr_desc') }}</p>
    </div>

    <div class="card settings-section animate-fadeIn">
      <div class="form-group">
        <label class="form-label">{{ t('pr_name') }} <span class="required">*</span></label>
        <input 
          v-model="form.name" 
          type="text" 
          class="input"
          :placeholder="t('pr_name_ph')" 
          :disabled="isSaving"
        />
      </div>

      <div class="grid-2">
        <div class="form-group">
          <label class="form-label">{{ t('pr_year') }} <span class="required">*</span></label>
          <input 
            v-model="form.birthYear" 
            type="number" 
            class="input"
            :placeholder="t('pr_year_ph')" 
            :disabled="isSaving"
          />
        </div>

        <div class="form-group">
          <label class="form-label">{{ t('pr_nat') }} <span class="required">*</span></label>
          <input 
            v-model="form.nationality" 
            type="text" 
            class="input"
            :placeholder="t('pr_nat_ph')" 
            :disabled="isSaving"
          />
        </div>
      </div>

      <div class="grid-2">
        <div class="form-group">
          <label class="form-label">{{ t('pr_language') }}</label>
          <select v-model="form.language" class="input" :disabled="isSaving">
            <option value="vi-VN">{{ t('pr_lang_vi') }}</option>
            <option value="en-US">{{ t('pr_lang_en') }}</option>
          </select>
        </div>

        <div class="form-group">
          <label class="form-label">{{ t('pr_tone') }}</label>
          <select v-model="form.preferences" class="input" :disabled="isSaving">
            <option value="Friendly">{{ t('pr_tone_friendly') }}</option>
            <option value="Concise">{{ t('pr_tone_concise') }}</option>
            <option value="Professional">{{ t('pr_tone_prof') }}</option>
          </select>
        </div>
      </div>

      <div class="form-group">
        <label class="form-label">{{ t('pr_hobbies') }}</label>
        <textarea 
          v-model="form.hobbies" 
          rows="3"
          class="input textarea"
          :placeholder="t('pr_hobbies_ph')" 
          :disabled="isSaving"
        ></textarea>
      </div>
    </div>

    <!-- Save Button -->
    <div class="save-bar">
      <button class="btn btn-primary" @click="saveProfile" :disabled="isSaving || !form.name || !form.birthYear || !form.nationality">
        {{ isSaving ? t('pr_saving') : t('pr_save') }}
      </button>
      <span v-if="saveMessage" class="save-message animate-fadeIn">{{ saveMessage }}</span>
    </div>
  </div>
</template>

<style scoped>
.profile-settings {
  padding: var(--space-lg);
  overflow-y: auto;
  height: 100%;
}

.page-header { margin-bottom: var(--space-lg); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }

.settings-section { margin-bottom: var(--space-md); }

.form-group {
  margin-bottom: var(--space-md);
}

.form-label {
  display: block;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 6px;
}

.required {
  color: #ef4444;
}

.input {
  width: 100%;
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  color: var(--text-primary);
  border-radius: var(--radius-md);
  padding: 10px 14px;
  font-size: 14px;
  transition: all 0.2s;
  font-family: inherit;
}

.textarea {
  resize: vertical;
}

.input:focus {
  outline: none;
  border-color: var(--accent-start);
  box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.2);
}

.input:disabled {
  opacity: 0.6;
  cursor: not-allowed;
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
</style>
