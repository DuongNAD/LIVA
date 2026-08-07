import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import ToolPanel from "../../src/components/ToolPanel.vue";

describe("ToolPanel", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("declares its left-bottom overlay placement away from the default avatar", () => {
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "get_weather",
        state: "loading",
        payload: null,
      },
    });

    expect(wrapper.get(".tool-panel").attributes("data-placement")).toBe("left-bottom");
  });

  it("renders a dedicated weather result with location, icon, temperature and description", () => {
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "get_weather",
        state: "done",
        payload: {
          location: "Hà Nội",
          icon: "⛅",
          temperature: 31,
          description: "Có mây",
        },
      },
    });

    const weather = wrapper.get(".tool-panel__weather");
    expect(weather.text()).toContain("Hà Nội");
    expect(weather.text()).toContain("⛅");
    expect(weather.text()).toContain("31°C");
    expect(weather.text()).toContain("Có mây");
  });

  it("renders weather returned through the real MCP content envelope", () => {
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "get_weather",
        state: "done",
        payload: {
          content: [{ type: "text", text: "Hà Nội: 31°C, có mây, độ ẩm 70%." }],
          isError: false,
        },
      },
    });

    const weather = wrapper.get(".tool-panel__weather");
    expect(weather.text()).toContain("Hà Nội");
    expect(weather.text()).toContain("31°C");
    expect(weather.text()).toContain("có mây");
  });

  it("shows an animated waiting state while the tool is loading", () => {
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "get_weather",
        state: "loading",
        payload: null,
      },
    });

    expect(wrapper.get(".tool-panel__loading").text()).toContain("Đang xử lý");
    expect(wrapper.find(".tool-panel__spinner").exists()).toBe(true);
  });

  it("uses the human-readable label emitted with tool_start", () => {
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "get_weather",
        state: "loading",
        payload: { label: "Đang xem thời tiết…" },
      },
    });

    expect(wrapper.get(".tool-panel__loading").text()).toContain("Đang xem thời tiết…");
  });

  it("shows a clear error message instead of leaving the panel loading", () => {
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "get_weather",
        state: "error",
        payload: { message: "Không lấy được thời tiết vì mất kết nối." },
      },
    });

    const error = wrapper.get(".tool-panel__error");
    expect(error.attributes("role")).toBe("alert");
    expect(error.text()).toContain("Không lấy được thời tiết vì mất kết nối.");
    expect(wrapper.find(".tool-panel__loading").exists()).toBe(false);
  });

  it("renders an unknown tool with a readable generic payload", () => {
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "read_markdown",
        state: "done",
        payload: { path: "notes/today.md", lines: 12 },
      },
    });

    const fallback = wrapper.get(".tool-panel__default");
    expect(fallback.text()).toContain("read_markdown");
    expect(fallback.text()).toContain("notes/today.md");
    expect(fallback.text()).toContain("12");
  });

  it("emits close when the user presses the close button", async () => {
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "get_weather",
        state: "loading",
        payload: null,
      },
    });

    await wrapper.get('button[aria-label="Đóng bảng công cụ"]').trigger("click");
    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("auto-closes ten seconds after entering the done state", async () => {
    vi.useFakeTimers();
    const wrapper = mount(ToolPanel, {
      props: {
        tool: "get_weather",
        state: "done",
        payload: { temperature: 31 },
      },
    });

    await vi.advanceTimersByTimeAsync(9_999);
    expect(wrapper.emitted("close")).toBeUndefined();
    await vi.advanceTimersByTimeAsync(1);
    expect(wrapper.emitted("close")).toHaveLength(1);
  });
});
