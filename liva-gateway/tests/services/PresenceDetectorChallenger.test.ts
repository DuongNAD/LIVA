import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { PresenceDetector } from "../../src/services/PresenceDetector";
import { wireReactiveSync, ReactiveSyncDeps } from "../../src/core/events/ReactiveSync";
import { exec } from "child_process";

// Mock child_process for this file only
vi.mock("child_process", () => ({
  exec: vi.fn()
}));

describe("PresenceDetector Challenger Mocks and Routing Tests", () => {
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

  // =========================================================================
  // Challenger Check 2: Latency and Non-Zero Exit Code in PowerShell Query
  // =========================================================================
  it("should remain responsive and default safely to ACTIVE when PowerShell query fails with exit code non-zero", async () => {
    detector = new PresenceDetector(180000, 10000);
    const mockExec = exec as any;
    
    // Simulate non-zero exit code (error callback)
    mockExec.mockImplementation((cmd: any, options: any, cb?: any) => {
      const callback = typeof options === "function" ? options : cb;
      callback(new Error("Command failed: exit code 1"), { stdout: "" });
    });

    let eventPayload: any = null;
    detector.on("presence_changed", (evt) => {
      eventPayload = evt;
    });

    detector.start();
    
    // Trigger initial check
    await vi.runOnlyPendingTimersAsync();

    // Default presence must remain/fail-safe to ACTIVE on query failure
    expect(detector.getPresence()).toBe("ACTIVE");
    expect(eventPayload).toBeNull();
  });

  it("should remain responsive and default to ACTIVE during latency (> 2 seconds) in PowerShell execution", async () => {
    detector = new PresenceDetector(180000, 10000);
    const mockExec = exec as any;

    let resolveCallback: any = null;
    // Simulate a delayed response (latency)
    mockExec.mockImplementation((cmd: any, options: any, cb?: any) => {
      const callback = typeof options === "function" ? options : cb;
      resolveCallback = () => callback(null, { stdout: "3000" });
    });

    detector.start();
    
    // Trigger the check (moves timer to execute checkPresence)
    await vi.advanceTimersByTimeAsync(1);

    // During the latency period (before resolving), the detector remains responsive
    // and its state is still the default/last known (ACTIVE)
    expect(detector.getPresence()).toBe("ACTIVE");

    // After 3 seconds, we resolve the PowerShell query
    if (resolveCallback) resolveCallback();
    await vi.advanceTimersByTimeAsync(3000);

    expect(detector.getPresence()).toBe("ACTIVE");
  });

  // =========================================================================
  // Challenger Check 3: Rapid Presence State Switching & Reactive Sync Routing
  // =========================================================================
  it("should transition states and emit events in the correct order under rapid presence switching", async () => {
    detector = new PresenceDetector(180000, 10000);
    
    const events: string[] = [];
    detector.on("presence_changed", (evt) => {
      events.push(evt.presence);
    });

    // Directly trigger internal updates rapidly
    const updatePresenceMethod = (detector as any).updatePresence.bind(detector);

    // Rapid switch: ACTIVE -> AWAY -> ACTIVE within milliseconds
    updatePresenceMethod("AWAY");
    updatePresenceMethod("ACTIVE");

    expect(events).toEqual(["AWAY", "ACTIVE"]);
    expect(detector.getPresence()).toBe("ACTIVE");
  });

  it("should update routing automatically when presence transitions between ACTIVE and AWAY", async () => {
    // Set up dependencies for wireReactiveSync
    let currentPresence: "ACTIVE" | "AWAY" = "ACTIVE";
    const telegramSent: string[] = [];
    const uiBroadcasts: any[] = [];

    const mockDeps: ReactiveSyncDeps = {
      agentLoop: {
        onThinkingStart: null,
        onThinkingEnd: null,
        onSpokenResponse: null,
        onSystemBusy: null,
        onStreamStart: null,
        onStreamChunk: null,
        onThoughtChunk: null,
        onRecoveryReset: null,
        onLatencyMask: null,
        Orchestrator: {
          on: vi.fn()
        }
      } as any,
      ui: {
        broadcastUIEvent: vi.fn().mockImplementation(async (name, data) => {
          uiBroadcasts.push({ name, data });
        }),
        removeListener: vi.fn(),
        on: vi.fn()
      } as any,
      getVoiceEngine: () => ({
        preempt: vi.fn(),
        flushTTS: vi.fn(),
        pushTokens: vi.fn()
      } as any),
      setVoiceEngine: vi.fn(),
      whisperNode: {
        flush: vi.fn()
      } as any,
      dispatch: async (id, payload) => {
        if (id === "ui_broadcast") {
          uiBroadcasts.push(payload);
        }
      },
      addTelemetryLog: vi.fn(),
      isTtsFallbackActive: () => false,
      setTtsFallbackActive: vi.fn(),
      createFallbackVoiceEngine: vi.fn(),
      onFallbackVoiceEngineCreated: vi.fn(),
      getPresence: () => currentPresence,
      getOwnerTelegramId: () => "owner-tele-id",
      telegramBridge: {
        sendText: async (id: any, text: any) => {
          telegramSent.push(text);
        }
      }
    };

    // Wire the reactive sync
    wireReactiveSync(mockDeps);

    // 1. When presence is ACTIVE, spoken response should broadcast to UI and not go to Telegram
    currentPresence = "ACTIVE";
    await mockDeps.agentLoop.onSpokenResponse!("Hello Local User");
    expect(uiBroadcasts).toContainEqual({ name: "ai_spoken_response", data: { text: "Hello Local User" } });
    expect(telegramSent).toHaveLength(0);

    // Clear broadcasts
    uiBroadcasts.length = 0;

    // 2. Transition to AWAY
    currentPresence = "AWAY";
    await mockDeps.agentLoop.onSpokenResponse!("Hello Remote User");
    // Should NOT broadcast to UI
    expect(uiBroadcasts).toHaveLength(0);
    // Should go to Telegram
    expect(telegramSent).toContain("Hello Remote User");
  });

  // =========================================================================
  // Challenger Check 4: Stop/Shutdown Cleanup
  // =========================================================================
  it("should cleanly clean up interval and stop scheduled checks on stop()", () => {
    detector = new PresenceDetector(180000, 10000);
    
    detector.start();
    expect((detector as any).intervalId).not.toBeNull();

    detector.stop();
    expect((detector as any).intervalId).toBeNull();
  });
});
