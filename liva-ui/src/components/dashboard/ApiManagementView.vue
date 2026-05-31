<script setup lang="ts">
/**
 * ApiManagementView.vue — AI & Integrations Management
 * ====================================================
 * Modern 2-column design:
 * 1. AI Infrastructure & Search Core (Cloud AI, Whisper Cloud, Tavily Search, Weather API)
 * 2. Personal & Social Integrations (Telegram, Zalo OA, Email, Google Workspace)
 */
import { onActivated, onMounted, ref, onUnmounted } from "vue";
import { useGateway } from "../../composables/useGateway";

const gateway = useGateway();

// ==========================================
// CONFIG STATE (.env Vault)
// ==========================================
let rawEnvContent = "";
const isSavingEnv = ref(false);
const envMessage = ref("");

// ─── AI Providers & Search Core ───
// Cloud AI Core
const useCloudAI = ref(false);
const aiBaseUrl = ref("");
const aiApiKey = ref("");
const aiModel = ref("");
const showAiApiKey = ref(false);
const aiProvider = ref("local");

// Whisper Cloud STT
const useWhisperCloud = ref(false);
const whisperCloudUrl = ref("");

// Tavily Web Search
const useTavily = ref(false);
const tavilyKey = ref("");
const showTavilyKey = ref(false);

// Weather API
const useWeather = ref(false);
const weatherKey = ref("");
const showWeatherKey = ref(false);

// ─── Personal & Social Integrations ───
// Telegram
const useTelegram = ref(false);
const telegramToken = ref("");
const telegramAllowedIds = ref("");
const showTelegramToken = ref(false);

// Zalo
const useZalo = ref(false);
const zaloToken = ref("");
const showZaloToken = ref(false);
const zaloAppId = ref("");
const zaloAppSecret = ref("");
const zaloUserId = ref("");
const showZaloSecret = ref(false);

// Email
const useEmail = ref(false);
const emailHost = ref("");
const emailPort = ref("993");
const emailUser = ref("");
const emailPass = ref("");
const showEmailPass = ref(false);

// Google
const useGoogle = ref(false);
const googleSecret = ref("");

// ─── Parsers & Helpers ───
const parseEnvField = (content: string, key: string): string => {
  const match = content.match(new RegExp(`^${key}=(.*)$`, 'm'));
  return match ? match[1].trim() : '';
};

const setEnvField = (content: string, key: string, value: string): string => {
  const regex = new RegExp(`^${key}=.*$`, 'm');
  if (regex.test(content)) {
    return content.replace(regex, `${key}=${value}`);
  } else {
    return content + `\n${key}=${value}`;
  }
};

const onEnvConfigData = (payload: any) => {
  const envContent = payload?.content || '';
  const vaultData = payload?.vault || {};
  rawEnvContent = envContent;

  // Parse AI Provider
  aiProvider.value = parseEnvField(rawEnvContent, 'AI_PROVIDER') || "local";
  useCloudAI.value = aiProvider.value === 'cloud';
  aiBaseUrl.value = vaultData['AI_BASE_URL'] || parseEnvField(rawEnvContent, 'AI_BASE_URL');
  aiApiKey.value = vaultData['AI_API_KEY'] || parseEnvField(rawEnvContent, 'AI_API_KEY');
  aiModel.value = vaultData['AI_MODEL'] || parseEnvField(rawEnvContent, 'AI_MODEL');

  // Parse Whisper Cloud STT
  whisperCloudUrl.value = vaultData['WHISPER_CLOUD_URL'] || parseEnvField(rawEnvContent, 'WHISPER_CLOUD_URL');
  useWhisperCloud.value = whisperCloudUrl.value.length > 0;

  // Parse Tavily Search
  tavilyKey.value = vaultData['TAVILY_API_KEY'] || parseEnvField(rawEnvContent, 'TAVILY_API_KEY');
  useTavily.value = tavilyKey.value.length > 0;

  // Parse Weather API
  weatherKey.value = vaultData['WEATHER_API_KEY'] || parseEnvField(rawEnvContent, 'WEATHER_API_KEY');
  useWeather.value = weatherKey.value.length > 0;

  // Parse Telegram
  useTelegram.value = parseEnvField(rawEnvContent, 'REMOTE_CONTROL_ENABLED') === 'true';
  telegramToken.value = vaultData['TELEGRAM_BOT_TOKEN'] || parseEnvField(rawEnvContent, 'TELEGRAM_BOT_TOKEN');
  telegramAllowedIds.value = vaultData['TELEGRAM_ALLOWED_IDS'] || parseEnvField(rawEnvContent, 'TELEGRAM_ALLOWED_IDS');

  // Parse Zalo
  zaloToken.value = vaultData['ZALO_OA_ACCESS_TOKEN'] || parseEnvField(rawEnvContent, 'ZALO_OA_ACCESS_TOKEN');
  zaloAppId.value = vaultData['ZALO_APP_ID'] || parseEnvField(rawEnvContent, 'ZALO_APP_ID');
  zaloAppSecret.value = vaultData['ZALO_APP_SECRET'] || parseEnvField(rawEnvContent, 'ZALO_APP_SECRET');
  zaloUserId.value = vaultData['ZALO_USER_ID'] || parseEnvField(rawEnvContent, 'ZALO_USER_ID');
  useZalo.value = zaloToken.value.length > 0 || zaloAppId.value.length > 0;

  // Parse Email
  emailHost.value = vaultData['EMAIL_HOST'] || parseEnvField(rawEnvContent, 'EMAIL_HOST');
  emailPort.value = vaultData['EMAIL_PORT'] || parseEnvField(rawEnvContent, 'EMAIL_PORT') || "993";
  emailUser.value = vaultData['EMAIL_USER'] || parseEnvField(rawEnvContent, 'EMAIL_USER');
  emailPass.value = vaultData['EMAIL_PASS'] || parseEnvField(rawEnvContent, 'EMAIL_PASS');
  useEmail.value = emailUser.value.length > 0;

  // Parse Google
  googleSecret.value = vaultData['GOOGLE_CLIENT_SECRET'] || parseEnvField(rawEnvContent, 'GOOGLE_CLIENT_SECRET');
  useGoogle.value = googleSecret.value.length > 0;
};

const loadEnvConfig = () => {
  gateway.sendMsg('get_env_config');
};

const saveEnvConfig = async () => {
  isSavingEnv.value = true;
  envMessage.value = "";
  
  let updatedEnv = rawEnvContent;
  
  // Clean up duplicate vault comments
  updatedEnv = updatedEnv.replace(/^# .*đã được chuyển vào liva_vault\.json.*$/gm, '').replace(/\n\n+/g, '\n');
  
  // Cloud AI Core
  updatedEnv = setEnvField(updatedEnv, 'AI_PROVIDER', useCloudAI.value ? 'cloud' : 'local');
  updatedEnv = setEnvField(updatedEnv, 'AI_BASE_URL', useCloudAI.value ? aiBaseUrl.value : '');
  updatedEnv = setEnvField(updatedEnv, 'AI_API_KEY', useCloudAI.value ? aiApiKey.value : '');
  updatedEnv = setEnvField(updatedEnv, 'AI_MODEL', useCloudAI.value ? aiModel.value : '');

  // Whisper Cloud STT
  updatedEnv = setEnvField(updatedEnv, 'WHISPER_CLOUD_URL', useWhisperCloud.value ? whisperCloudUrl.value : '');

  // Tavily Search
  updatedEnv = setEnvField(updatedEnv, 'TAVILY_API_KEY', useTavily.value ? tavilyKey.value : '');

  // Weather API
  updatedEnv = setEnvField(updatedEnv, 'WEATHER_API_KEY', useWeather.value ? weatherKey.value : '');

  // Telegram
  updatedEnv = setEnvField(updatedEnv, 'REMOTE_CONTROL_ENABLED', useTelegram.value ? 'true' : 'false');
  updatedEnv = setEnvField(updatedEnv, 'TELEGRAM_BOT_TOKEN', useTelegram.value ? telegramToken.value : '');
  updatedEnv = setEnvField(updatedEnv, 'TELEGRAM_ALLOWED_IDS', useTelegram.value ? telegramAllowedIds.value : '');
  updatedEnv = setEnvField(updatedEnv, 'TELEGRAM_CHAT_ID', useTelegram.value ? telegramAllowedIds.value : '');
  updatedEnv = setEnvField(updatedEnv, 'TELEGRAM_ADMIN_ID', useTelegram.value ? telegramAllowedIds.value : '');

  // Zalo
  updatedEnv = setEnvField(updatedEnv, 'ZALO_OA_ACCESS_TOKEN', useZalo.value ? zaloToken.value : '');
  updatedEnv = setEnvField(updatedEnv, 'ZALO_APP_ID', useZalo.value ? zaloAppId.value : '');
  updatedEnv = setEnvField(updatedEnv, 'ZALO_APP_SECRET', useZalo.value ? zaloAppSecret.value : '');
  updatedEnv = setEnvField(updatedEnv, 'ZALO_USER_ID', useZalo.value ? zaloUserId.value : '');

  // Email
  updatedEnv = setEnvField(updatedEnv, 'EMAIL_HOST', useEmail.value ? emailHost.value : '');
  updatedEnv = setEnvField(updatedEnv, 'EMAIL_PORT', useEmail.value ? emailPort.value : '');
  updatedEnv = setEnvField(updatedEnv, 'EMAIL_USER', useEmail.value ? emailUser.value : '');
  updatedEnv = setEnvField(updatedEnv, 'EMAIL_PASS', useEmail.value ? emailPass.value : '');

  // Google
  updatedEnv = setEnvField(updatedEnv, 'GOOGLE_CLIENT_SECRET', useGoogle.value ? googleSecret.value : '');

  // Send to backend
  gateway.sendMsg('save_env_config', { content: updatedEnv });
  rawEnvContent = updatedEnv;
  envMessage.value = "✅ Đã lưu cấu hình và đang khởi động lại Gateway...";
  isSavingEnv.value = false;
};

const isRestarting = ref(false);

// Lifecycle
onMounted(() => { 
  if (!gateway.isConnected.value) gateway.init(); 
  gateway.onEnvConfigData(onEnvConfigData);
  loadEnvConfig();
});

onActivated(() => {
  gateway.onEnvConfigData(onEnvConfigData);
  loadEnvConfig();
});

onUnmounted(() => {
  gateway.offEnvConfigData();
});
</script>

<template>
  <div class="api-view animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">🔌 Tích hợp & Bảo mật</h1>
      <p class="page-desc">Quản lý các kết nối API mở rộng được lưu trữ và mã hóa an toàn trong Vault.</p>
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