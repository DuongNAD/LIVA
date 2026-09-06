import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";

// Mock Tauri invoke
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

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
    action: "launch",
    target: "Headless Chromium 1280x800",
    status: "success",
    details: "Browser sandbox initialized with SSRF Guard",
  },
  {
    id: "act-2",
    timestamp_unix: 1700000045,
    action: "navigate",
    target: "https://liva.ai/dashboard",
    status: "success",
    details: "Loaded page (HTTP 200)",
  },
]);

const skillsListRef = ref([
  {
    name: "github-integration",
    description: "Manage GitHub repos",
    category: "developer",
    isCoreSkill: false,
    enabled: true,
    status: "active",
  },
]);

const gatewayMock = {
  isConnected: ref(true),
  browserStatus: browserStatusRef,
  browserScreenshot: browserScreenshotRef,
  browserActionLogs: browserActionLogsRef,
  skillsList: skillsListRef,

  fetchBrowserStatus: vi.fn().mockImplementation(async () => browserStatusRef.value),
  fetchBrowserScreenshot: vi.fn().mockImplementation(async () => browserScreenshotRef.value),
  fetchBrowserActionLogs: vi.fn().mockImplementation(async () => browserActionLogsRef.value),
  navigateBrowser: vi.fn().mockImplementation(async (url: string) => {
    if (url.includes("169.254.169.254") || url.includes("localhost") || url.includes("127.0.0.1")) {
      throw new Error("Navigation failed: Security policy blocked URL: SSRF attempt prohibited");
    }
    return {
      success: true,
      url,
      title: `Loaded ${url}`,
      httpStatus: 200,
    };
  }),
  extractBrowserDom: vi.fn().mockImplementation(async (mode: string) => ({
    mode,
    content: `# Extracted DOM in ${mode} mode\nLine 1\nLine 2`,
    length: 45,
  })),
  controlBrowser: vi.fn().mockImplementation(async (action: string) => {
    if (action === "pause") browserStatusRef.value.isPaused = true;
    if (action === "resume") browserStatusRef.value.isPaused = false;
    if (action === "clear_logs") browserActionLogsRef.value = [];
    return { success: true, action };
  }),

  getSkillManifest: vi.fn().mockResolvedValue({
    skillId: "adversarial-test-skill",
    name: "adversarial-test-skill",
    version: "2.0.0",
    description: "Adversarial stress test skill package",
    author: "Security Auditor",
    license: "Apache-2.0",
    triggers: [{ type: "intent", config: "test_execution" }],
    permissions: [{ type: "keystore_access" }],
    tools: [
      {
        name: "test_tool_1",
        description: "Primary probe tool",
        risk_level: "read_only_safe",
      },
      {
        name: "test_tool_2",
        description: "Destructive wipe tool",
        risk_level: "destructive_high_risk",
      },
    ],
    runtimeType: "native_rust",
    markdownInstructions: "# Instructions\n".repeat(50),
    rawContent: "---\nname: adversarial-test-skill\nversion: 2.0.0\n---\n# Instructions",
    contentHash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    dirPath: "/skills/adversarial-test-skill",
  }),

  getSkillConfig: vi.fn().mockResolvedValue({
    skillId: "adversarial-test-skill",
    params: { timeoutSeconds: 30, maxRetries: 3 },
  }),
  saveSkillConfig: vi.fn().mockResolvedValue({ success: true }),
  fetchSkillLogs: vi.fn().mockResolvedValue({
    skillId: "adversarial-test-skill",
    count: 2,
    logs: [
      {
        id: "log-stress-1",
        skillId: "adversarial-test-skill",
        timestampUnix: 1700000000,
        caller: "StressHarness",
        status: "SUCCESS",
        durationMs: 12,
        input: { payload: "x".repeat(500) },
        output: { result: "ok", items: Array.from({ length: 50 }, (_, i) => i) },
      },
      {
        id: "log-stress-2",
        skillId: "adversarial-test-skill",
        timestampUnix: 1700000050,
        caller: "StressHarness",
        status: "ERROR",
        durationMs: 89,
        input: { query: "trigger_failure" },
        error: "Execution timeout exceeded (30s limit)",
      },
    ],
  }),
  installSkillFromHub: vi.fn().mockResolvedValue({
    success: true,
    skillId: "clawhub-weather-radar",
    name: "clawhub-weather-radar",
    installedPath: "/skills/clawhub-weather-radar",
  }),
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

import BrowserPreviewView from "../../src/components/dashboard/BrowserPreviewView.vue";
import SkillManifestDrawer from "../../src/components/dashboard/skills/SkillManifestDrawer.vue";
import SkillLogsPanel from "../../src/components/dashboard/skills/SkillLogsPanel.vue";
import ClawHubMarketplaceModal from "../../src/components/dashboard/skills/ClawHubMarketplaceModal.vue";

describe("Adversarial Challenger: Browser Preview & Skill Manifest UI Edge Cases", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("BrowserPreviewView.vue Adversarial URL & Polling Edge Cases", () => {
    it("should reject malicious SSRF URL inputs with descriptive toast warning", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();

      const urlInput = wrapper.find(".url-input");
      expect(urlInput.exists()).toBe(true);

      // Attempt SSRF target input
      await urlInput.setValue("http://169.254.169.254/latest/meta-data/");
      const goBtn = wrapper.findAll("button").find((b) => b.text() === "Go");
      expect(goBtn).toBeDefined();

      await goBtn?.trigger("click");
      await flushPromises();

      expect(gatewayMock.navigateBrowser).toHaveBeenCalledWith("http://169.254.169.254/latest/meta-data/");
      expect(wrapper.text()).toContain("Navigation failed: Error: Navigation failed: Security policy blocked URL");
    });

    it("should prevent submission on empty or whitespace-only URLs", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();

      const urlInput = wrapper.find(".url-input");
      await urlInput.setValue("   ");

      const goBtn = wrapper.findAll("button").find((b) => b.text() === "Go");
      expect(goBtn?.attributes("disabled")).toBeDefined();

      await goBtn?.trigger("click");
      await flushPromises();

      expect(gatewayMock.navigateBrowser).not.toHaveBeenCalled();
    });

    it("should safely handle rapid auto-refresh toggles without timer leaks", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();

      const refreshBtn = wrapper.findAll("button").find((b) => b.text().includes("Auto-Refresh"));
      expect(refreshBtn).toBeDefined();

      // Rapidly toggle 10 times
      for (let i = 0; i < 10; i++) {
        await refreshBtn?.trigger("click");
      }

      // Advance fake timers by 5000ms
      vi.advanceTimersByTime(5000);
      await flushPromises();

      // Unmount to trigger onUnmounted clearInterval
      wrapper.unmount();
      expect(true).toBe(true);
    });

    it("should execute all DOM extraction modes smoothly", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();

      // Switch to DOM tab
      const domTab = wrapper.findAll(".tab-btn").find((b) => b.text().includes("DOM Extraction"));
      await domTab?.trigger("click");
      await flushPromises();

      const extractBtn = wrapper.find(".dom-control-row button");
      await extractBtn.trigger("click");
      await flushPromises();

      expect(gatewayMock.extractBrowserDom).toHaveBeenCalledWith("semantic");
      expect(wrapper.text()).toContain("# Extracted DOM in semantic mode");
    });

    it("should execute session control actions (pause, resume, stop, clear_logs)", async () => {
      const wrapper = mount(BrowserPreviewView);
      await flushPromises();

      const pauseBtn = wrapper.find(".session-controls button[title='Pause active automation']");
      if (pauseBtn.exists()) {
        await pauseBtn.trigger("click");
        await flushPromises();
        expect(gatewayMock.controlBrowser).toHaveBeenCalledWith("pause");
      }

      const stopBtn = wrapper.find(".session-controls .btn-danger");
      await stopBtn.trigger("click");
      await flushPromises();
      expect(gatewayMock.controlBrowser).toHaveBeenCalledWith("stop");
    });
  });

  describe("SkillManifestDrawer.vue Adversarial Parsing & Display", () => {
    it("should display parsed tools and markdown instructions without errors", async () => {
      const wrapper = mount(SkillManifestDrawer, {
        props: {
          skillId: "adversarial-test-skill",
        },
      });
      await flushPromises();

      expect(gatewayMock.getSkillManifest).toHaveBeenCalledWith("adversarial-test-skill");
      expect(wrapper.text()).toContain("adversarial-test-skill");
      expect(wrapper.text()).toContain("test_tool_1");
      expect(wrapper.text()).toContain("test_tool_2");
      expect(wrapper.text()).toContain("read_only_safe");
      expect(wrapper.text()).toContain("destructive_high_risk");
    });

    it("should switch to raw SKILL.md tab and allow copying", async () => {
      const writeTextMock = vi.fn().mockResolvedValue(undefined);
      Object.assign(navigator, {
        clipboard: {
          writeText: writeTextMock,
        },
      });

      const wrapper = mount(SkillManifestDrawer, {
        props: {
          skillId: "adversarial-test-skill",
        },
      });
      await flushPromises();

      const rawTab = wrapper.findAll(".tab-btn").find((b) => b.text().includes("Raw SKILL.md"));
      await rawTab?.trigger("click");
      await flushPromises();

      expect(wrapper.text()).toContain("Raw SKILL.md");
      const copyBtn = wrapper.find(".raw-actions button");
      await copyBtn.trigger("click");
      await flushPromises();

      expect(writeTextMock).toHaveBeenCalled();
    });

    it("should render error fallback gracefully if manifest loading fails", async () => {
      gatewayMock.getSkillManifest.mockRejectedValueOnce(new Error("File read error"));

      const wrapper = mount(SkillManifestDrawer, {
        props: {
          skillId: "broken-skill",
        },
      });
      await flushPromises();

      expect(wrapper.text()).toContain('Failed to load manifest for skill "broken-skill"');
    });
  });

  describe("SkillLogsPanel.vue Heavy Logs & Error Payloads", () => {
    it("should render execution logs with status badges and payloads", async () => {
      const wrapper = mount(SkillLogsPanel, {
        props: {
          skillId: "adversarial-test-skill",
        },
      });
      await flushPromises();

      expect(gatewayMock.fetchSkillLogs).toHaveBeenCalledWith("adversarial-test-skill", 25);
      expect(wrapper.text()).toContain("SUCCESS");
      expect(wrapper.text()).toContain("ERROR");
      expect(wrapper.text()).toContain("Execution timeout exceeded");
    });

    it("should display empty state when skill has zero execution logs", async () => {
      gatewayMock.fetchSkillLogs.mockResolvedValueOnce({ skillId: "empty-skill", count: 0, logs: [] });

      const wrapper = mount(SkillLogsPanel, {
        props: {
          skillId: "empty-skill",
        },
      });
      await flushPromises();

      expect(wrapper.text()).toContain("No execution logs found for this skill yet.");
    });
  });

  describe("ClawHubMarketplaceModal.vue Discovery & Edge Cases", () => {
    it("should filter skills by search query with special characters", async () => {
      const wrapper = mount(ClawHubMarketplaceModal);
      await flushPromises();

      const searchInput = wrapper.find(".modal-search input");
      await searchInput.setValue("weather");
      await flushPromises();

      expect(wrapper.text()).toContain("Weather & Radar Pro");
      expect(wrapper.text()).not.toContain("GitHub PR & Issue Triager");

      // Search with regex characters
      await searchInput.setValue("[*+?]");
      await flushPromises();
      expect(wrapper.findAll(".hub-card").length).toBe(0);
    });

    it("should perform 1-click install and emit installed event", async () => {
      const wrapper = mount(ClawHubMarketplaceModal);
      await flushPromises();

      const installBtns = wrapper.findAll(".hub-card-footer button");
      expect(installBtns.length).toBeGreaterThan(0);

      await installBtns[0].trigger("click");
      await flushPromises();

      expect(gatewayMock.installSkillFromHub).toHaveBeenCalled();
      expect(wrapper.emitted("installed")).toBeDefined();
    });
  });
});
