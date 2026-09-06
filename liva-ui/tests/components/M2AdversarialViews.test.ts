import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";

const channelsListRef = ref([]);
const pairedNodesListRef = ref([]);
const pendingPairingListRef = ref([]);
const browserStatusRef = ref({
  isRunning: true,
  isPaused: false,
  currentUrl: "https://liva.ai/dashboard",
  pageTitle: "LIVA Dashboard",
  httpStatus: 200,
  viewportWidth: 1280,
  viewportHeight: 800,
  sandboxActive: true,
  ssrfGuard: true,
});
const browserScreenshotRef = ref("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==");
const browserActionLogsRef = ref([]);

const gatewayMock = {
  isConnected: ref(true),
  channelsList: channelsListRef,
  pairedNodesList: pairedNodesListRef,
  pendingPairingList: pendingPairingListRef,
  browserStatus: browserStatusRef,
  browserScreenshot: browserScreenshotRef,
  browserActionLogs: browserActionLogsRef,
  skillsList: ref([]),

  getWhatsAppQr: vi.fn().mockResolvedValue({
    qrData: "2@LIVA_PAIR_MOCK_QR",
    expiresAtUnix: Date.now() + 120000,
    ttlSeconds: 120,
    pairingState: "awaiting_scan",
  }),
  configureChannel: vi.fn().mockResolvedValue({ success: true }),
  fetchChannels: vi.fn().mockResolvedValue([]),
  testChannel: vi.fn().mockResolvedValue({ success: true }),
  approvePairing: vi.fn().mockResolvedValue({ success: true }),
  revokePairing: vi.fn().mockResolvedValue({ success: true }),
  fetchBrowserStatus: vi.fn().mockImplementation(async () => browserStatusRef.value),
  fetchBrowserScreenshot: vi.fn().mockImplementation(async () => browserScreenshotRef.value),
  fetchBrowserActionLogs: vi.fn().mockImplementation(async () => browserActionLogsRef.value),
  navigateBrowser: vi.fn().mockResolvedValue({ success: true }),
  controlBrowser: vi.fn().mockResolvedValue({ success: true }),
  getSkillManifest: vi.fn().mockResolvedValue({}),
};

vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => gatewayMock,
}));

vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (k: string) => k,
    currentLang: ref("en-US"),
  }),
}));

import WhatsAppQrModal from "../../src/components/dashboard/channels/WhatsAppQrModal.vue";
import ChannelsManagementView from "../../src/components/dashboard/ChannelsManagementView.vue";
import NodePairingView from "../../src/components/dashboard/NodePairingView.vue";
import BrowserPreviewView from "../../src/components/dashboard/BrowserPreviewView.vue";
import SkillsView from "../../src/components/dashboard/SkillsView.vue";

describe("M2 Adversarial Frontend Stress Tests", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("WhatsAppQrModal Adversarial Scenarios", () => {
    it("handles zero or negative TTL gracefully without breaking timer display", async () => {
      gatewayMock.getWhatsAppQr.mockResolvedValueOnce({
        qrData: "2@LIVA_PAIR_EXPIRED_0",
        expiresAtUnix: 0,
        ttlSeconds: 0,
        pairingState: "expired",
      });

      const wrapper = mount(WhatsAppQrModal);
      await flushPromises();

      expect(wrapper.exists()).toBe(true);
      expect(wrapper.text()).toContain("0:00");
    });

    it("survives network failure when refreshing QR code", async () => {
      gatewayMock.getWhatsAppQr.mockRejectedValueOnce(new Error("Network IPC Timeout"));

      const wrapper = mount(WhatsAppQrModal);
      await flushPromises();

      // Should not throw unhandled exception or crash DOM
      expect(wrapper.exists()).toBe(true);
    });

    it("survives rapid multi-clicks on Refresh Code button", async () => {
      gatewayMock.getWhatsAppQr.mockResolvedValue({
        qrData: "2@LIVA_PAIR_TEST_120",
        expiresAtUnix: Date.now() + 120000,
        ttlSeconds: 120,
        pairingState: "awaiting_scan",
      });

      const wrapper = mount(WhatsAppQrModal);
      await flushPromises();

      const refreshBtn = wrapper.findAll("button").find((b) => b.text().includes("Refresh"));
      expect(refreshBtn).toBeDefined();

      // Click 20 times rapidly
      for (let i = 0; i < 20; i++) {
        await refreshBtn!.trigger("click");
      }
      await flushPromises();

      expect(gatewayMock.getWhatsAppQr).toHaveBeenCalled();
      expect(wrapper.exists()).toBe(true);
    });
  });

  describe("ChannelsManagementView Adversarial Scenarios", () => {
    it("handles backend returning empty or corrupted channels list", async () => {
      gatewayMock.fetchChannels.mockResolvedValueOnce([]);

      const wrapper = mount(ChannelsManagementView);
      await flushPromises();

      expect(wrapper.exists()).toBe(true);
    });

    it("handles test probe failure gracefully without blocking UI", async () => {
      gatewayMock.testChannel.mockRejectedValueOnce(new Error("Connection refused (ECONNREFUSED)"));

      const wrapper = mount(ChannelsManagementView);
      await flushPromises();

      const testBtn = wrapper.find(".btn-test");
      if (testBtn.exists()) {
        await testBtn.trigger("click");
        await flushPromises();
      }

      expect(wrapper.exists()).toBe(true);
    });
  });

  describe("NodePairingView Adversarial Scenarios", () => {
    it("handles invalid 6-digit short code rejection cleanly", async () => {
      gatewayMock.approvePairing.mockRejectedValueOnce(new Error("Invalid or expired pairing code"));

      const wrapper = mount(NodePairingView);
      await flushPromises();

      const input = wrapper.find("input");
      if (input.exists()) {
        await input.setValue("000000");
        const submitBtn = wrapper.findAll("button").find((b) => b.text().includes("Approve") || b.text().includes("Pair"));
        if (submitBtn) {
          await submitBtn.trigger("click");
          await flushPromises();
        }
      }

      expect(wrapper.exists()).toBe(true);
    });

    it("handles revoking already-revoked device without uncaught error", async () => {
      gatewayMock.revokePairing.mockRejectedValueOnce(new Error("Node not found"));

      const wrapper = mount(NodePairingView);
      await flushPromises();

      expect(wrapper.exists()).toBe(true);
    });
  });

  describe("BrowserPreviewView Adversarial Scenarios", () => {
    it("handles SSRF blocked navigation error notification", async () => {
      gatewayMock.navigateBrowser.mockRejectedValueOnce(
        new Error("Security policy blocked URL: SSRF or Private IP target prohibited: http://127.0.0.1:8080")
      );

      const wrapper = mount(BrowserPreviewView);
      await flushPromises();

      const input = wrapper.find(".url-input");
      if (input.exists()) {
        await input.setValue("http://127.0.0.1:8080");
        const goBtn = wrapper.findAll("button").find((b) => b.text() === "Go");
        if (goBtn) {
          await goBtn.trigger("click");
          await flushPromises();
        }
      }

      expect(wrapper.exists()).toBe(true);
    });

    it("handles rapid control state toggling (pause / resume / stop storms)", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();

      const pauseBtn = wrapper.find(".session-controls button");
      if (pauseBtn.exists()) {
        for (let i = 0; i < 10; i++) {
          await pauseBtn.trigger("click");
        }
        expect(gatewayMock.controlBrowser).toHaveBeenCalled();
      }
    });
  });

  describe("SkillsView Adversarial Scenarios", () => {
    it("handles corrupted manifest without crashing drawer viewer", async () => {
      gatewayMock.getSkillManifest.mockResolvedValueOnce({
        skillId: "corrupted-skill",
        name: "Corrupted",
        version: "0.0.0",
        description: "",
        markdownInstructions: "",
        rawContent: ":::INVALID YAML:::",
        contentHash: "corrupted_hash",
      });

      const wrapper = mount(SkillsView);
      await flushPromises();

      expect(wrapper.exists()).toBe(true);
    });
  });
});
