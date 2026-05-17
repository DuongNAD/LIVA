<script setup lang="ts">
/**
 * ApiManagementView.vue — AI & Integrations Management
 * ====================================================
 * Modern 2-tab design:
 * 1. AI Provider & Inference (Local/Cloud LLM)
 * 2. Integrations & Vault (.env secrets for Zalo, Telegram)
 */
import { computed, onActivated, onMounted, ref, watch } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";

const electronAPI = (globalThis as any).electronAPI;

const gateway = useGateway();
const { t } = useI18n();



// ==========================================
// 2. INTEGRATIONS STATE (.env Vault)
// ==========================================
let rawEnvContent = "";
const isSavingEnv = ref(false);
const envMessage = ref("");

// Telegram
const useTelegram = ref(false);
const telegramToken = ref("");
const telegramAllowedIds = ref("");
const showTelegramToken = ref(false);

// Zalo
const useZalo = ref(false);
const zaloAppId = ref("");
const zaloAppSecret = ref("");
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

// Tavily
const useTavily = ref(false);
const tavilyKey = ref("");
const showTavilyKey = ref(false);

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

const loadEnvConfig = async () => {
  let vaultData: Record<string, string> = {};
  if (electronAPI?.getVaultConfig) {
    vaultData = await electronAPI.getVaultConfig() || {};
  }

  if (electronAPI?.getEnvConfig) {
    rawEnvContent = await electronAPI.getEnvConfig();
    
    // Parse Telegram
    useTelegram.value = parseEnvField(rawEnvContent, 'REMOTE_CONTROL_ENABLED') === 'true';
    telegramToken.value = parseEnvField(rawEnvContent, 'TELEGRAM_BOT_TOKEN');
    telegramAllowedIds.value = parseEnvField(rawEnvContent, 'TELEGRAM_ALLOWED_IDS');

    // Parse Zalo
    zaloAppId.value = parseEnvField(rawEnvContent, 'ZALO_APP_ID');
    zaloAppSecret.value = parseEnvField(rawEnvContent, 'ZALO_APP_SECRET');
    useZalo.value = zaloAppId.value.length > 0;

    // Parse Email (mix of .env and Vault)
    emailHost.value = vaultData['EMAIL_HOST'] || parseEnvField(rawEnvContent, 'EMAIL_HOST');
    emailPort.value = parseEnvField(rawEnvContent, 'EMAIL_PORT') || "993";
    emailUser.value = vaultData['EMAIL_USER'] || parseEnvField(rawEnvContent, 'EMAIL_USER');
    emailPass.value = vaultData['EMAIL_PASS'] || parseEnvField(rawEnvContent, 'EMAIL_PASS');
    useEmail.value = emailUser.value.length > 0;

    // Parse Google
    googleSecret.value = parseEnvField(rawEnvContent, 'GOOGLE_CLIENT_SECRET');
    useGoogle.value = googleSecret.value.length > 0;

    // Parse Tavily
    tavilyKey.value = vaultData['TAVILY_API_KEY'] || parseEnvField(rawEnvContent, 'TAVILY_API_KEY');
    useTavily.value = tavilyKey.value.length > 0;
  }
};

const saveEnvConfig = async () => {
  if (!electronAPI?.saveEnvConfig) return;
  
  isSavingEnv.value = true;
  envMessage.value = "";
  
  // Patch content securely
  let updatedEnv = rawEnvContent;
  
  // Clean up duplicate vault comments so the file doesn't grow infinitely
  updatedEnv = updatedEnv.replace(/^# .*đã được chuyển vào liva_vault\.json.*$/gm, '').replace(/\n\n+/g, '\n');
  
  // Telegram
  updatedEnv = setEnvField(updatedEnv, 'REMOTE_CONTROL_ENABLED', useTelegram.value ? 'true' : 'false');
  updatedEnv = setEnvField(updatedEnv, 'TELEGRAM_BOT_TOKEN', useTelegram.value ? telegramToken.value : '');
  updatedEnv = setEnvField(updatedEnv, 'TELEGRAM_ALLOWED_IDS', useTelegram.value ? telegramAllowedIds.value : '');
  updatedEnv = setEnvField(updatedEnv, 'TELEGRAM_CHAT_ID', useTelegram.value ? telegramAllowedIds.value : ''); // Sync chat ID
  updatedEnv = setEnvField(updatedEnv, 'TELEGRAM_ADMIN_ID', useTelegram.value ? telegramAllowedIds.value : ''); // Sync admin ID

  // Zalo
  updatedEnv = setEnvField(updatedEnv, 'ZALO_APP_ID', useZalo.value ? zaloAppId.value : '');
  updatedEnv = setEnvField(updatedEnv, 'ZALO_APP_SECRET', useZalo.value ? zaloAppSecret.value : '');

  // Email
  updatedEnv = setEnvField(updatedEnv, 'EMAIL_HOST', useEmail.value ? emailHost.value : '');
  updatedEnv = setEnvField(updatedEnv, 'EMAIL_PORT', useEmail.value ? emailPort.value : '');
  updatedEnv = setEnvField(updatedEnv, 'EMAIL_USER', useEmail.value ? emailUser.value : '');
  updatedEnv = setEnvField(updatedEnv, 'EMAIL_PASS', useEmail.value ? emailPass.value : '');

  // Google
  updatedEnv = setEnvField(updatedEnv, 'GOOGLE_CLIENT_SECRET', useGoogle.value ? googleSecret.value : '');

  // Tavily
  updatedEnv = setEnvField(updatedEnv, 'TAVILY_API_KEY', useTavily.value ? tavilyKey.value : '');

  const res = await electronAPI.saveEnvConfig(updatedEnv);
  if (res?.success) {
    envMessage.value = "✅ Đã lưu cấu hình. Hãy khởi động lại hệ thống để áp dụng!";
    rawEnvContent = updatedEnv; // sync local
  } else {
    envMessage.value = "❌ Lỗi: " + (res?.error || "Không thể lưu");
  }
  isSavingEnv.value = false;
};

// ==========================================
// LIFECYCLE
// ==========================================
onMounted(() => { 
  if (!gateway.isConnected.value) gateway.init(); 
  loadEnvConfig();
});
onActivated(() => {
  loadEnvConfig();
});
</script>

<template>
  <div class="api-view animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">🔌 Tích hợp & Bảo mật</h1>
      <p class="page-desc">Quản lý các kết nối API mở rộng (Telegram, Zalo, Google...) được lưu trữ an toàn trong Vault.</p>
    </div>

    <!-- ======================================================== -->
    <!-- INTEGRATIONS & VAULT -->
    <!-- ======================================================== -->
    <div class="tab-content animate-fadeIn">
      
      <!-- Telegram -->
      <div class="card section integration-card" :class="{'active': useTelegram}">
        <label class="toggle-label cursor-pointer mb-2">
            <input type="checkbox" v-model="useTelegram" class="form-checkbox h-5 w-5" />
            <div class="flex flex-col">
                <span class="section-subtitle mb-0">Telegram Remote Control</span>
                <span class="text-xs text-gray-400">Cho phép điều khiển PC và tương tác với LIVA từ xa qua Telegram.</span>
            </div>
        </label>
        
        <div v-if="useTelegram" class="integration-form animate-fadeIn mt-4">
          <div class="form-group">
            <label class="form-label">Telegram Bot Token (Lấy từ @BotFather)</label>
            <div class="input-with-toggle">
              <input v-model="telegramToken" :type="showTelegramToken ? 'text' : 'password'" class="input" placeholder="123456789:AA..." />
              <button class="btn btn-secondary" @click="showTelegramToken = !showTelegramToken">{{ showTelegramToken ? 'Ẩn' : 'Hiện' }}</button>
            </div>
          </div>
          <div class="form-group">
            <label class="form-label">Allowed Chat IDs (Bảo mật: Chỉ Chat ID này mới được phép điều khiển)</label>
            <input v-model="telegramAllowedIds" class="input" placeholder="Ví dụ: 123456789" />
            <span class="form-help mt-1">Để lấy Chat ID, hãy nhắn lệnh "/start" cho bot của bạn trên Telegram.</span>
          </div>
        </div>
      </div>

      <!-- Tavily Search -->
      <div class="card section integration-card mt-4" :class="{'active': useTavily}">
        <label class="toggle-label cursor-pointer mb-2">
            <input type="checkbox" v-model="useTavily" class="form-checkbox h-5 w-5" />
            <div class="flex flex-col">
                <span class="section-subtitle mb-0">Tavily Web Search</span>
                <span class="text-xs text-gray-400">Cho phép AI tìm kiếm thông tin thời gian thực từ Internet.</span>
            </div>
        </label>

        <div v-if="useTavily" class="integration-form animate-fadeIn mt-4">
          <div class="form-group">
            <label class="form-label">Tavily API Key (Lấy từ tavily.com)</label>
            <div class="input-with-toggle">
              <input v-model="tavilyKey" :type="showTavilyKey ? 'text' : 'password'" class="input" placeholder="tvly-..." />
              <button class="btn btn-secondary" @click="showTavilyKey = !showTavilyKey">{{ showTavilyKey ? 'Ẩn' : 'Hiện' }}</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Email -->
      <div class="card section integration-card mt-4" :class="{'active': useEmail}">
        <label class="toggle-label cursor-pointer mb-2">
            <input type="checkbox" v-model="useEmail" class="form-checkbox h-5 w-5" />
            <div class="flex flex-col">
                <span class="section-subtitle mb-0">Email IMAP/SMTP</span>
                <span class="text-xs text-gray-400">Cho phép LIVA đọc, phân tích và phản hồi Email tự động.</span>
            </div>
        </label>

        <div v-if="useEmail" class="integration-form animate-fadeIn mt-4">
          <div class="grid-2">
            <div class="form-group">
              <label class="form-label">Email Host (VD: imap.gmail.com)</label>
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
              <label class="form-label">App Password (Mật khẩu ứng dụng)</label>
              <div class="input-with-toggle">
                <input v-model="emailPass" :type="showEmailPass ? 'text' : 'password'" class="input" placeholder="abcd efgh ijkl mnop" />
                <button class="btn btn-secondary" @click="showEmailPass = !showEmailPass">{{ showEmailPass ? 'Ẩn' : 'Hiện' }}</button>
              </div>
            </div>
          </div>
          <p class="form-help mt-2">Đối với Gmail, bắt buộc sử dụng <b>Mật khẩu Ứng dụng (App Password)</b>, không dùng mật khẩu thường.</p>
        </div>
      </div>

      <!-- Google APIs -->
      <div class="card section integration-card mt-4" :class="{'active': useGoogle}">
        <label class="toggle-label cursor-pointer mb-2">
            <input type="checkbox" v-model="useGoogle" class="form-checkbox h-5 w-5" />
            <div class="flex flex-col">
                <span class="section-subtitle mb-0">Google OAuth2 (Drive, Docs, Calendar)</span>
                <span class="text-xs text-gray-400">Tích hợp các công cụ văn phòng và lịch của Google.</span>
            </div>
        </label>

        <div v-if="useGoogle" class="integration-form animate-fadeIn mt-4">
          <div class="form-group">
            <label class="form-label">Google Client Secret</label>
            <input v-model="googleSecret" class="input" placeholder="GOCSPX-..." />
          </div>
          <p class="form-help text-warning mt-2">Lưu ý: Bạn cũng cần tải file <b>credentials.json</b> từ Google Cloud Console và đặt vào thư mục gốc của LIVA Gateway (openclaw-gateway).</p>
        </div>
      </div>

      <!-- Zalo -->
      <div class="card section integration-card mt-4" :class="{'active': useZalo}">
        <label class="toggle-label cursor-pointer mb-2">
            <input type="checkbox" v-model="useZalo" class="form-checkbox h-5 w-5" />
            <div class="flex flex-col">
                <span class="section-subtitle mb-0">Zalo OA (Tự động hóa tin nhắn)</span>
                <span class="text-xs text-gray-400">Cho phép LIVA gửi thông báo/tin nhắn qua nền tảng Zalo Official Account.</span>
            </div>
        </label>

        <div v-if="useZalo" class="integration-form animate-fadeIn mt-4">
          <div class="grid-2">
            <div class="form-group">
              <label class="form-label">Zalo App ID</label>
              <input v-model="zaloAppId" class="input" placeholder="123456789012345678" />
            </div>
            <div class="form-group">
              <label class="form-label">Zalo App Secret</label>
              <div class="input-with-toggle">
                <input v-model="zaloAppSecret" :type="showZaloSecret ? 'text' : 'password'" class="input" placeholder="AaBbCcDdEeFf..." />
                <button class="btn btn-secondary" @click="showZaloSecret = !showZaloSecret">{{ showZaloSecret ? 'Ẩn' : 'Hiện' }}</button>
              </div>
            </div>
          </div>
          <p class="form-help text-warning mt-2">Lưu ý: Mọi Token và mật khẩu tĩnh ở màn hình này (ngoại trừ Zalo Secret) sẽ tự động được mã hóa 2 chiều AES-256-GCM vào Vault an toàn.</p>
        </div>
      </div>

      <div class="actions mt-6">
        <button class="btn btn-primary" @click="saveEnvConfig" :disabled="isSavingEnv">{{ isSavingEnv ? 'Đang lưu...' : 'Lưu Tích hợp (Cần Restart Gateway)' }}</button>
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
.section-subtitle { margin-bottom: 8px; display: inline-block;}

/* Tabs */
.tabs-header { display: flex; gap: 8px; margin-bottom: var(--space-lg); border-bottom: 1px solid var(--border-default); padding-bottom: 8px; }
.tab-btn { padding: 10px 20px; border-radius: var(--radius-md); background: transparent; border: 1px solid transparent; color: var(--text-secondary); font-weight: 600; cursor: pointer; transition: all 0.2s; }
.tab-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
.tab-btn.active { background: rgba(124,58,237,.1); border-color: rgba(124,58,237,.3); color: #a78bfa; }

/* Status */
.status-card { display: flex; flex-direction: column; gap: 10px; margin-bottom: var(--space-md); }
.status-row { display: flex; justify-content: space-between; align-items: center; font-size: 13px; }
.pill { padding: 4px 10px; border-radius: 999px; background: var(--bg-hover); }
.pill-ok { background: rgba(63,185,80,.15); color: #3fb950; }
.pill-bad { background: rgba(248,81,73,.15); color: var(--color-danger); }

/* Integration Cards */
.integration-card { border: 1px solid var(--border-default); transition: all 0.3s ease; }
.integration-card.active { border-color: rgba(124,58,237,.4); background: rgba(124,58,237,.02); }
.integration-form { padding-top: 16px; border-top: 1px solid var(--border-subtle); }
.toggle-label { display: flex; align-items: center; gap: 12px; }
.form-checkbox { width: 20px; height: 20px; accent-color: var(--accent-start); }

/* Grid / Form */
.grid-2 { display: grid; grid-template-columns: repeat(2, 1fr); gap: var(--space-sm); }
.grid-3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--space-sm); }
.input-with-toggle { display: flex; gap: 8px; }
.input-with-toggle .input { flex: 1; }
.actions { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
.hint { font-size: 13px; }
.text-green { color: #22c55e; }
.text-red { color: #ef4444; }
.text-warning { color: #d97706; font-size: 12px; }

@media (max-width: 768px) { .grid-2, .grid-3 { grid-template-columns: 1fr; } }
</style>