import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mount } from "@vue/test-utils";
import ToastContainer from "../../src/components/ToastContainer.vue";
import { useToast } from "../../src/composables/useToast";

describe("ToastContainer.vue", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    const { clear } = useToast();
    clear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should render empty container initially", () => {
    const wrapper = mount(ToastContainer);
    expect(wrapper.findAll(".toast-item").length).toBe(0);
  });

  it("should render toasts when added to queue", async () => {
    const wrapper = mount(ToastContainer);
    const toast = useToast();

    toast.success("Success notification", { title: "Done" });
    toast.error("Error notification");
    await wrapper.vm.$nextTick();

    const items = wrapper.findAll(".toast-item");
    expect(items.length).toBe(2);
    expect(wrapper.find(".toast-success").exists()).toBe(true);
    expect(wrapper.find(".toast-error").exists()).toBe(true);
    expect(wrapper.find(".toast-title").text()).toBe("Done");
    expect(wrapper.text()).toContain("Success notification");
    expect(wrapper.text()).toContain("Error notification");
  });

  it("should dismiss toast when clicking close button", async () => {
    const wrapper = mount(ToastContainer);
    const toast = useToast();

    toast.info("Click close button", { dismissible: true });
    await wrapper.vm.$nextTick();

    const closeBtn = wrapper.find(".toast-close-btn");
    expect(closeBtn.exists()).toBe(true);

    await closeBtn.trigger("click");
    await wrapper.vm.$nextTick();

    expect(wrapper.findAll(".toast-item").length).toBe(0);
  });

  it("should execute action callback when action button clicked", async () => {
    const wrapper = mount(ToastContainer);
    const toast = useToast();
    const actionMock = vi.fn();

    toast.warning("Needs reload", {
      action: { label: "Reload Now", onClick: actionMock },
    });
    await wrapper.vm.$nextTick();

    const actionBtn = wrapper.find(".toast-action-btn");
    expect(actionBtn.exists()).toBe(true);
    expect(actionBtn.text()).toBe("Reload Now");

    await actionBtn.trigger("click");
    expect(actionMock).toHaveBeenCalled();
  });
});
