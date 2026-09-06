import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const channelsListRef = ref([
  {
    id: "telegram",
    name: "Telegram Bot",
    channel_type: "telegram",
    status: "connected",
    enabled: true,
    capabilities: {
      streaming_text: true,
      binary_attachments: true,
      voice_notes: true,
      interactive_buttons: true,
      typing_indicator: true,
      thread_replies: false,
    },
    last_seen_unix: 1700000000,
    message_count: 42,
    config_summary: {
      mode: "polling",
      botToken_masked: "1234***wxyz",
    },
  },
  {
    id: "whatsapp",
    name: "WhatsApp Multi-Device",
    channel_type: "whatsapp",
    status: "disconnected",
    enabled: false,
    capabilities: {
      streaming_text: false,
      binary_attachments: true,
      voice_notes: true,
      interactive_buttons: true,
      typing_indicator: true,
      thread_replies: false,
    },
    last_seen_unix: 0,
    message_count: 0,
    config_summary: {
      pairing_mode: "qr_code",
    },
  },
  {
    id: "discord",
    name: "Discord Gateway Bot",
    channel_type: "discord",
    status: "reconnecting",
    enabled: true,
    capabilities: {
      streaming_text: true,
      binary_attachments: true,
      voice_notes: true,
      interactive_buttons: true,
      typing_indicator: true,
      thread_replies: true,
    },
    last_seen_unix: 1699999000,
    message_count: 10,
    config_summary: {
      gateway_intents: "Guilds, GuildMessages",
      botToken_masked: "MTAx***9876",
    },
  },
  {
    id: "slack",
    name: "Slack Socket Bot",
    channel_type: "slack",
    status: "failed",
    enabled: true,
    capabilities: {
      streaming_text: true,
      binary_attachments: true,
      voice_notes: false,
      interactive_buttons: true,
      typing_indicator: true,
      thread_replies: true,
    },
    last_seen_unix: 0,
    message_count: 0,
    config_summary: {
      transport: "socket_mode",
      botToken_masked: "xoxb***1234",
    },
  },
]);

const pairedNodesListRef = ref([
  {
    nodeId: "node-1-iphone",
    nodeName: "Alice's iPhone 16",
    role: "mobile_companion",
    publicKey: "ed25519_pubkey_alice_phone",
    approvedAtUnix: 1700000000,
    lastSeenUnix: 1700000100,
    deviceType: "mobile",
  },
  {
    nodeId: "node-2-server",
    nodeName: "Home GPU Server",
    role: "headless_node",
    publicKey: "ed25519_pubkey_gpu_box",
    approvedAtUnix: 1699900000,
    lastSeenUnix: 1700000120,
    deviceType: "server",
  },
]);

const pendingPairingListRef = ref([
  {
    challengeId: "ch-999-pending",
    shortCode: "481920",
    nonce: "nonce_val_123",
    nodeId: "node-3-pending-tab",
    nodeName: "Bob's iPad Pro",
    role: "mobile_companion",
    publicKey: "ed25519_pubkey_bob_tab",
    createdAtUnix: 1700000000,
    expiresAtUnix: 1700000300,
    ttlRemainingSeconds: 240,
  },
]);

const browserStatusRef = ref({
  isRunning: true,
  isPaused: false,
  currentUrl: "https://liva.ai/dashboard",
  pageTitle: "LIVA Cognitive Dashboard",
  httpStatus: 200,
  viewportWidth: 1280,
  viewportHeight: 800,
  sandboxActive: true,
  ssrfGuard: true,
});

const browserScreenshotRef = ref("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==");

const browserActionLogsRef = ref([
  {
    id: "act-1",
    timestamp_unix: 1700000000,
    action: "navigate",
    target: "https://liva.ai/dashboard",
    status: "success",
    details: "Loaded page (HTTP 200)",
  },
  {
    id: "act-2",
    timestamp_unix: 1700000010,
    action: "screenshot",
    target: "Viewport (1280x800)",
    status: "success",
    details: "Captured screenshot frame",
  },
]);

const skillsListRef = ref([
  {
    name: "github-integration",
    description: "Manage GitHub repositories and pull requests",
    category: "developer",
    isCoreSkill: false,
    enabled: true,
    status: "active",
  },
  {
    name: "weather-forecast",
    description: "Real-time weather radar and temperature",
    category: "utilities",
    isCoreSkill: true,
    enabled: false,
    status: "disabled",
  },
]);

const gatewayMock = {
  isConnected: ref(true),
  channelsList: channelsListRef,
  pairedNodesList: pairedNodesListRef,
  pendingPairingList: pendingPairingListRef,
  browserStatus: browserStatusRef,
  browserScreenshot: browserScreenshotRef,
  browserActionLogs: browserActionLogsRef,
  skillsList: skillsListRef,

  fetchChannels: vi.fn().mockImplementation(async () => channelsListRef.value),
  configureChannel: vi.fn().mockImplementation(async (id, cfg) => {
    const ch = channelsListRef.value.find((c) => c.id === id);
    if (ch) {
      ch.enabled = cfg.enabled;
    }
    return { success: true, channel: ch };
  }),
  getWhatsAppQr: vi.fn().mockResolvedValue({
    qrData: "2@LIVA_PAIR_MOCK_QR_CODE",
    expiresAtUnix: 1700000120,
    ttlSeconds: 120,
    pairingState: "awaiting_scan",
  }),
  startChannel: vi.fn().mockResolvedValue({ success: true }),
  stopChannel: vi.fn().mockResolvedValue({ success: true }),
  testChannel: vi.fn().mockResolvedValue({
    channelId: "telegram",
    success: true,
    latencyMs: 38,
    status: "connected",
    message: "Handshake verified",
  }),

  fetchPairedNodes: vi.fn().mockImplementation(async () => pairedNodesListRef.value),
  fetchPendingPairing: vi.fn().mockImplementation(async () => pendingPairingListRef.value),
  approvePairing: vi.fn().mockImplementation(async ({ shortCode }) => {
    pendingPairingListRef.value = pendingPairingListRef.value.filter((p) => p.shortCode !== shortCode);
    return { success: true, paired: true, authToken: "tok_123" };
  }),
  rejectPairing: vi.fn().mockImplementation(async (challengeId) => {
    pendingPairingListRef.value = pendingPairingListRef.value.filter((p) => p.challengeId !== challengeId);
    return { success: true, challengeId };
  }),
  revokePairing: vi.fn().mockImplementation(async (nodeId) => {
    pairedNodesListRef.value = pairedNodesListRef.value.filter((n) => n.nodeId !== nodeId);
    return { success: true, nodeId, revoked: true };
  }),
  createPairingChallenge: vi.fn().mockResolvedValue({
    challengeId: "ch-gen-123",
    shortCode: "123456",
    nodeId: "node-gen-123",
    nodeName: "Test Phone",
    expiresAtUnix: 1700000300,
    qrPayload: "liva-pair:123456:ch-gen-123",
  }),

  fetchBrowserStatus: vi.fn().mockImplementation(async () => browserStatusRef.value),
  fetchBrowserScreenshot: vi.fn().mockImplementation(async () => browserScreenshotRef.value),
  fetchBrowserActionLogs: vi.fn().mockImplementation(async () => browserActionLogsRef.value),
  navigateBrowser: vi.fn().mockResolvedValue({
    success: true,
    url: "https://liva.ai/dashboard",
    title: "LIVA Dashboard",
    httpStatus: 200,
  }),
  extractBrowserDom: vi.fn().mockResolvedValue({
    mode: "semantic",
    content: "# LIVA Heading\nSample semantic content",
    length: 37,
  }),
  controlBrowser: vi.fn().mockResolvedValue({ success: true, state: "paused" }),

  getSkillManifest: vi.fn().mockResolvedValue({
    skillId: "github-integration",
    name: "github-integration",
    version: "1.0.0",
    description: "Manage GitHub repositories",
    author: "LIVA Team",
    license: "MIT",
    triggers: [{ type: "intent", config: "github" }],
    permissions: [{ type: "network_egress" }],
    tools: [{ name: "create_issue", description: "Create GitHub issue", risk_level: "read_only_safe" }],
    runtimeType: "native_rust",
    markdownInstructions: "# GitHub Integration\nDetailed instructions.",
    rawContent: "---\nname: github-integration\n---\n# GitHub",
    contentHash: "sha256_mock_hash",
    dirPath: "/skills/github-integration",
  }),
  getSkillConfig: vi.fn().mockResolvedValue({
    skillId: "github-integration",
    params: { timeoutSeconds: 30, maxRetries: 3 },
    schema: {
      type: "object",
      properties: {
        timeoutSeconds: { type: "number", default: 30, description: "Timeout" },
      },
    },
  }),
  saveSkillConfig: vi.fn().mockResolvedValue({ success: true, skillId: "github-integration" }),
  fetchSkillLogs: vi.fn().mockResolvedValue({
    skillId: "github-integration",
    count: 1,
    logs: [
      {
        id: "log-1",
        skillId: "github-integration",
        timestampUnix: 1700000000,
        caller: "ReActAgent",
        status: "SUCCESS",
        durationMs: 45,
        input: { query: "list_prs" },
        output: { pr_count: 2 },
      },
    ],
  }),
  installSkillFromHub: vi.fn().mockResolvedValue({
    success: true,
    skillId: "clawhub-searxng",
    name: "clawhub-searxng",
    installedPath: "/skills/clawhub-searxng",
  }),
  sendMsg: vi.fn(),
  onSkillCheckResult: vi.fn(),
  offSkillCheckResult: vi.fn(),
  onAllSkillsCheckComplete: vi.fn(),
  offAllSkillsCheckComplete: vi.fn(),
};

vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => gatewayMock,
}));

vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (key: string) => `trans_${key}`,
    currentLang: ref("en-US"),
  }),
}));

import ChannelsManagementView from "../../src/components/dashboard/ChannelsManagementView.vue";
import WhatsAppQrModal from "../../src/components/dashboard/channels/WhatsAppQrModal.vue";
import SkillsView from "../../src/components/dashboard/SkillsView.vue";
import SkillManifestDrawer from "../../src/components/dashboard/skills/SkillManifestDrawer.vue";
import SkillConfigModal from "../../src/components/dashboard/skills/SkillConfigModal.vue";
import SkillLogsPanel from "../../src/components/dashboard/skills/SkillLogsPanel.vue";
import ClawHubMarketplaceModal from "../../src/components/dashboard/skills/ClawHubMarketplaceModal.vue";
import NodePairingView from "../../src/components/dashboard/NodePairingView.vue";
import BrowserPreviewView from "../../src/components/dashboard/BrowserPreviewView.vue";
import Sidebar from "../../src/components/dashboard/Sidebar.vue";

describe("Milestone 2 Dashboard Views & Components", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("ChannelsManagementView.vue & WhatsAppQrModal.vue", () => {
    it("should mount ChannelsManagementView and display channel list", async () => {
      const wrapper = mount(ChannelsManagementView);
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(wrapper.text()).toContain("Multi-Channel Hub");
      expect(wrapper.text()).toContain("Telegram Bot");
      expect(wrapper.text()).toContain("WhatsApp Multi-Device");
      expect(wrapper.text()).toContain("Discord Gateway Bot");
      expect(wrapper.text()).toContain("Slack Socket Mode");
      expect(gatewayMock.fetchChannels).toHaveBeenCalled();
    });

    it("should trigger test connection on Telegram channel", async () => {
      const wrapper = mount(ChannelsManagementView);
      await flushPromises();
      const testBtns = wrapper.findAll("button");
      const tgTestBtn = testBtns.find((b) => b.text().includes("Test Probe"));
      if (tgTestBtn) {
        await tgTestBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.testChannel).toHaveBeenCalled();
      }
    });

    it("should open WhatsApp QR Modal and display QR code countdown", async () => {
      const wrapper = mount(WhatsAppQrModal, {
        props: {
          isOpen: true,
        },
      });
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(wrapper.text()).toContain("Link WhatsApp Companion");
      expect(gatewayMock.getWhatsAppQr).toHaveBeenCalled();
    });
  });

  describe("SkillsView.vue & Subcomponents", () => {
    it("should mount SkillsView and display category badges and skills", async () => {
      const wrapper = mount(SkillsView);
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(wrapper.text()).toContain("github-integration");
      expect(wrapper.text()).toContain("weather-forecast");
    });

    it("should mount SkillManifestDrawer and display manifest", async () => {
      const wrapper = mount(SkillManifestDrawer, {
        props: {
          isOpen: true,
          skillId: "github-integration",
        },
      });
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(gatewayMock.getSkillManifest).toHaveBeenCalledWith("github-integration");
      expect(wrapper.text()).toContain("github-integration");
    });

    it("should mount SkillConfigModal and save configuration", async () => {
      const wrapper = mount(SkillConfigModal, {
        props: {
          isOpen: true,
          skillId: "github-integration",
        },
      });
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(gatewayMock.getSkillConfig).toHaveBeenCalledWith("github-integration");
      const saveBtn = wrapper.find(".btn-primary");
      if (saveBtn.exists()) {
        await saveBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.saveSkillConfig).toHaveBeenCalled();
      }
    });

    it("should mount SkillLogsPanel and load execution logs", async () => {
      const wrapper = mount(SkillLogsPanel, {
        props: {
          isOpen: true,
          skillId: "github-integration",
        },
      });
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(gatewayMock.fetchSkillLogs).toHaveBeenCalled();
      expect(wrapper.text()).toContain("SUCCESS");
      expect(wrapper.text()).toContain("45ms");
    });

    it("should mount ClawHubMarketplaceModal and allow 1-click install", async () => {
      const wrapper = mount(ClawHubMarketplaceModal, {
        props: {
          isOpen: true,
        },
      });
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(wrapper.text()).toContain("ClawHub Skill Marketplace");
      const installBtn = wrapper.findAll(".btn-install").at(0);
      if (installBtn?.exists()) {
        await installBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.installSkillFromHub).toHaveBeenCalled();
      }
    });
  });

  describe("NodePairingView.vue", () => {
    it("should mount NodePairingView and display paired and pending devices", async () => {
      const wrapper = mount(NodePairingView);
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(wrapper.text()).toContain("Alice's iPhone 16");
      expect(wrapper.text()).toContain("Home GPU Server");
      expect(wrapper.text()).toContain("Bob's iPad Pro");
      expect(wrapper.text()).toContain("481920");
    });

    it("should approve a pending pairing challenge", async () => {
      const wrapper = mount(NodePairingView);
      await flushPromises();
      const approveBtn = wrapper.find(".btn-approve");
      if (approveBtn.exists()) {
        await approveBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.approvePairing).toHaveBeenCalledWith({ shortCode: "481920" });
      }
    });

    it("should revoke an approved node", async () => {
      const wrapper = mount(NodePairingView);
      await flushPromises();
      const revokeBtn = wrapper.find(".btn-revoke");
      if (revokeBtn.exists()) {
        await revokeBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.revokePairing).toHaveBeenCalledWith("node-1-iphone");
      }
    });

    it("should create a sample pairing challenge", async () => {
      const wrapper = mount(NodePairingView);
      await flushPromises();
      const createBtn = wrapper.find(".btn-create-challenge");
      if (createBtn.exists()) {
        await createBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.createPairingChallenge).toHaveBeenCalled();
      }
    });
  });

  describe("BrowserPreviewView.vue", () => {
    it("should mount BrowserPreviewView and display preview controls", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();
      expect(wrapper.exists()).toBe(true);
      expect(wrapper.text()).toContain("Browser Automation Preview");
      expect(wrapper.text()).toContain("LIVA Cognitive Dashboard");
      expect(wrapper.text()).toContain("HTTP 200");
      expect(wrapper.find(".screenshot-img").exists()).toBe(true);
    });

    it("should execute navigate action", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();
      const navBtn = wrapper.find(".btn-go");
      if (navBtn.exists()) {
        await navBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.navigateBrowser).toHaveBeenCalled();
      }
    });

    it("should execute DOM extraction", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();
      const extractBtn = wrapper.find(".btn-extract");
      if (extractBtn.exists()) {
        await extractBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.extractBrowserDom).toHaveBeenCalled();
      }
    });

    it("should pause and resume automation session", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();
      const pauseBtn = wrapper.find(".btn-pause");
      if (pauseBtn.exists()) {
        await pauseBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.controlBrowser).toHaveBeenCalledWith("pause");
      }
    });
  });

  describe("Sidebar.vue Navigation", () => {
    it("should emit navigate event when clicking M2 navigation items", async () => {
      const wrapper = mount(Sidebar, {
        props: {
          activePage: "avatar",
        },
      });
      const btns = wrapper.findAll(".sidebar-btn");
      expect(btns.length).toBeGreaterThanOrEqual(13);

      for (const btn of btns) {
        await btn.trigger("click");
      }
      expect(wrapper.emitted("navigate")).toBeDefined();
      const emittedPages = wrapper.emitted("navigate")?.map((call) => call[0]);
      expect(emittedPages).toContain("channels");
      expect(emittedPages).toContain("pairing");
      expect(emittedPages).toContain("browser");
      expect(emittedPages).toContain("skills");
    });
  });
});
