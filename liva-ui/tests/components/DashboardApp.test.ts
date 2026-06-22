import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount } from "@vue/test-utils";
import { ref } from "vue";

// Mock Node url to prevent JSDOM crash on Windows for root-relative URLs
vi.mock("url", async (importOriginal) => {
  const original = await importOriginal<typeof import("url")>();
  return {
    ...original,
    fileURLToPath: (url: string | URL) => {
      try {
        return original.fileURLToPath(url);
      } catch (e) {
        return "C:\\dummy_path";
      }
    },
  };
});

// Mock absolute asset path
vi.mock("/liva-logo.png", () => ({ default: "liva-logo.png" }));

// Mock useGateway
const userProfileRef = ref<any>({ name: "John" });
const isProfileLoadingRef = ref(false);
const gpuSetupStatusRef = ref(null);
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
});
const configDataRef = ref({
  ai: {
    provider: "local",
  },
});

const initMock = vi.fn();
const destroyMock = vi.fn();

vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => ({
    userProfile: userProfileRef,
    isProfileLoading: isProfileLoadingRef,
    gpuSetupStatus: gpuSetupStatusRef,
    systemStatus: systemStatusRef,
    configData: configDataRef,
    init: initMock,
    destroy: destroyMock,
  }),
}));

// Mock useI18n
vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (key: string) => key,
    currentLang: ref("en-US"),
  }),
}));

import DashboardApp from "../../src/DashboardApp.vue";

describe("DashboardApp.vue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should mount and render layout", () => {
    const wrapper = mount(DashboardApp, {
      global: {
        stubs: {
          TitleBar: true,
          Sidebar: true,
          StatusBar: true,
          OnboardingForm: true,
          AvatarGallery: true,
          AISettings: true,
          ApiManagementView: true,
          VoiceManagementView: true,
          TaskManager: true,
          MemoryViewer: true,
          SkillsView: true,
          SystemView: true,
          UserProfile: true,
          SettingsView: true,
        },
      },
    });

    expect(wrapper.exists()).toBe(true);
    expect(initMock).toHaveBeenCalled();

    vi.advanceTimersByTime(3500);
    
    wrapper.unmount();
    expect(destroyMock).toHaveBeenCalled();
  });
});
