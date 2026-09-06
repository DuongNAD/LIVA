<script setup lang="ts">
/**
 * ChannelsManagementView.vue — Multi-Channel Management Dashboard
 * ================================================================
 * Visual interface for Telegram, WhatsApp (with Live QR pairing),
 * Discord, and Slack adapters with live connection health monitors.
 */
import { ref, onMounted, computed } from "vue";
import { useGateway } from "../../composables/useGateway";
import WhatsAppQrModal from "./channels/WhatsAppQrModal.vue";
import type { ChannelItem, ChannelConnectionStatus } from "liva-common";

const gateway = useGateway();

// Form States
const showWaQrModal = ref(false);
const isTestingChannel = ref<string | null>(null);
const testResults = ref<Record<string, { success: boolean; latencyMs: number; message: string }>>({});
const showTokens = ref<Record<string, boolean>>({
  telegram: false,
  discord: false,
  slack_bot: false,
  slack_app: false,
});

// Channel Config Forms
const telegramConfig = ref({
  enabled: false,
  botToken: "",
  polling: true,
  allowedUserIds: "",
});

const whatsAppConfig = ref({
  enabled: false,
  phoneNumberId: "",
  apiToken: "",
  webhookSecret: "",
});

const discordConfig = ref({
  enabled: false,
  botToken: "",
  clientId: "",
  guildId: "",
  intents: ["Guilds", "GuildMessages", "MessageContent"],
});

const slackConfig = ref({
  enabled: false,
  botToken: "",
  appToken: "",
  signingSecret: "",
  webhookUrl: "",
});

const isSaving = ref<string | null>(null);
const toastMessage = ref<string | null>(null);

const showToast = (msg: string) => {
  toastMessage.value = msg;
  setTimeout(() => {
    toastMessage.value = null;
  }, 3500);
};

const channels = computed<ChannelItem[]>(() => {
  return gateway.channelsList.value || [];
});

const activeChannelsCount = computed(() => {
  return channels.value.filter((c) => {
    const status = typeof c.status === "object" ? c.status.status : c.status;
    return status === "connected" && c.enabled;
  }).length;
});

const getStatusType = (statusRaw?: ChannelItem["status"]): ChannelConnectionStatus => {
  if (!statusRaw) {
    return "disconnected";
  }
  if (typeof statusRaw === "object" && statusRaw !== null) {
    return statusRaw.status;
  }
  return (statusRaw as ChannelConnectionStatus) || "disconnected";
};

const getStatusBadge = (statusRaw?: ChannelItem["status"]) => {
  const status = getStatusType(statusRaw);
  switch (status) {
    case "connected":
      return { text: "Connected", class: "badge-connected", dot: "🟢" };
    case "reconnecting":
      return { text: "Reconnecting", class: "badge-reconnecting", dot: "🟡" };
    case "failed":
      return { text: "Failed", class: "badge-failed", dot: "🔴" };
    case "standby":
      return { text: "Standby", class: "badge-standby", dot: "⚪" };
    default:
      return { text: "Disconnected", class: "badge-disconnected", dot: "⚪" };
  }
};

const refreshChannels = async () => {
  const list = await gateway.fetchChannels();
  if (list) {
    // Populate form data from summary if present
    const tg = list.find((c) => c.id === "telegram");
    if (tg) {
      telegramConfig.value.enabled = tg.enabled;
      if (tg.config_summary?.botToken_masked) {
        telegramConfig.value.botToken = tg.config_summary.botToken_masked;
      }
    }
    const wa = list.find((c) => c.id === "whatsapp");
    if (wa) whatsAppConfig.value.enabled = wa.enabled;
    const dc = list.find((c) => c.id === "discord");
    if (dc) {
      discordConfig.value.enabled = dc.enabled;
      if (dc.config_summary?.botToken_masked) {
        discordConfig.value.botToken = dc.config_summary.botToken_masked;
      }
    }
    const sl = list.find((c) => c.id === "slack");
    if (sl) {
      slackConfig.value.enabled = sl.enabled;
      if (sl.config_summary?.botToken_masked) {
        slackConfig.value.botToken = sl.config_summary.botToken_masked;
      }
    }
  }
};

onMounted(() => {
  refreshChannels();
});

const handleSaveTelegram = async () => {
  isSaving.value = "telegram";
  try {
    await gateway.configureChannel("telegram", {
      enabled: telegramConfig.value.enabled,
      botToken: telegramConfig.value.botToken,
      polling: telegramConfig.value.polling,
      allowedUserIds: telegramConfig.value.allowedUserIds,
    });
    showToast("Telegram configuration saved and updated.");
  } finally {
    isSaving.value = null;
  }
};

const handleSaveDiscord = async () => {
  isSaving.value = "discord";
  try {
    await gateway.configureChannel("discord", {
      enabled: discordConfig.value.enabled,
      botToken: discordConfig.value.botToken,
      clientId: discordConfig.value.clientId,
      guildId: discordConfig.value.guildId,
    });
    showToast("Discord bot configuration saved and gateway synced.");
  } finally {
    isSaving.value = null;
  }
};

const handleSaveSlack = async () => {
  isSaving.value = "slack";
  try {
    await gateway.configureChannel("slack", {
      enabled: slackConfig.value.enabled,
      botToken: slackConfig.value.botToken,
      appToken: slackConfig.value.appToken,
      signingSecret: slackConfig.value.signingSecret,
      webhookUrl: slackConfig.value.webhookUrl,
    });
    showToast("Slack socket mode credentials saved.");
  } finally {
    isSaving.value = null;
  }
};

const handleTestConnection = async (channelId: string) => {
  isTestingChannel.value = channelId;
  try {
    const res = await gateway.testChannel(channelId);
    testResults.value[channelId] = {
      success: res.success,
      latencyMs: res.latencyMs,
      message: res.message,
    };
    showToast(`${res.message} (${res.latencyMs}ms)`);
  } catch (err) {
    testResults.value[channelId] = {
      success: false,
      latencyMs: 0,
      message: String(err),
    };
  } finally {
    isTestingChannel.value = null;
  }
};

const handleToggleChannel = async (channelId: string, currentEnabled: boolean) => {
  if (currentEnabled) {
    await gateway.stopChannel(channelId);
    showToast(`Stopped channel ${channelId}`);
  } else {
    await gateway.startChannel(channelId);
    showToast(`Started channel ${channelId}`);
  }
};

const generateDiscordInvite = () => {
  const cid = discordConfig.value.clientId || "1234567890";
  const url = `https://discord.com/api/oauth2/authorize?client_id=${cid}&permissions=8&scope=bot%20applications.commands`;
  navigator.clipboard?.writeText(url);
  showToast("Discord Bot Invite URL copied to clipboard!");
};
</script>

<template>
  <div class="channels-view">
    <!-- Header -->
    <header class="channels-header">
      <div class="header-titles">
        <h2>Multi-Channel Hub</h2>
        <p class="subtitle">
          Connect and manage external messaging bridges (Telegram, WhatsApp, Discord, Slack) with zero-copy stream processing.
        </p>
      </div>
      <div class="header-actions">
        <div class="stat-pill">
          <span>Active Bridges:</span>
          <strong>{{ activeChannelsCount }}/{{ channels.length || 4 }}</strong>
        </div>
        <button class="btn btn-secondary" @click="refreshChannels">
          🔄 Refresh
        </button>
      </div>
    </header>

    <!-- Toast Banner -->
    <Transition name="fade">
      <div v-if="toastMessage" class="toast-banner">
        <span>{{ toastMessage }}</span>
      </div>
    </Transition>

    <!-- Channel Cards Grid -->
    <div class="channels-grid">
      <!-- 1. Telegram Card -->
      <div class="channel-card">
        <div class="card-header">
          <div class="channel-brand">
            <div class="icon-avatar telegram-bg">
              <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#38bdf8" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>
            </div>
            <div>
              <h3>Telegram Bot</h3>
              <span class="subtext">Teloxide bot with voice PTT & streaming</span>
            </div>
          </div>
          <div class="card-header-right">
            <span :class="['badge', getStatusBadge(channels.find(c => c.id === 'telegram')?.status).class]">
              {{ getStatusBadge(channels.find(c => c.id === 'telegram')?.status).text }}
            </span>
          </div>
        </div>

        <div class="card-body">
          <div class="form-group">
            <label>Bot API Token</label>
            <div class="input-with-action">
              <input
                :type="showTokens.telegram ? 'text' : 'password'"
                v-model="telegramConfig.botToken"
                placeholder="123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
              />
              <button class="icon-btn" @click="showTokens.telegram = !showTokens.telegram">
                {{ showTokens.telegram ? "🙈" : "👁️" }}
              </button>
            </div>
          </div>

          <div class="form-row">
            <div class="form-group flex-1">
              <label>Allowed User IDs (Whitelist)</label>
              <input
                type="text"
                v-model="telegramConfig.allowedUserIds"
                placeholder="e.g. 10928374, 98234719"
              />
            </div>
            <div class="form-group toggle-group">
              <label>Polling Mode</label>
              <label class="switch">
                <input type="checkbox" v-model="telegramConfig.polling" />
                <span class="slider"></span>
              </label>
            </div>
          </div>

          <!-- Test result banner if present -->
          <div v-if="testResults.telegram" class="test-result-box" :class="{ error: !testResults.telegram.success }">
            <span>{{ testResults.telegram.message }}</span>
            <strong v-if="testResults.telegram.latencyMs">({{ testResults.telegram.latencyMs }}ms)</strong>
          </div>
        </div>

        <div class="card-footer">
          <div class="toggle-channel">
            <label class="switch">
              <input
                type="checkbox"
                :checked="channels.find(c => c.id === 'telegram')?.enabled"
                @change="handleToggleChannel('telegram', Boolean(channels.find(c => c.id === 'telegram')?.enabled))"
              />
              <span class="slider"></span>
            </label>
            <span class="toggle-label">{{ channels.find(c => c.id === 'telegram')?.enabled ? "Enabled" : "Disabled" }}</span>
          </div>

          <div class="btn-group">
            <button
              class="btn btn-secondary"
              :disabled="isTestingChannel === 'telegram'"
              @click="handleTestConnection('telegram')"
            >
              {{ isTestingChannel === 'telegram' ? 'Testing...' : 'Test Probe' }}
            </button>
            <button
              class="btn btn-primary"
              :disabled="isSaving === 'telegram'"
              @click="handleSaveTelegram"
            >
              {{ isSaving === 'telegram' ? 'Saving...' : 'Save Config' }}
            </button>
          </div>
        </div>
      </div>

      <!-- 2. WhatsApp Card (with Live QR Modal) -->
      <div class="channel-card">
        <div class="card-header">
          <div class="channel-brand">
            <div class="icon-avatar whatsapp-bg">
              <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#4ade80" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"/></svg>
            </div>
            <div>
              <h3>WhatsApp Multi-Device</h3>
              <span class="subtext">Meta Cloud API & Web QR pairing bridge</span>
            </div>
          </div>
          <div class="card-header-right">
            <span :class="['badge', getStatusBadge(channels.find(c => c.id === 'whatsapp')?.status).class]">
              {{ getStatusBadge(channels.find(c => c.id === 'whatsapp')?.status).text }}
            </span>
          </div>
        </div>

        <div class="card-body">
          <div class="pairing-banner">
            <div class="pairing-banner-info">
              <h4>Direct Companion Pairing</h4>
              <p>Scan WhatsApp Web QR code to connect without Meta Business Verification.</p>
            </div>
            <button class="btn btn-emerald" @click="showWaQrModal = true">
              📱 Scan Pairing QR
            </button>
          </div>

          <div class="form-row">
            <div class="form-group flex-1">
              <label>Phone Number ID (Optional Cloud API)</label>
              <input
                type="text"
                v-model="whatsAppConfig.phoneNumberId"
                placeholder="109283746592817"
              />
            </div>
            <div class="form-group flex-1">
              <label>Webhook Verify Token</label>
              <input
                type="text"
                v-model="whatsAppConfig.webhookSecret"
                placeholder="liva_webhook_secret_key"
              />
            </div>
          </div>

          <div v-if="testResults.whatsapp" class="test-result-box" :class="{ error: !testResults.whatsapp.success }">
            <span>{{ testResults.whatsapp.message }}</span>
            <strong v-if="testResults.whatsapp.latencyMs">({{ testResults.whatsapp.latencyMs }}ms)</strong>
          </div>
        </div>

        <div class="card-footer">
          <div class="toggle-channel">
            <label class="switch">
              <input
                type="checkbox"
                :checked="channels.find(c => c.id === 'whatsapp')?.enabled"
                @change="handleToggleChannel('whatsapp', Boolean(channels.find(c => c.id === 'whatsapp')?.enabled))"
              />
              <span class="slider"></span>
            </label>
            <span class="toggle-label">{{ channels.find(c => c.id === 'whatsapp')?.enabled ? "Enabled" : "Disabled" }}</span>
          </div>

          <div class="btn-group">
            <button
              class="btn btn-secondary"
              :disabled="isTestingChannel === 'whatsapp'"
              @click="handleTestConnection('whatsapp')"
            >
              {{ isTestingChannel === 'whatsapp' ? 'Testing...' : 'Test Handshake' }}
            </button>
          </div>
        </div>
      </div>

      <!-- 3. Discord Card -->
      <div class="channel-card">
        <div class="card-header">
          <div class="channel-brand">
            <div class="icon-avatar discord-bg">
              <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#818cf8" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6h0a14.5 14.5 0 0 0-4-1.5 9.87 9.87 0 0 0-1 1.5 14.5 14.5 0 0 0-4 0 9.87 9.87 0 0 0-1-1.5A14.5 14.5 0 0 0 4 6c-2 6-1 12 0 14a14.5 14.5 0 0 0 5 2.5 9.87 9.87 0 0 0 1-1.5 14.5 14.5 0 0 0 4 0 9.87 9.87 0 0 0 1 1.5 14.5 14.5 0 0 0 5-2.5c1-2 2-8 0-14z"/><circle cx="9" cy="12" r="1.5"/><circle cx="15" cy="12" r="1.5"/></svg>
            </div>
            <div>
              <h3>Discord Gateway Bot</h3>
              <span class="subtext">WebSocket Gateway & Thread Replies</span>
            </div>
          </div>
          <div class="card-header-right">
            <span :class="['badge', getStatusBadge(channels.find(c => c.id === 'discord')?.status).class]">
              {{ getStatusBadge(channels.find(c => c.id === 'discord')?.status).text }}
            </span>
          </div>
        </div>

        <div class="card-body">
          <div class="form-group">
            <label>Bot Token</label>
            <div class="input-with-action">
              <input
                :type="showTokens.discord ? 'text' : 'password'"
                v-model="discordConfig.botToken"
                placeholder="MTA5ODc2NTQzMjEw.GhIjKl.MnOpQrStUvWxYz"
              />
              <button class="icon-btn" @click="showTokens.discord = !showTokens.discord">
                {{ showTokens.discord ? "🙈" : "👁️" }}
              </button>
            </div>
          </div>

          <div class="form-row">
            <div class="form-group flex-1">
              <label>Application / Client ID</label>
              <input
                type="text"
                v-model="discordConfig.clientId"
                placeholder="109876543210123456"
              />
            </div>
            <div class="form-group flex-1">
              <label>Guild / Server ID (Optional)</label>
              <input
                type="text"
                v-model="discordConfig.guildId"
                placeholder="987654321098765432"
              />
            </div>
          </div>

          <div v-if="testResults.discord" class="test-result-box" :class="{ error: !testResults.discord.success }">
            <span>{{ testResults.discord.message }}</span>
            <strong v-if="testResults.discord.latencyMs">({{ testResults.discord.latencyMs }}ms)</strong>
          </div>
        </div>

        <div class="card-footer">
          <div class="toggle-channel">
            <label class="switch">
              <input
                type="checkbox"
                :checked="channels.find(c => c.id === 'discord')?.enabled"
                @change="handleToggleChannel('discord', Boolean(channels.find(c => c.id === 'discord')?.enabled))"
              />
              <span class="slider"></span>
            </label>
            <span class="toggle-label">{{ channels.find(c => c.id === 'discord')?.enabled ? "Enabled" : "Disabled" }}</span>
          </div>

          <div class="btn-group">
            <button class="btn btn-secondary" @click="generateDiscordInvite">
              🔗 Invite Bot
            </button>
            <button
              class="btn btn-secondary"
              :disabled="isTestingChannel === 'discord'"
              @click="handleTestConnection('discord')"
            >
              {{ isTestingChannel === 'discord' ? 'Testing...' : 'Test Probe' }}
            </button>
            <button
              class="btn btn-primary"
              :disabled="isSaving === 'discord'"
              @click="handleSaveDiscord"
            >
              {{ isSaving === 'discord' ? 'Saving...' : 'Save Config' }}
            </button>
          </div>
        </div>
      </div>

      <!-- 4. Slack Card -->
      <div class="channel-card">
        <div class="card-header">
          <div class="channel-brand">
            <div class="icon-avatar slack-bg">
              <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#e879f9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="3" height="8" x="13" y="2" rx="1.5"/><path d="M19 8.5V10h1.5A1.5 1.5 0 1 0 19 8.5"/><rect width="3" height="8" x="8" y="14" rx="1.5"/><path d="M5 15.5V14H3.5A1.5 1.5 0 1 0 5 15.5"/><rect width="8" height="3" x="14" y="13" rx="1.5"/><path d="M15.5 19H14v1.5a1.5 1.5 0 1 0 1.5-1.5"/><rect width="8" height="3" x="2" y="8" rx="1.5"/><path d="M8.5 5H10V3.5A1.5 1.5 0 1 0 8.5 5"/></svg>
            </div>
            <div>
              <h3>Slack Socket Mode</h3>
              <span class="subtext">Block Kit, Socket Mode & Thread Timestamps</span>
            </div>
          </div>
          <div class="card-header-right">
            <span :class="['badge', getStatusBadge(channels.find(c => c.id === 'slack')?.status).class]">
              {{ getStatusBadge(channels.find(c => c.id === 'slack')?.status).text }}
            </span>
          </div>
        </div>

        <div class="card-body">
          <div class="form-row">
            <div class="form-group flex-1">
              <label>Bot User OAuth Token (xoxb-...)</label>
              <div class="input-with-action">
                <input
                  :type="showTokens.slack_bot ? 'text' : 'password'"
                  v-model="slackConfig.botToken"
                  placeholder="xoxb-12345-67890-abcdef"
                />
                <button class="icon-btn" @click="showTokens.slack_bot = !showTokens.slack_bot">
                  {{ showTokens.slack_bot ? "🙈" : "👁️" }}
                </button>
              </div>
            </div>
            <div class="form-group flex-1">
              <label>App-Level Token (xapp-...)</label>
              <div class="input-with-action">
                <input
                  :type="showTokens.slack_app ? 'text' : 'password'"
                  v-model="slackConfig.appToken"
                  placeholder="xapp-1-A12345-67890"
                />
                <button class="icon-btn" @click="showTokens.slack_app = !showTokens.slack_app">
                  {{ showTokens.slack_app ? "🙈" : "👁️" }}
                </button>
              </div>
            </div>
          </div>

          <div class="form-row">
            <div class="form-group flex-1">
              <label>Signing Secret</label>
              <input
                type="password"
                v-model="slackConfig.signingSecret"
                placeholder="a1b2c3d4e5f6g7h8"
              />
            </div>
            <div class="form-group flex-1">
              <label>Incoming Webhook URL (Optional)</label>
              <input
                type="text"
                v-model="slackConfig.webhookUrl"
                placeholder="https://hooks.slack.com/services/..."
              />
            </div>
          </div>

          <div v-if="testResults.slack" class="test-result-box" :class="{ error: !testResults.slack.success }">
            <span>{{ testResults.slack.message }}</span>
            <strong v-if="testResults.slack.latencyMs">({{ testResults.slack.latencyMs }}ms)</strong>
          </div>
        </div>

        <div class="card-footer">
          <div class="toggle-channel">
            <label class="switch">
              <input
                type="checkbox"
                :checked="channels.find(c => c.id === 'slack')?.enabled"
                @change="handleToggleChannel('slack', Boolean(channels.find(c => c.id === 'slack')?.enabled))"
              />
              <span class="slider"></span>
            </label>
            <span class="toggle-label">{{ channels.find(c => c.id === 'slack')?.enabled ? "Enabled" : "Disabled" }}</span>
          </div>

          <div class="btn-group">
            <button
              class="btn btn-secondary"
              :disabled="isTestingChannel === 'slack'"
              @click="handleTestConnection('slack')"
            >
              {{ isTestingChannel === 'slack' ? 'Testing...' : 'Test Probe' }}
            </button>
            <button
              class="btn btn-primary"
              :disabled="isSaving === 'slack'"
              @click="handleSaveSlack"
            >
              {{ isSaving === 'slack' ? 'Saving...' : 'Save Config' }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- WhatsApp QR Modal -->
    <WhatsAppQrModal
      v-if="showWaQrModal"
      @close="showWaQrModal = false"
      @paired="refreshChannels"
    />
  </div>
</template>

<style scoped>
.channels-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 24px;
  overflow-y: auto;
  gap: 20px;
  background: var(--bg-secondary, #0e1017);
}

.channels-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
}

.header-titles h2 {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary, #ffffff);
  margin: 0 0 6px 0;
}

.subtitle {
  font-size: 13px;
  color: var(--text-secondary, #94a3b8);
  margin: 0;
  max-width: 650px;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.stat-pill {
  padding: 6px 12px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid var(--border-default, #2a2d3d);
  font-size: 12px;
  color: var(--text-secondary, #94a3b8);
  display: flex;
  gap: 6px;
}

.stat-pill strong {
  color: #38bdf8;
}

.toast-banner {
  padding: 10px 16px;
  border-radius: 8px;
  background: rgba(56, 189, 248, 0.12);
  border: 1px solid rgba(56, 189, 248, 0.3);
  color: #38bdf8;
  font-size: 13px;
  font-weight: 500;
}

.channels-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(460px, 1fr));
  gap: 20px;
}

.channel-card {
  background: rgba(18, 20, 29, 0.7);
  border: 1px solid var(--border-default, #242738);
  border-radius: 14px;
  display: flex;
  flex-direction: column;
  backdrop-filter: blur(10px);
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.2);
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
}

.channel-brand {
  display: flex;
  align-items: center;
  gap: 12px;
}

.icon-avatar {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.telegram-bg { background: rgba(56, 189, 248, 0.12); }
.whatsapp-bg { background: rgba(34, 197, 94, 0.12); }
.discord-bg { background: rgba(99, 102, 241, 0.12); }
.slack-bg { background: rgba(232, 121, 249, 0.12); }

.channel-brand h3 {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0;
}

.subtext {
  font-size: 11px;
  color: var(--text-muted, #64748b);
}

.badge {
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
}

.badge-connected {
  background: rgba(34, 197, 94, 0.12);
  color: #4ade80;
  border: 1px solid rgba(34, 197, 94, 0.25);
}

.badge-disconnected {
  background: rgba(148, 163, 184, 0.1);
  color: #94a3b8;
  border: 1px solid rgba(148, 163, 184, 0.2);
}

.badge-reconnecting {
  background: rgba(234, 179, 8, 0.12);
  color: #facc15;
  border: 1px solid rgba(234, 179, 8, 0.25);
}

.badge-failed {
  background: rgba(239, 68, 68, 0.12);
  color: #f87171;
  border: 1px solid rgba(239, 68, 68, 0.25);
}

.card-body {
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  flex: 1;
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

.form-group input {
  padding: 8px 12px;
  border-radius: 8px;
  background: rgba(10, 11, 16, 0.7);
  border: 1px solid var(--border-default, #2a2d3d);
  color: var(--text-primary, #ffffff);
  font-size: 13px;
  outline: none;
}

.form-group input:focus {
  border-color: #6366f1;
}

.form-row {
  display: flex;
  gap: 12px;
}

.flex-1 { flex: 1; }

.input-with-action {
  display: flex;
  position: relative;
  align-items: center;
}

.input-with-action input {
  width: 100%;
  padding-right: 36px;
}

.icon-btn {
  position: absolute;
  right: 6px;
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 4px;
  font-size: 14px;
}

.pairing-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: rgba(34, 197, 94, 0.08);
  border: 1px dashed rgba(34, 197, 94, 0.3);
  border-radius: 10px;
}

.pairing-banner-info h4 {
  font-size: 13px;
  font-weight: 600;
  color: #4ade80;
  margin: 0 0 2px 0;
}

.pairing-banner-info p {
  font-size: 11px;
  color: var(--text-secondary, #94a3b8);
  margin: 0;
}

.test-result-box {
  padding: 8px 12px;
  border-radius: 6px;
  background: rgba(34, 197, 94, 0.1);
  border: 1px solid rgba(34, 197, 94, 0.25);
  color: #4ade80;
  font-size: 12px;
  display: flex;
  justify-content: space-between;
}

.test-result-box.error {
  background: rgba(239, 68, 68, 0.1);
  border-color: rgba(239, 68, 68, 0.25);
  color: #f87171;
}

.card-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 14px 20px;
  background: rgba(0, 0, 0, 0.2);
  border-top: 1px solid rgba(255, 255, 255, 0.04);
}

.toggle-channel {
  display: flex;
  align-items: center;
  gap: 10px;
}

.toggle-label {
  font-size: 12px;
  color: var(--text-secondary, #94a3b8);
}

.btn-group {
  display: flex;
  gap: 8px;
}

.btn {
  padding: 7px 14px;
  border-radius: 8px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  border: none;
}

.btn-secondary {
  background: rgba(255, 255, 255, 0.06);
  color: var(--text-primary, #ffffff);
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.btn-secondary:hover {
  background: rgba(255, 255, 255, 0.12);
}

.btn-primary {
  background: linear-gradient(135deg, #6366f1, #8b5cf6);
  color: #ffffff;
}

.btn-primary:hover {
  background: linear-gradient(135deg, #4f46e5, #7c3aed);
}

.btn-emerald {
  background: linear-gradient(135deg, #10b981, #059669);
  color: #ffffff;
}

/* Switch Styles */
.switch {
  position: relative;
  display: inline-block;
  width: 38px;
  height: 20px;
}

.switch input {
  opacity: 0;
  width: 0;
  height: 0;
}

.slider {
  position: absolute;
  cursor: pointer;
  inset: 0;
  background-color: rgba(255, 255, 255, 0.12);
  transition: 0.2s;
  border-radius: 20px;
}

.slider:before {
  position: absolute;
  content: "";
  height: 14px;
  width: 14px;
  left: 3px;
  bottom: 3px;
  background-color: white;
  transition: 0.2s;
  border-radius: 50%;
}

input:checked + .slider {
  background: linear-gradient(135deg, #10b981, #059669);
}

input:checked + .slider:before {
  transform: translateX(18px);
}
</style>
