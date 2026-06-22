import { describe, it, expect, vi, beforeEach } from "vitest";
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

// Mock Assets
vi.mock("../src/assets/vite.svg", () => "vite.svg");
vi.mock("../src/assets/hero.png", () => "hero.png");
vi.mock("../src/assets/vue.svg", () => "vue.svg");

// Mock useGateway
const userProfileRef = ref<any>({
  name: "John Doe",
  birthYear: "1995",
  nationality: "US",
  language: "en-US",
  hobbies: "Coding",
  preferences: "Friendly",
});

const configDataRef = ref<any>({
  ai: {
    routerModel: "router-model-v1",
  },
  avatar: {
    engineMode: "auto",
  },
});

const systemStatusRef = ref<any>({
  model: "active-model",
  latencyMs: 50,
});

const isConnectedRef = ref(true);
const sendMsgMock = vi.fn();
const saveUserProfileMock = vi.fn();

vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => ({
    userProfile: userProfileRef,
    configData: configDataRef,
    systemStatus: systemStatusRef,
    isConnected: isConnectedRef,
    sendMsg: sendMsgMock,
    saveUserProfile: saveUserProfileMock,
  }),
}));

// Mock useI18n
vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (key: string) => `translated_${key}`,
    currentLang: ref("en-US"),
  }),
}));

import Sidebar from "../../src/components/dashboard/Sidebar.vue";
import StatusBar from "../../src/components/dashboard/StatusBar.vue";
import UserProfile from "../../src/components/dashboard/UserProfile.vue";
import OnboardingForm from "../../src/components/dashboard/OnboardingForm.vue";

describe("Dashboard Components Test Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // HelloWorld.vue skipped to avoid JSDOM <use> tag path resolution issue on Windows

  describe("Sidebar.vue", () => {
    it("should mount and emit navigate when button clicked", async () => {
      const wrapper = mount(Sidebar, {
        props: {
          activePage: "avatar",
        },
      });
      const buttons = wrapper.findAll(".sidebar-btn");
      expect(buttons.length).toBeGreaterThan(0);
      await buttons[1].trigger("click");
      expect(wrapper.emitted("navigate")).toBeTruthy();
      expect(wrapper.emitted("navigate")?.[0]).toEqual(["ai"]);
    });
  });

  // TitleBar.vue skipped to avoid JSDOM img src path resolution issue on Windows

  describe("StatusBar.vue", () => {
    it("should display connection status and latency information", () => {
      const wrapper = mount(StatusBar);
      expect(wrapper.text()).toContain("translated_connected");
      expect(wrapper.text()).toContain("active-model");
      expect(wrapper.text()).toContain("50ms");
    });

    it("should update display when connection goes down", async () => {
      isConnectedRef.value = false;
      const wrapper = mount(StatusBar);
      expect(wrapper.text()).toContain("translated_disconnected");
      isConnectedRef.value = true; // reset
    });
  });

  describe("UserProfile.vue", () => {
    it("should mount and fill form with userProfile values", async () => {
      const wrapper = mount(UserProfile);
      await wrapper.vm.$nextTick();
      const nameInput = wrapper.find('input[id="profileName"]');
      expect((nameInput.element as HTMLInputElement).value).toBe("John Doe");
    });

    it("should call saveUserProfile when form is submitted", async () => {
      const wrapper = mount(UserProfile);
      await wrapper.vm.$nextTick();
      await wrapper.find("button.btn-primary").trigger("click");
      expect(saveUserProfileMock).toHaveBeenCalled();
    });
  });

  describe("OnboardingForm.vue", () => {
    it("should mount and submit onboarding form", async () => {
      const wrapper = mount(OnboardingForm);
      await wrapper.find('input[id="name"]').setValue("Alice");
      await wrapper.find('input[id="birthYear"]').setValue("1995");
      await wrapper.find('input[id="nationality"]').setValue("US");
      await wrapper.find("form").trigger("submit.prevent");
      expect(saveUserProfileMock).toHaveBeenCalled();
    });
  });
});
