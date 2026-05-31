<script setup lang="ts">
/**
 * VoiceManagementView.vue — Voice status and training control surface
 * [v25] Multi-voice profile support with Edge-TTS voice switching
 */
import { computed, onActivated, onMounted, ref, watch } from "vue";
import { useGateway } from "../../composables/useGateway";

const gateway = useGateway();
const activeProfile = ref("vi-VN-HoaiMyNeural");
const provider = ref("hybrid");
const language = ref("vi-VN");
const sampleRate = ref(16000);
const trainingEnabled = ref(false);
const statusMessage = ref("");
const isBusy = ref(false);
const testingVoice = ref("");

const voice = computed(() => gateway.voiceStatus.value || gateway.configData.value?.voice || {});
const profiles = computed(() => gateway.voiceProfiles.value || []);

const syncFromGateway = () => {
  const v = voice.value as Record<string, unknown>;
  activeProfile.value = String(v.activeProfile ?? "vi-VN-HoaiMyNeural");
  provider.value = String(v.provider ?? "hybrid");
  language.value = String(v.language ?? "vi-VN");
  sampleRate.value = Number(v.sampleRate ?? 16000);
  trainingEnabled.value = Boolean(v.trainingEnabled ?? false);
};

const saveVoice = () => {
  gateway.sendMsg("update_config", {
    voice: {
      enabled: true,
      provider: provider.value,
      activeProfile: activeProfile.value,
      trainingEnabled: trainingEnabled.value,
      sampleRate: sampleRate.value,
      language: language.value,
    },
  });
  statusMessage.value = "✅ Đã lưu cấu hình voice.";
  setTimeout(() => { statusMessage.value = ""; }, 3000);
};

const refreshProfiles = () => {
  gateway.sendMsg("get_voice_profiles");
  gateway.sendMsg("get_voice_status");
};

const startTraining = () => {
  isBusy.value = true;
  gateway.sendMsg("start_voice_training", {
    profile: activeProfile.value,
    language: language.value,
    sampleRate: sampleRate.value,
  });
  statusMessage.value = "⏳ Đã gửi yêu cầu bắt đầu training.";
  isBusy.value = false;
};

const stopTraining = () => {
  gateway.sendMsg("stop_voice_training");
  statusMessage.value = "⏹️ Đã gửi yêu cầu dừng training.";
};

const selectProfile = (id: string) => {
  activeProfile.value = id;
  gateway.sendMsg("select_voice_profile", { profile: id });
  const profileInfo = profiles.value.find((p: any) => (p.id || p.name || p) === id);
  const displayName = profileInfo ? String((profileInfo as any).name || id) : id;
  statusMessage.value = `🎤 Đã chuyển sang giọng: ${displayName}`;
  setTimeout(() => { statusMessage.value = ""; }, 3000);
};

// @ts-ignore
const testVoice = (id: string) => {
  testingVoice.value = id;
  gateway.sendMsg("select_voice_profile", { profile: id });
  // Send a test message through the regular chat so user can hear the voice
  statusMessage.value = `🔊 Đang test giọng: ${id}...`;
  setTimeout(() => { testingVoice.value = ""; statusMessage.value = ""; }, 3000);
};

const getProfileId = (profile: any): string => {
  return String(profile.id ?? profile.name ?? profile);
};

const getProfileLang = (profile: any): string => {
  return String(profile.lang || profile.language || "");
};

const getLangFlag = (lang: string): string => {
  const map: Record<string, string> = {
    "vi-VN": "🇻🇳",
    "en-US": "🇺🇸",
    "ja-JP": "🇯🇵",
    "ko-KR": "🇰🇷",
    "zh-CN": "🇨🇳",
  };
  return map[lang] || "🌐";
};

watch(() => gateway.voiceStatus.value, syncFromGateway, { deep: true, immediate: true });
onMounted(() => { if (!gateway.isConnected.value) gateway.init(); });
onActivated(() => { syncFromGateway(); refreshProfiles(); });
</script>

<template>
  <div class="voice-view animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">🎙️ Voice Management</h1>
      <p class="page-desc">Quản lý voice profile, trạng thái training và cấu hình STT/TTS.</p>
    </div>

    <div class="card section">
      <div class="section-subtitle">Current Voice</div>
      <div class="grid-2">
        <div class="form-group"><label class="form-label">Provider</label><input v-model="provider" class="input" /></div>
        <div class="form-group"><label class="form-label">Active Profile</label><input v-model="activeProfile" class="input" readonly /></div>
        <div class="form-group"><label class="form-label">Language</label><input v-model="language" class="input" /></div>
        <div class="form-group"><label class="form-label">Sample Rate</label><input v-model.number="sampleRate" class="input" type="number" min="8000" max="48000" step="1000" /></div>
      </div>
      <label class="toggle-row"><input type="checkbox" v-model="trainingEnabled" /> Enable training mode</label>
      <div class="actions">
        <button class="btn btn-primary" @click="saveVoice">💾 Lưu voice</button>
        <button class="btn btn-secondary" @click="refreshProfiles">🔄 Refresh profiles</button>
      </div>
    </div>

    <div class="card section">
      <div class="section-subtitle">Training Control</div>
      <div class="actions">
        <button class="btn btn-primary" @click="startTraining" :disabled="isBusy">▶️ Start training</button>
        <button class="btn btn-danger" @click="stopTraining">⏹️ Stop training</button>
      </div>
    </div>

    <div class="card section">
      <div class="section-subtitle">Voice Profiles</div>
      <p class="hint" v-if="profiles.length === 0">Chưa có profile nào. Nhấn Refresh.</p>
      <div class="profile-grid">
        <button
          v-for="profile in profiles"
          :key="getProfileId(profile)"
          class="profile-card"
          :class="{ active: activeProfile === getProfileId(profile), testing: testingVoice === getProfileId(profile) }"
          @click="selectProfile(getProfileId(profile))"
        >
          <div class="profile-header">
            <span class="profile-flag">{{ getLangFlag(getProfileLang(profile)) }}</span>
            <span class="profile-active-badge" v-if="activeProfile === getProfileId(profile)">✓ Active</span>
          </div>
          <strong class="profile-name">{{ String(profile.name ?? profile.id ?? profile) }}</strong>
          <span class="profile-desc">{{ profile.description || getProfileLang(profile) || 'voice profile' }}</span>
          <span class="profile-id">{{ getProfileId(profile) }}</span>
        </button>
      </div>
    </div>

    <transition name="fade">
      <div class="status-toast" v-if="statusMessage">
        {{ statusMessage }}
      </div>
    </transition>
  </div>
</template>

<style scoped>
.voice-view { padding: var(--space-lg); height: 100%; overflow-y: auto; }
.page-header { margin-bottom: var(--space-lg); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }
.section { margin-bottom: var(--space-md); }
.grid-2 { display: grid; grid-template-columns: repeat(2, 1fr); gap: var(--space-sm); }
.actions { display: flex; gap: 10px; flex-wrap: wrap; margin-top: 12px; }
.toggle-row { display: flex; align-items: center; gap: 8px; margin-top: 8px; color: var(--text-secondary); }
.hint { color: var(--text-secondary); font-size: 12px; margin: 8px 0; }

/* Profile Grid */
.profile-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 12px;
  margin-top: 8px;
}

.profile-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
  padding: 14px 16px;
  border-radius: 14px;
  border: 2px solid var(--border-default);
  background: var(--bg-tertiary);
  color: var(--text-primary);
  cursor: pointer;
  transition: all 0.25s ease;
  text-align: left;
  position: relative;
}

.profile-card:hover {
  border-color: var(--color-primary, #7c6fe6);
  transform: translateY(-2px);
  box-shadow: 0 4px 16px rgba(124, 111, 230, 0.15);
}

.profile-card.active {
  border-color: var(--color-primary, #7c6fe6);
  background: linear-gradient(135deg, rgba(124, 111, 230, 0.08), rgba(124, 111, 230, 0.03));
  box-shadow: 0 2px 12px rgba(124, 111, 230, 0.2);
}

.profile-card.testing {
  animation: pulse-test 0.6s ease-in-out infinite alternate;
}

@keyframes pulse-test {
  from { box-shadow: 0 0 0 0 rgba(124, 111, 230, 0.3); }
  to { box-shadow: 0 0 0 8px rgba(124, 111, 230, 0); }
}

.profile-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
}

.profile-flag { font-size: 22px; }

.profile-active-badge {
  font-size: 11px;
  color: #4ade80;
  background: rgba(74, 222, 128, 0.12);
  padding: 2px 8px;
  border-radius: 20px;
  font-weight: 600;
}

.profile-name {
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
}

.profile-desc {
  font-size: 11px;
  color: var(--text-secondary);
  line-height: 1.4;
}

.profile-id {
  font-size: 10px;
  color: var(--text-tertiary, #666);
  font-family: monospace;
  opacity: 0.6;
}

/* Status Toast */
.status-toast {
  position: fixed;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  background: var(--bg-secondary, #1e1e2e);
  border: 1px solid var(--border-default);
  color: var(--text-primary);
  padding: 10px 24px;
  border-radius: 12px;
  font-size: 13px;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.3);
  z-index: 1000;
}

.fade-enter-active, .fade-leave-active { transition: opacity 0.3s, transform 0.3s; }
.fade-enter-from, .fade-leave-to { opacity: 0; transform: translateX(-50%) translateY(10px); }

@media (max-width: 768px) {
  .grid-2 { grid-template-columns: 1fr; }
  .profile-grid { grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); }
}
</style>