import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";
import BiAnalyticsView from "../../src/components/dashboard/BiAnalyticsView.vue";

// Mock useGateway
const isConnectedRef = ref(true);
vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => ({
    isConnected: isConnectedRef,
    init: vi.fn(),
    sendMsg: vi.fn(),
  }),
}));

// Mock useI18n
vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (key: string) => (key === "nav_bi" ? "BI Analytics" : key),
    currentLang: ref("en-US"),
  }),
}));

// Mock useToast
const mockToast = {
  show: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
  warn: vi.fn(),
  info: vi.fn(),
  dismiss: vi.fn(),
  clear: vi.fn(),
  toasts: ref([]),
};
vi.mock("../../src/composables/useToast", () => ({
  useToast: () => mockToast,
}));

describe("BiAnalyticsView.vue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should render KPI cards, charts, and SQL console", () => {
    const wrapper = mount(BiAnalyticsView);

    expect(wrapper.text()).toContain("BI Analytics");
    expect(wrapper.text()).toContain("Total Queries");
    expect(wrapper.text()).toContain("Avg Latency");
    expect(wrapper.text()).toContain("Token Throughput");
    expect(wrapper.text()).toContain("Swarm Tasks Executed");
    expect(wrapper.text()).toContain("Agent Swarm Workload");
    expect(wrapper.text()).toContain("SQL Query Console & Schema Explorer");
  });

  it("should switch time range filter and update KPI values", async () => {
    const wrapper = mount(BiAnalyticsView);

    const timeButtons = wrapper.findAll(".time-btn");
    expect(timeButtons.length).toBe(4);

    // Switch to 1h
    await timeButtons[0].trigger("click");
    expect(wrapper.find(".kpi-value").text()).toContain("142");

    // Switch to 7d
    await timeButtons[2].trigger("click");
    expect(wrapper.find(".kpi-value").text()).toContain("12,450");
  });

  it("should apply query template when clicking template chip", async () => {
    const wrapper = mount(BiAnalyticsView);

    const chips = wrapper.findAll(".template-chip");
    expect(chips.length).toBeGreaterThan(0);

    await chips[1].trigger("click");
    const textarea = wrapper.find<HTMLTextAreaElement>(".sql-textarea");
    expect(textarea.element.value).toContain("SELECT tool_name");
  });

  it("should execute query and show toast notification", async () => {
    const wrapper = mount(BiAnalyticsView);

    const runBtn = wrapper.findAll("button").find(b => b.text().includes("Run Query"));
    expect(runBtn).toBeDefined();

    await runBtn!.trigger("click");
    vi.advanceTimersByTime(500);
    await flushPromises();

    expect(mockToast.success).toHaveBeenCalled();
  });

  it("should handle refresh data action", async () => {
    const wrapper = mount(BiAnalyticsView);

    const refreshBtn = wrapper.findAll("button").find(b => b.text().includes("Refresh"));
    expect(refreshBtn).toBeDefined();

    await refreshBtn!.trigger("click");
    vi.advanceTimersByTime(500);
    await flushPromises();

    expect(mockToast.info).toHaveBeenCalledWith("BI Telemetry & metrics updated.");
  });

  it("should export CSV and trigger toast", async () => {
    const wrapper = mount(BiAnalyticsView);

    const exportBtn = wrapper.findAll("button").find(b => b.text().includes("Export CSV"));
    expect(exportBtn).toBeDefined();

    // Mock URL.createObjectURL and link.click
    global.URL.createObjectURL = vi.fn().mockReturnValue("blob:test");
    global.URL.revokeObjectURL = vi.fn();

    await exportBtn!.trigger("click");
    expect(mockToast.success).toHaveBeenCalledWith("CSV exported successfully.");
  });
});
