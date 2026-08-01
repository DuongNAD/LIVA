import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";

// Mock absolute asset path
vi.mock("/liva-logo.png", () => ({ default: "liva-logo.png" }));

// Mock Tauri APIs
const mockMinimize = vi.fn();
const mockMaximize = vi.fn();
const mockHide = vi.fn();
const mockIsMaximized = vi.fn().mockResolvedValue(false);

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: () => mockMinimize(),
    maximize: () => mockMaximize(),
    unmaximize: vi.fn(),
    hide: () => mockHide(),
    isMaximized: () => mockIsMaximized(),
  }),
}));

// Mock @tauri-apps/plugin-dialog
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue("C:\\models_dir"),
}));

// Mock @tauri-apps/plugin-process
vi.mock("@tauri-apps/plugin-process", () => ({
  exit: vi.fn(),
}));

const platformMock = {
  platformName: "tauri" as const,
  hasVaultSecret: vi.fn().mockResolvedValue(false),
  storeVaultSecret: vi.fn().mockResolvedValue(undefined),
  deleteVaultSecret: vi.fn().mockResolvedValue(undefined),
};

vi.mock("../../src/platform", () => ({
  detectPlatform: () => platformMock,
}));

// Mock HardwareDetector
vi.mock("../../src/utils/HardwareDetector", () => ({
  detectOptimalEngine: () => "3D",
  profileHardware: () => ({ recommendedEngine: "3D", isWeakGPU: false }),
}));

// Mock useGateway
const userProfileRef = ref({
  name: "Alice",
  birthYear: "1990",
  nationality: "VN",
  language: "vi-VN",
  hobbies: "reading",
  preferences: "friendly",
});
const configDataRef = ref({
  ai: {
    provider: "local",
    routerModel: "model-v1",
    modelPath: "/path/to/model",
    temperature: 0.7,
    maxTokens: 2048,
    systemPrompt: "hello",
    apiKey: "abc",
    baseURL: "http://localhost",
  },
  avatar: {
    engineMode: "3D",
    vrmModel: "avatar.vrm",
    live2dModel: "pio.json",
  },
  voice: {
    inputDevice: "default",
    outputDevice: "default",
    voiceName: "en-US-1",
    sttEnabled: true,
    ttsEnabled: true,
  },
  system: {
    geolocationEnabled: true,
    digestInterestsEnabled: true,
    digestInterestsHour: 10,
    digestInterestsMinute: 0,
    digestInterestsDeliverUI: true,
    digestInterestsDeliverTelegram: false,
    digestInterestsDeliverZalo: false,
    digestInterestsDeliverEmail: false,
    digestFocusEnabled: true,
    digestFocusHour: 18,
    digestFocusMinute: 30,
    digestFocusDeliverUI: true,
    digestFocusDeliverTelegram: false,
    digestFocusDeliverZalo: false,
    digestFocusDeliverEmail: false,
    digestFocusTopics: "AI, Tech",
  }
});
const systemStatusRef = ref({
  healthChecks: {
    gateway: { status: "online" },
    aiEngine: { status: "online" },
    orchestrator: { status: "online" },
    voiceEngine: { status: "online" },
    memory: { status: "online" },
    vramGuard: { status: "online" },
    whisper: { status: "online" },
  },
  cpuUsage: 12,
  ramUsage: 45,
  vramUsage: 30,
  latencyMs: 15,
});
const isConnectedRef = ref(true);
const memoryDataRef = ref({
  facts: [
    { key: "k1", value: "v1", importance: 0.5, updatedAt: "2026-06-22", category: "user" }
  ],
  events: [
    { id: "e1", content: "event 1", timestamp: "2026-06-22" }
  ],
  vectors: [
    { id: "v1", content: "vector 1", score: 0.9 }
  ],
  l0: [
    { id: "l1", content: "l0 1" }
  ],
  l0_5: "L0.5 status content"
});
const tasksListRef = ref([
  { id: "1", title: "task 1", status: "pending", priority: "high", category: "work" },
]);
const skillsListRef = ref([
  { name: "skill 1", description: "desc", enabled: true, status: "active" },
]);
const voiceStatusRef = ref({
  activeProfile: "vi-VN-HoaiMyNeural",
  provider: "hybrid",
  language: "vi-VN",
  sampleRate: 16000,
  trainingEnabled: false,
});
const voiceProfilesRef = ref([
  { id: "vp1", name: "Hoai My", lang: "vi-VN" },
  { id: "vp2", name: "Guy", lang: "en-US" }
]);

const registeredCallbacks = new Map<string, Function>();

const gatewayMock = {
  userProfile: userProfileRef,
  configData: configDataRef,
  systemStatus: systemStatusRef,
  isConnected: isConnectedRef,
  memoryData: memoryDataRef,
  tasksList: tasksListRef,
  skillsList: skillsListRef,
  voiceStatus: voiceStatusRef,
  voiceProfiles: voiceProfilesRef,
  // VoiceManagementView đọc bốn ref này ngay trong setup() (computed + watch);
  // thiếu bất kỳ ref nào thì mount đổ ở "Cannot read properties of undefined
  // (reading 'value')".
  // ObservationConsentPanel (nhúng trong SettingsView) đọc ref này trong một
  // computed ngay ở setup().
  observationConsent: ref({ granted: false, active: false, updatedAt: null as number | null }),
  vieneuVoices: ref<{ id: string; name: string }[]>([]),
  vieneuCurrent: ref<string | null>(null),
  vieneuEnabled: ref(false),
  vieneuNotice: ref(""),
  avatarModels3D: ref([
    { filename: "models/avatar1.vrm", format: "vrm" },
    { filename: "models/avatar2.fbx", format: "fbx" }
  ]),
  avatarModels2D: ref([
    { filename: "models/live2d/hime.json", format: "live2d" }
  ]),
  init: vi.fn(),
  destroy: vi.fn(),
  sendMsg: vi.fn(),
  updateConfig: vi.fn(),
  onMemoryUpdated: vi.fn(),
  offMemoryUpdated: vi.fn(),
  onTaskPlanReply: vi.fn(),
  onSkillCheckResult: vi.fn(),
  onAllSkillsCheckComplete: vi.fn(),
  offSkillCheckResult: vi.fn(),
  offAllSkillsCheckComplete: vi.fn(),
  onEnvConfigData: vi.fn((cb) => { registeredCallbacks.set("onEnvConfigData", cb); }),
  offEnvConfigData: vi.fn(),
  onMemoryResetResult: vi.fn((cb) => { registeredCallbacks.set("onMemoryResetResult", cb); }),
  offMemoryResetResult: vi.fn(),
};

vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => gatewayMock,
}));

// Mock useI18n
vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (key: string) => `translated_${key}`,
    currentLang: ref("en-US"),
  }),
}));

import AISettings from "../../src/components/dashboard/AISettings.vue";
import ApiManagementView from "../../src/components/dashboard/ApiManagementView.vue";
import AvatarGallery from "../../src/components/dashboard/AvatarGallery.vue";
import MemoryViewer from "../../src/components/dashboard/MemoryViewer.vue";
import SettingsView from "../../src/components/dashboard/SettingsView.vue";
import SkillsView from "../../src/components/dashboard/SkillsView.vue";
import SystemView from "../../src/components/dashboard/SystemView.vue";
import TaskManager from "../../src/components/dashboard/TaskManager.vue";
import TitleBar from "../../src/components/dashboard/TitleBar.vue";
import VoiceManagementView from "../../src/components/dashboard/VoiceManagementView.vue";

describe("Dashboard Views", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    registeredCallbacks.clear();
    platformMock.hasVaultSecret.mockResolvedValue(false);
    platformMock.storeVaultSecret.mockResolvedValue(undefined);
  });

  it("should mount and exercise AISettings.vue", async () => {
    const wrapper = mount(AISettings);
    expect(wrapper.exists()).toBe(true);

    // Click provider buttons to toggle
    const providerBtns = wrapper.findAll(".provider-btn");
    if (providerBtns.length >= 2) {
      await providerBtns[1].trigger("click"); // Switch to cloud
      expect(wrapper.vm.provider).toBe("cloud");
      await providerBtns[0].trigger("click"); // Switch back to local
      expect(wrapper.vm.provider).toBe("local");
    }

    // Save config
    const saveBtn = wrapper.find(".btn-primary");
    if (saveBtn.exists()) {
      (wrapper.vm as any).cloudApiKey = "new-cloud-secret";
      await saveBtn.trigger("click");
      await flushPromises();
      expect(gatewayMock.updateConfig).toHaveBeenCalled();
      const configPatch = gatewayMock.updateConfig.mock.calls.at(-1)?.[0];
      expect(configPatch.ai).not.toHaveProperty("cloudApiKey");
      expect(platformMock.storeVaultSecret).toHaveBeenCalledWith(
        "ai/cloud_api_key",
        "new-cloud-secret",
      );
    }
  });

  it("should mount and exercise ApiManagementView.vue", async () => {
    const wrapper = mount(ApiManagementView);
    expect(wrapper.exists()).toBe(true);
    await flushPromises();
    expect(gatewayMock.onEnvConfigData).not.toHaveBeenCalled();
    expect(platformMock.hasVaultSecret).toHaveBeenCalledWith("ai/cloud_api_key");

    const saveBtn = wrapper.find(".btn-primary");
    if (saveBtn.exists()) {
      (wrapper.vm as any).useCloudAI = true;
      (wrapper.vm as any).aiApiKey = "new-ai-secret";
      await saveBtn.trigger("click");
      await flushPromises();
      expect(platformMock.storeVaultSecret).toHaveBeenCalledWith(
        "ai/cloud_api_key",
        "new-ai-secret",
      );
      expect(gatewayMock.updateConfig).toHaveBeenCalled();
      expect(gatewayMock.sendMsg).not.toHaveBeenCalledWith(
        "save_env_config",
        expect.any(Object),
      );
    }
  });

  it("should mount and exercise AvatarGallery.vue", async () => {
    const wrapper = mount(AvatarGallery);
    expect(wrapper.exists()).toBe(true);

    // Switch tabs
    const tabHeaders = wrapper.findAll(".tab-header");
    if (tabHeaders.length >= 2) {
      await tabHeaders[1].trigger("click");
      await tabHeaders[0].trigger("click");
    }

    // Trigger folder pick
    const folderPickBtn = wrapper.find(".btn-secondary");
    if (folderPickBtn.exists()) {
      await folderPickBtn.trigger("click");
    }

    // Click model card
    const card = wrapper.find(".model-card");
    if (card.exists()) {
      await card.trigger("click");
      expect(gatewayMock.updateConfig).toHaveBeenCalled();
    }
  });

  it("should mount and exercise MemoryViewer.vue", async () => {
    const wrapper = mount(MemoryViewer);
    expect(wrapper.exists()).toBe(true);
    const btn = wrapper.find(".btn-danger");
    if (btn.exists()) {
      await btn.trigger("click");
    }
  });

  it("should mount and exercise SettingsView.vue", async () => {
    const wrapper = mount(SettingsView);
    expect(wrapper.exists()).toBe(true);

    // Trigger settings change to save settings
    const input = wrapper.find('input[type="number"]');
    if (input.exists()) {
      await input.setValue(22);
      await input.trigger("change");
      expect(gatewayMock.updateConfig).toHaveBeenCalled();
    }

    // Trigger memory reset confirmations
    const resetBtn = wrapper.find(".btn-danger");
    if (resetBtn.exists()) {
      await resetBtn.trigger("click"); // Opens confirm modal
      
      const confirmBtn = wrapper.find(".modal-actions .btn-danger");
      if (confirmBtn.exists()) {
        await confirmBtn.trigger("click");
        expect(gatewayMock.sendMsg).toHaveBeenCalledWith(
          "memory:delete_subject",
          { dryRun: false },
        );

        const resetResultCb = registeredCallbacks.get("onMemoryResetResult");
        if (resetResultCb) {
          resetResultCb({ success: true });
        }
      }
    }
  });

  it("should mount and exercise SkillsView.vue", async () => {
    const wrapper = mount(SkillsView);
    expect(wrapper.exists()).toBe(true);
    const btn = wrapper.find("button.btn-primary");
    if (btn.exists()) {
      await btn.trigger("click");
    }
  });

  it("should mount and exercise SystemView.vue", async () => {
    const wrapper = mount(SystemView);
    expect(wrapper.exists()).toBe(true);
  });

  it("should mount and exercise TaskManager.vue", async () => {
    const wrapper = mount(TaskManager);
    expect(wrapper.exists()).toBe(true);
    const input = wrapper.find('input[type="text"]');
    if (input.exists()) {
      await input.setValue("New Task Title");
    }
    const form = wrapper.find("form");
    if (form.exists()) {
      await form.trigger("submit.prevent");
    }
  });

  it("should mount and exercise TitleBar.vue", async () => {
    const wrapper = mount(TitleBar);
    expect(wrapper.exists()).toBe(true);
    const btns = wrapper.findAll(".titlebar-btn");
    for (const btn of btns) {
      await btn.trigger("click");
    }
  });

  it("should mount and exercise VoiceManagementView.vue", async () => {
    const wrapper = mount(VoiceManagementView);
    expect(wrapper.exists()).toBe(true);

    // Chọn nút theo nhãn, không theo thứ tự `.btn-primary`: khối VieNeu được chèn
    // lên đầu view nên mọi chỉ số cứng đều trượt sang nút khác.
    const byLabel = (label: string) =>
      wrapper.findAll("button").find((btn) => btn.text().includes(label));

    // Save voice config
    const saveBtn = byLabel("Lưu voice");
    if (saveBtn?.exists()) {
      await saveBtn.trigger("click");
      expect(gatewayMock.sendMsg).toHaveBeenCalledWith("update_config", expect.any(Object));
    }

    // Start training
    const startTrainingBtn = byLabel("Start training");
    if (startTrainingBtn?.exists()) {
      await startTrainingBtn.trigger("click");
      expect(gatewayMock.sendMsg).toHaveBeenCalledWith("start_voice_training", expect.any(Object));
    }

    // Stop training
    const stopTrainingBtn = wrapper.find(".btn-danger");
    if (stopTrainingBtn.exists()) {
      await stopTrainingBtn.trigger("click");
      expect(gatewayMock.sendMsg).toHaveBeenCalledWith("stop_voice_training");
    }

    // Click profile card
    const profileCard = wrapper.find(".profile-card");
    if (profileCard.exists()) {
      await profileCard.trigger("click");
      expect(gatewayMock.sendMsg).toHaveBeenCalledWith("select_voice_profile", expect.any(Object));
    }
  });
});
