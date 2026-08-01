import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";

const visionAnswer = ref("");
const visionBusy = ref(false);
const visionError = ref("");
const isConnected = ref(true);
const watchActive = ref(false);
const watchStarting = ref(false);
const watchError = ref("");
const watchLastDiff = ref(0);
const watchEvents = ref<Array<{ time: string; difference: number }>>([]);
const askVision = vi.fn();
const startScreenWatch = vi.fn();
const stopScreenWatch = vi.fn();

vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => ({
    askVision,
    visionAnswer,
    visionBusy,
    visionError,
    isConnected,
    watchActive,
    watchStarting,
    watchError,
    watchLastDiff,
    watchEvents,
    startScreenWatch,
    stopScreenWatch,
  }),
}));

import VisionView from "../../src/components/dashboard/VisionView.vue";

describe("VisionView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    visionAnswer.value = "";
    visionBusy.value = false;
    visionError.value = "";
    isConnected.value = true;
    watchActive.value = false;
    watchStarting.value = false;
    watchError.value = "";
    watchLastDiff.value = 0;
    watchEvents.value = [];
  });

  it("gửi câu hỏi bằng nút và Enter, nhưng chặn khi vision đang bận", async () => {
    const wrapper = mount(VisionView);
    const input = wrapper.find("textarea");

    await input.setValue("Đang mở cửa sổ nào?");
    await wrapper.find(".vision-controls .vision-btn").trigger("click");
    await input.trigger("keydown", { key: "Enter" });
    expect(askVision).toHaveBeenNthCalledWith(1, "Đang mở cửa sổ nào?");
    expect(askVision).toHaveBeenNthCalledWith(2, "Đang mở cửa sổ nào?");

    visionBusy.value = true;
    await wrapper.vm.$nextTick();
    await input.trigger("keydown", { key: "Enter" });
    expect(askVision).toHaveBeenCalledTimes(2);
    expect(wrapper.find(".spinner").exists()).toBe(true);
  });

  it.each([
    ["timeout", "Hết thời gian chờ"],
    ["not_connected", "Chưa kết nối"],
    ["vision requires a release build", "Vision cần bản build release"],
    ["cannot load mmproj", "Chưa cấu hình mô hình thị giác"],
    ["lỗi lạ", "lỗi lạ"],
  ])("dịch lỗi %s thành hướng khắc phục", async (code, expected) => {
    visionError.value = code;
    const wrapper = mount(VisionView);
    expect(wrapper.find(".vision-error").text()).toContain(expected);
  });

  it("hiện kết quả, bật/dừng canh chừng và render mức thay đổi", async () => {
    visionAnswer.value = "Có một terminal đang build.";
    watchActive.value = true;
    watchLastDiff.value = 0.126;
    watchEvents.value = [
      { time: "08:30:00", difference: 0.25 },
      { time: "08:31:00", difference: 0.5 },
    ];
    const wrapper = mount(VisionView);

    expect(wrapper.find(".vision-answer").text()).toContain("terminal đang build");
    expect(wrapper.find(".watch-status").text()).toContain("12.6%");
    expect(wrapper.findAll(".watch-events li")).toHaveLength(2);
    expect(wrapper.find(".watch-events").text()).toContain("25.0%");

    await wrapper.find(".watch-toggle").trigger("click");
    expect(stopScreenWatch).toHaveBeenCalledOnce();

    watchActive.value = false;
    await wrapper.vm.$nextTick();
    await wrapper.find(".watch-toggle").trigger("click");
    expect(startScreenWatch).toHaveBeenCalledOnce();
  });

  it("khóa hành động khi mất kết nối hoặc đang khởi động watch", async () => {
    isConnected.value = false;
    watchStarting.value = true;
    watchError.value = "Không chụp được màn hình";
    const wrapper = mount(VisionView);
    const buttons = wrapper.findAll("button");

    expect(buttons[0].attributes("disabled")).toBeDefined();
    expect(buttons[1].attributes("disabled")).toBeDefined();
    expect(wrapper.text()).toContain("Đang bật");
    expect(wrapper.text()).toContain("Không chụp được màn hình");
  });
});
