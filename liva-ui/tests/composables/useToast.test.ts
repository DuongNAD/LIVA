import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { useToast } from "../../src/composables/useToast";

describe("useToast composable", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    const { clear } = useToast();
    clear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should start with an empty toast list", () => {
    const { toasts } = useToast();
    expect(toasts.value.length).toBe(0);
  });

  it("should show success, error, warning, info toasts", () => {
    const toast = useToast();

    const id1 = toast.success("Saved successfully");
    const id2 = toast.error("Failed to connect");
    const id3 = toast.warning("Low disk space");
    const id4 = toast.info("New update available");

    expect(toast.toasts.value.length).toBe(4);
    expect(toast.toasts.value[0].type).toBe("success");
    expect(toast.toasts.value[0].message).toBe("Saved successfully");
    expect(toast.toasts.value[1].type).toBe("error");
    expect(toast.toasts.value[2].type).toBe("warning");
    expect(toast.toasts.value[3].type).toBe("info");
    expect(id1).toBeDefined();
    expect(id2).toBeDefined();
    expect(id3).toBeDefined();
    expect(id4).toBeDefined();
  });

  it("should auto-dismiss toast after duration", () => {
    const toast = useToast();

    toast.show({
      message: "Temporary toast",
      type: "info",
      duration: 3000,
    });

    expect(toast.toasts.value.length).toBe(1);

    vi.advanceTimersByTime(2999);
    expect(toast.toasts.value.length).toBe(1);

    vi.advanceTimersByTime(2);
    expect(toast.toasts.value.length).toBe(0);
  });

  it("should manually dismiss a toast by id", () => {
    const toast = useToast();

    const id = toast.info("Dismiss me");
    expect(toast.toasts.value.length).toBe(1);

    toast.dismiss(id);
    expect(toast.toasts.value.length).toBe(0);
  });

  it("should clear all toasts", () => {
    const toast = useToast();

    toast.info("1");
    toast.success("2");
    toast.error("3");
    expect(toast.toasts.value.length).toBe(3);

    toast.clear();
    expect(toast.toasts.value.length).toBe(0);
  });

  it("should support custom action callbacks", () => {
    const toast = useToast();
    const actionSpy = vi.fn();

    const id = toast.show({
      message: "Action toast",
      action: {
        label: "Retry",
        onClick: actionSpy,
      },
    });

    const item = toast.toasts.value.find((t) => t.id === id);
    expect(item).toBeDefined();
    expect(item?.action?.label).toBe("Retry");

    item?.action?.onClick();
    expect(actionSpy).toHaveBeenCalledTimes(1);
  });

  it("should cap active toasts at 6 to prevent overflow", () => {
    const toast = useToast();

    for (let i = 1; i <= 8; i++) {
      toast.info(`Message ${i}`, { duration: 0 });
    }

    expect(toast.toasts.value.length).toBe(6);
    expect(toast.toasts.value[toast.toasts.value.length - 1].message).toBe("Message 8");
  });
});
