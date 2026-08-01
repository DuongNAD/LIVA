<script setup lang="ts">
/**
 * ApiManagementView.vue — public integration settings plus write-only Stronghold inputs.
 * Stored secrets are never loaded back into the renderer.
 */
import { computed, onActivated, onMounted, ref } from "vue";
import { useGateway } from "../../composables/useGateway";
import { detectPlatform } from "../../platform";

const gateway = useGateway();
const platform = detectPlatform();
const isSavingEnv = ref(false);
const envMessage = ref("");

const useCloudAI = ref(false);
const aiBaseUrl = ref("");
const aiApiKey = ref("");
const aiModel = ref("");
const showAiApiKey = ref(false);
const aiProvider = ref("local");

const useWhisperCloud = ref(false);
const whisperCloudUrl = ref("");
const useTavily = ref(false);
const tavilyKey = ref("");
const showTavilyKey = ref(false);
const useWeather = ref(false);
const weatherKey = ref("");
const showWeatherKey = ref(false);

const useTelegram = ref(false);
const telegramToken = ref("");
const telegramAllowedIds = ref("");
const showTelegramToken = ref(false);
const useZalo = ref(false);
const zaloToken = ref("");
const showZaloToken = ref(false);
const zaloAppId = ref("");
const zaloAppSecret = ref("");
const zaloUserId = ref("");
const showZaloSecret = ref(false);
const useEmail = ref(false);
const emailHost = ref("");
const emailPort = ref("993");
const emailUser = ref("");
const emailPass = ref("");
const showEmailPass = ref(false);
const useGoogle = ref(false);
const googleSecret = ref("");

const secretPresence = ref<Record<string, boolean>>({});
const configuredSecretCount = computed(
  () => Object.values(secretPresence.value).filter(Boolean).length,
);

const SECRET_KEYS = [
  "ai/cloud_api_key",
  "search/tavily_api_key",
  "weather/api_key",
  "telegram/bot_token",
  "zalo/access_token",
  "zalo/app_secret",
  "email/password",
  "google/client_secret",
] as const;

const loadSecureConfig = async () => {
  const config = gateway.configData.value as unknown as {
    ai?: Record<string, unknown>;
    integrations?: Record<string, unknown>;
  };
  const ai = config?.ai || {};
  const integrations = config?.integrations || {};

  aiProvider.value = String(ai.provider || "local");
  useCloudAI.value = aiProvider.value === "cloud";
  aiBaseUrl.value = String(ai.cloudBaseUrl || "");
  aiModel.value = String(ai.cloudModel || "");
  whisperCloudUrl.value = String(integrations.whisperCloudUrl || "");
  useWhisperCloud.value = Boolean(integrations.whisperCloudEnabled);
  useTavily.value = Boolean(integrations.tavilyEnabled);
  useWeather.value = Boolean(integrations.weatherEnabled);
  useTelegram.value = Boolean(integrations.telegramEnabled);
  telegramAllowedIds.value = String(integrations.telegramAllowedIds || "");
  useZalo.value = Boolean(integrations.zaloEnabled);
  zaloAppId.value = String(integrations.zaloAppId || "");
  zaloUserId.value = String(integrations.zaloUserId || "");
  useEmail.value = Boolean(integrations.emailEnabled);
  emailHost.value = String(integrations.emailHost || "");
  emailPort.value = String(integrations.emailPort || "993");
  emailUser.value = String(integrations.emailUser || "");
  useGoogle.value = Boolean(integrations.googleEnabled);

  const pairs = await Promise.all(
    SECRET_KEYS.map(async key => [key, await platform.hasVaultSecret(key)] as const),
  );
  secretPresence.value = Object.fromEntries(pairs);
};

const saveEnvConfig = async () => {
  isSavingEnv.value = true;
  envMessage.value = "";

  try {
    const pendingSecrets: Array<[string, string]> = [
      ["ai/cloud_api_key", aiApiKey.value],
      ["search/tavily_api_key", tavilyKey.value],
      ["weather/api_key", weatherKey.value],
      ["telegram/bot_token", telegramToken.value],
      ["zalo/access_token", zaloToken.value],
      ["zalo/app_secret", zaloAppSecret.value],
      ["email/password", emailPass.value],
      ["google/client_secret", googleSecret.value],
    ];
    for (const [key, value] of pendingSecrets) {
      if (value) {
        await platform.storeVaultSecret(key, value);
        secretPresence.value[key] = true;
      }
    }

    gateway.updateConfig({
      ai: {
        provider: useCloudAI.value ? "cloud" : "local",
        cloudBaseUrl: aiBaseUrl.value,
        cloudModel: aiModel.value,
        localModelsDir: gateway.configData.value.ai?.localModelsDir || "",
        routerModel: gateway.configData.value.ai?.routerModel || "",
        expertModel: gateway.configData.value.ai?.expertModel || "",
        temperature: gateway.configData.value.ai?.temperature ?? 0.3,
        maxTokens: gateway.configData.value.ai?.maxTokens ?? 2048,
        topP: gateway.configData.value.ai?.topP ?? 0.9,
      },
      integrations: {
        whisperCloudEnabled: useWhisperCloud.value,
        whisperCloudUrl: whisperCloudUrl.value,
        tavilyEnabled: useTavily.value,
        weatherEnabled: useWeather.value,
        telegramEnabled: useTelegram.value,
        telegramAllowedIds: telegramAllowedIds.value,
        zaloEnabled: useZalo.value,
        zaloAppId: zaloAppId.value,
        zaloUserId: zaloUserId.value,
        emailEnabled: useEmail.value,
        emailHost: emailHost.value,
        emailPort: emailPort.value,
        emailUser: emailUser.value,
        googleEnabled: useGoogle.value,
      },
    });

    aiApiKey.value = "";
    tavilyKey.value = "";
    weatherKey.value = "";
    telegramToken.value = "";
    zaloToken.value = "";
    zaloAppSecret.value = "";
    emailPass.value = "";
    googleSecret.value = "";
    envMessage.value = "✅ Đã lưu cấu hình công khai và secret vào Stronghold.";
  } catch (error) {
    envMessage.value = "❌ Không thể lưu cấu hình: " + String(error);
  } finally {
    isSavingEnv.value = false;
  }
};

const isRestarting = ref(false);

onMounted(() => {
  if (!gateway.isConnected.value) gateway.init();
  void loadSecureConfig();
});

onActivated(() => {
  void loadSecureConfig();
});
</script>

<template>
  <div class="api-view animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">🔌 Tích hợp & Bảo mật</h1>
      <p class="page-desc">
        Quản lý cấu hình công khai và secret write-only trong Stronghold.
        Đã cấu hình {{ configuredSecretCount }}/{{ SECRET_KEYS.length }} secret.
      </p>
    </div>

    <div class="tab-content animate-fadeIn">
      <!-- 2-Column Responsive Layout -->
      <div class="grid-2 mt-4">
        
        <!-- Column 1: AI Infrastructure & Search Core -->
        <div class="col-section">
          <h2 class="column-title">🧠 Hạ tầng AI & Tìm kiếm (Core Engine)</h2>
          <div class="flex flex-col gap-4">
            
            <!-- Cloud AI Core -->
            <div class="card section integration-card" :class="{'active': useCloudAI}">
              <label class="toggle-label cursor-pointer mb-2">
                <input type="checkbox" v-model="useCloudAI" class="form-checkbox h-5 w-5" />
                <div class="flex flex-col">
                  <span class="section-subtitle mb-0">Cloud AI Core (Gemini/OpenAI)</span>
                  <span class="text-xs text-gray-400">Sử dụng mô hình đám mây làm mô hình chính hoặc làm dự phòng.</span>
                </div>
              </label>

              <div v-if="useCloudAI" class="integration-form animate-fadeIn mt-4">
                <div class="form-group">
                  <label class="form-label">Base URL (Endpoint API)</label>
                  <input v-model="aiBaseUrl" class="input" placeholder="Ví dụ: https://generativelanguage.googleapis.com/v1beta/openai" />
                </div>
                <div class="form-group">
                  <label class="form-label">Cloud API Key</label>
                  <div class="input-with-toggle">
                    <input v-model="aiApiKey" :type="showAiApiKey ? 'text' : 'password'" class="input" placeholder="AI API Key bảo mật..." />
                    <button class="btn btn-secondary btn-sm" @click="showAiApiKey = !showAiApiKey">{{ showAiApiKey ? 'Ẩn' : 'Hiện' }}</button>
                  </div>
                </div>
                <div class="form-group">
                  <label class="form-label">AI Model</label>
                  <input v-model="aiModel" class="input" placeholder="Ví dụ: gemini-2.5-flash" />
                </div>
              </div>
            </div>

            <!-- Whisper Cloud STT -->
            <div class="card section integration-card" :class="{'active': useWhisperCloud}">
              <label class="toggle-label cursor-pointer mb-2">
                <input type="checkbox" v-model="useWhisperCloud" class="form-checkbox h-5 w-5" />
                <div class="flex flex-col">
                  <span class="section-subtitle mb-0">Whisper Cloud Speech-to-Text</span>
                  <span class="text-xs text-gray-400">Giải phóng 100% VRAM GPU local bằng cách xử lý giọng nói trên đám mây.</span>
                </div>
              </label>

              <div v-if="useWhisperCloud" class="integration-form animate-fadeIn mt-4">
                <div class="form-group">
                  <label class="form-label">Whisper API URL</label>
                  <input v-model="whisperCloudUrl" class="input" placeholder="Ví dụ: https://api.groq.com/openai/v1/audio/transcriptions" />
                </div>
              </div>
            </div>

            <!-- Tavily Search -->
            <div class="card section integration-card" :class="{'active': useTavily}">
              <label class="toggle-label cursor-pointer mb-2">
                <input type="checkbox" v-model="useTavily" class="form-checkbox h-5 w-5" />
                <div class="flex flex-col">
                  <span class="section-subtitle mb-0">Tavily Web Search</span>
                  <span class="text-xs text-gray-400">Cho phép AI chủ động tìm kiếm và tổng hợp thông tin từ Internet.</span>
                </div>
              </label>

              <div v-if="useTavily" class="integration-form animate-fadeIn mt-4">
                <div class="form-group">
                  <label class="form-label">Tavily API Key (Lấy từ tavily.com)</label>
                  <div class="input-with-toggle">
                    <input v-model="tavilyKey" :type="showTavilyKey ? 'text' : 'password'" class="input" placeholder="tvly-..." />
                    <button class="btn btn-secondary btn-sm" @click="showTavilyKey = !showTavilyKey">{{ showTavilyKey ? 'Ẩn' : 'Hiện' }}</button>
                  </div>
                </div>
              </div>
            </div>

            <!-- Weather API -->
            <div class="card section integration-card" :class="{'active': useWeather}">
              <label class="toggle-label cursor-pointer mb-2">
                <input type="checkbox" v-model="useWeather" class="form-checkbox h-5 w-5" />
                <div class="flex flex-col">
                  <span class="section-subtitle mb-0">Weather API</span>
                  <span class="text-xs text-gray-400">Cung cấp thông tin thời tiết chính xác và dự báo cho các địa phương.</span>
                </div>
              </label>

              <div v-if="useWeather" class="integration-form animate-fadeIn mt-4">
                <div class="form-group">
                  <label class="form-label">Weather API Key (Lấy từ weatherapi.com)</label>
                  <div class="input-with-toggle">
                    <input v-model="weatherKey" :type="showWeatherKey ? 'text' : 'password'" class="input" placeholder="Khóa API Weather..." />
                    <button class="btn btn-secondary btn-sm" @click="showWeatherKey = !showWeatherKey">{{ showWeatherKey ? 'Ẩn' : 'Hiện' }}</button>
                  </div>
                </div>
              </div>
            </div>

          </div>
        </div>

        <!-- Column 2: Personal & Social Integrations -->
        <div class="col-section">
          <h2 class="column-title">💬 Tài khoản & Tích hợp (Integrations)</h2>
          <div class="flex flex-col gap-4">
            
            <!-- Telegram -->
            <div class="card section integration-card" :class="{'active': useTelegram}">
              <label class="toggle-label cursor-pointer mb-2">
                <input type="checkbox" v-model="useTelegram" class="form-checkbox h-5 w-5" />
                <div class="flex flex-col">
                  <span class="section-subtitle mb-0">Telegram Remote Control</span>
                  <span class="text-xs text-gray-400">Điều khiển máy tính và nhận báo cáo bảo mật từ LIVA từ xa qua Telegram.</span>
                </div>
              </label>
              
              <div v-if="useTelegram" class="integration-form animate-fadeIn mt-4">
                <div class="form-group">
                  <label class="form-label">Telegram Bot Token (Lấy từ @BotFather)</label>
                  <div class="input-with-toggle">
                    <input v-model="telegramToken" :type="showTelegramToken ? 'text' : 'password'" class="input" placeholder="123456789:AA..." />
                    <button class="btn btn-secondary btn-sm" @click="showTelegramToken = !showTelegramToken">{{ showTelegramToken ? 'Ẩn' : 'Hiện' }}</button>
                  </div>
                </div>
                <div class="form-group">
                  <label class="form-label">Allowed Chat IDs (Bảo mật điều khiển)</label>
                  <input v-model="telegramAllowedIds" class="input" placeholder="Ví dụ: 123456789" />
                  <span class="form-help mt-1">Gửi lệnh "/start" cho Bot của bạn trên Telegram để lấy Chat ID.</span>
                </div>
              </div>
            </div>

            <!-- Zalo -->
            <div class="card section integration-card" :class="{'active': useZalo}">
              <label class="toggle-label cursor-pointer mb-2">
                <input type="checkbox" v-model="useZalo" class="form-checkbox h-5 w-5" />
                <div class="flex flex-col">
                  <span class="section-subtitle mb-0">Zalo OA (Tự động hóa tin nhắn)</span>
                  <span class="text-xs text-gray-400">Cho phép LIVA gửi báo cáo đẩy và thông báo khẩn cấp qua Zalo OA.</span>
                </div>
              </label>

              <div v-if="useZalo" class="integration-form animate-fadeIn mt-4">
                <div class="form-group">
                  <label class="form-label">Zalo Bot Token / OA Access Token</label>
                  <div class="input-with-toggle">
                    <input v-model="zaloToken" :type="showZaloToken ? 'text' : 'password'" class="input" placeholder="Token hoặc Bot Token (dạng bot_id:secret)..." />
                    <button class="btn btn-secondary btn-sm" @click="showZaloToken = !showZaloToken">{{ showZaloToken ? 'Ẩn' : 'Hiện' }}</button>
                  </div>
                </div>
                <div class="grid-2">
                  <div class="form-group">
                    <label class="form-label">Zalo App ID (Hệ OA cũ)</label>
                    <input v-model="zaloAppId" class="input" placeholder="App ID..." />
                  </div>
                  <div class="form-group">
                    <label class="form-label">Zalo App Secret (Hệ OA cũ)</label>
                    <div class="input-with-toggle">
                      <input v-model="zaloAppSecret" :type="showZaloSecret ? 'text' : 'password'" class="input" placeholder="App Secret..." />
                      <button class="btn btn-secondary btn-sm" @click="showZaloSecret = !showZaloSecret">{{ showZaloSecret ? 'Ẩn' : 'Hiện' }}</button>
                    </div>
                  </div>
                </div>
                <div class="form-group">
                  <label class="form-label">Zalo User ID nhận tin</label>
                  <input v-model="zaloUserId" class="input" placeholder="Có thể để trống để tự phát hiện (Auto-detect) khi nhắn tin cho Bot..." />
                </div>
              </div>
            </div>

            <!-- Email IMAP/SMTP -->
            <div class="card section integration-card" :class="{'active': useEmail}">
              <label class="toggle-label cursor-pointer mb-2">
                <input type="checkbox" v-model="useEmail" class="form-checkbox h-5 w-5" />
                <div class="flex flex-col">
                  <span class="section-subtitle mb-0">Email IMAP/SMTP</span>
                  <span class="text-xs text-gray-400">Cho phép LIVA quét hộp thư quan trọng và hỗ trợ gửi email phản hồi.</span>
                </div>
              </label>

              <div v-if="useEmail" class="integration-form animate-fadeIn mt-4">
                <div class="grid-2">
                  <div class="form-group">
                    <label class="form-label">Email Host</label>
                    <input v-model="emailHost" class="input" placeholder="imap.gmail.com" />
                  </div>
                  <div class="form-group">
                    <label class="form-label">Email Port</label>
                    <input v-model="emailPort" class="input" placeholder="993" />
                  </div>
                </div>
                <div class="grid-2 mt-3">
                  <div class="form-group">
                    <label class="form-label">Email Address</label>
                    <input v-model="emailUser" class="input" placeholder="user@gmail.com" />
                  </div>
                  <div class="form-group">
                    <label class="form-label">App Password</label>
                    <div class="input-with-toggle">
                      <input v-model="emailPass" :type="showEmailPass ? 'text' : 'password'" class="input" placeholder="abcd efgh ijkl mnop" />
                      <button class="btn btn-secondary btn-sm" @click="showEmailPass = !showEmailPass">{{ showEmailPass ? 'Ẩn' : 'Hiện' }}</button>
                    </div>
                  </div>
                </div>
                <p class="form-help mt-2">Đối với Gmail, bắt buộc sử dụng <b>Mật khẩu Ứng dụng (App Password)</b>, không dùng mật khẩu gốc.</p>
              </div>
            </div>

            <!-- Google APIs -->
            <div class="card section integration-card" :class="{'active': useGoogle}">
              <label class="toggle-label cursor-pointer mb-2">
                <input type="checkbox" v-model="useGoogle" class="form-checkbox h-5 w-5" />
                <div class="flex flex-col">
                  <span class="section-subtitle mb-0">Google OAuth2 Workspace</span>
                  <span class="text-xs text-gray-400">Liên kết tài liệu Google Docs, Drive và lịch Google Calendar.</span>
                </div>
              </label>

              <div v-if="useGoogle" class="integration-form animate-fadeIn mt-4">
                <div class="form-group">
                  <label class="form-label">Google Client Secret</label>
                  <input v-model="googleSecret" class="input" placeholder="GOCSPX-..." />
                </div>
                <p class="form-help text-warning mt-2">Lưu ý: Bạn cũng cần tải file <b>credentials.json</b> đặt vào thư mục gốc của LIVA Gateway (liva-gateway).</p>
              </div>
            </div>

          </div>
        </div>

      </div>

      <!-- Action Buttons -->
      <div class="actions mt-6">
        <button class="btn btn-primary" @click="saveEnvConfig" :disabled="isSavingEnv || isRestarting">{{ isSavingEnv ? 'Đang lưu...' : 'Lưu cấu hình & Khởi động lại' }}</button>
        <span class="hint font-medium" v-if="envMessage" :class="envMessage.includes('Lỗi') ? 'text-red' : 'text-green'">{{ envMessage }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.api-view { padding: var(--space-lg); height: 100%; overflow-y: auto; }
.page-header { margin-bottom: var(--space-lg); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }
.section { margin-bottom: var(--space-md); }
.section-subtitle { margin-bottom: 2px; font-weight: 700; color: var(--text-primary); font-size: 13px; }

/* 2-Column Sections */
.col-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
}

.column-title {
  font-size: 13px;
  font-weight: 800;
  text-transform: uppercase;
  color: var(--accent-start);
  letter-spacing: 0.5px;
  margin-bottom: var(--space-sm);
  border-left: 3px solid var(--accent-start);
  padding-left: 8px;
}

/* Integration Cards */
.integration-card { 
  border: 1px solid var(--border-default); 
  background: var(--bg-secondary);
  transition: all var(--transition-normal); 
  border-radius: var(--radius-md);
}
.integration-card:hover {
  border-color: var(--text-muted);
}
.integration-card.active { 
  border-color: var(--accent-start); 
  background: var(--bg-secondary); 
  box-shadow: var(--shadow-sm);
}
.integration-form { 
  padding-top: 16px; 
  border-top: 1px solid var(--border-subtle); 
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}
.toggle-label { display: flex; align-items: flex-start; gap: 12px; }
.form-checkbox { 
  width: 18px; 
  height: 18px; 
  margin-top: 2px;
  accent-color: var(--accent-start); 
  cursor: pointer;
}

/* Form Styling */
.form-group {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}
.form-label {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  color: var(--text-secondary);
  letter-spacing: 0.5px;
}

.grid-2 { display: grid; grid-template-columns: repeat(2, 1fr); gap: var(--space-md); }
.input-with-toggle { display: flex; gap: 8px; }
.input-with-toggle .input { flex: 1; }
.actions { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
.hint { font-size: 13px; }
.text-green { color: #22c55e; }
.text-red { color: #ef4444; }
.text-warning { color: #d97706; font-size: 12px; }

@media (max-width: 1024px) { .grid-2 { grid-template-columns: 1fr; } }
</style>
