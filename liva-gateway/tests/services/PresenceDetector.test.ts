import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { PresenceDetector } from "../../src/services/PresenceDetector";
import { exec } from "child_process";

// Mock the child_process.exec call
vi.mock("child_process", () => ({
  exec: vi.fn()
}));

describe("PresenceDetector Unit Tests", () => {
  let detector: PresenceDetector;

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    if (detector) {
      detector.stop();
    }
  });

  it("should initialize with default active state and thresholds", () => {
    detector = new PresenceDetector(180000, 10000);
    expect(detector.getPresence()).toBe("ACTIVE");
  });

  it("should transition to AWAY when idle threshold is exceeded", async () => {
    detector = new PresenceDetector(180000, 10000);
    let eventPayload: any = null;
    detector.on("presence_changed", (evt) => {
      eventPayload = evt;
    });

    const mockExec = exec as unknown as vi.Mock;
    
    // Simulate user has been idle for 200 seconds (200,000 ms)
    mockExec.mockImplementation((cmd, options, cb) => {
      const callback = typeof options === "function" ? options : cb;
      callback(null, { stdout: "200000" });
    });

    detector.start();
    
    // Trigger initial check
    await vi.runOnlyPendingTimersAsync();

    expect(detector.getPresence()).toBe("AWAY");
    expect(eventPayload).toEqual({ presence: "AWAY" });
  });

  it("should transition to ACTIVE when user activity is detected after AWAY", async () => {
    detector = new PresenceDetector(180000, 10000);
    let eventPayloads: any[] = [];
    detector.on("presence_changed", (evt) => {
      eventPayloads.push(evt);
    });

    const mockExec = exec as unknown as vi.Mock;
    
    // 1. First poll returns 200,000 ms (AWAY)
    mockExec.mockImplementationOnce((cmd, options, cb) => {
      const callback = typeof options === "function" ? options : cb;
      callback(null, { stdout: "200000" });
    });

    detector.start();
    await vi.runOnlyPendingTimersAsync();

    expect(detector.getPresence()).toBe("AWAY");

    // 2. Second poll returns 1,000 ms (ACTIVE)
    mockExec.mockImplementationOnce((cmd, options, cb) => {
      const callback = typeof options === "function" ? options : cb;
      callback(null, { stdout: "1000" });
    });

    await vi.runOnlyPendingTimersAsync();

    expect(detector.getPresence()).toBe("ACTIVE");
    expect(eventPayloads).toHaveLength(2);
    expect(eventPayloads[0]).toEqual({ presence: "AWAY" });
    expect(eventPayloads[1]).toEqual({ presence: "ACTIVE" });
  });

  it("should default safely to ACTIVE when PowerShell query fails", async () => {
    detector = new PresenceDetector(180000, 10000);
    let eventPayload: any = null;
    detector.on("presence_changed", (evt) => {
      eventPayload = evt;
    });

    const mockExec = exec as unknown as vi.Mock;
    
    // Mock execution error
    mockExec.mockImplementation((cmd, options, cb) => {
      const callback = typeof options === "function" ? options : cb;
      callback(new Error("PowerShell error"), { stdout: "" });
    });

    detector.start();
    await vi.runOnlyPendingTimersAsync();

    // Must remain ACTIVE (fail-safe)
    expect(detector.getPresence()).toBe("ACTIVE");
    expect(eventPayload).toBeNull();
  });
});
