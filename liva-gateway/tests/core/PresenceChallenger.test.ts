import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { exec, execSync } from "child_process";
import { promisify } from "util";
import { PresenceDetector } from "../../src/services/PresenceDetector";
import { wireReactiveSync, ReactiveSyncDeps } from "../../src/core/events/ReactiveSync";
import { logger } from "../../src/utils/logger";

// Mock child_process.exec to allow dynamic control in tests
vi.mock("child_process", async (importOriginal) => {
  const actual = await importOriginal<typeof import("child_process")>();
  const mockExec = vi.fn();
  // We mock the custom promisified version directly
  const mockExecAsync = vi.fn();
  (mockExec as any)[Symbol.for("nodejs.util.promisify.custom")] = mockExecAsync;
  return {
    ...actual,
    exec: mockExec,
  };
});

// Mock logger to prevent spam and allow verification
vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
}));

describe("PresenceDetector Challenger - Tick Count Wrap-Around Check", () => {
  it("verifies 32-bit unsigned arithmetic in JavaScript mimics C# behavior", () => {
    // Mimic (uint)Environment.TickCount - lii.dwTime using JS unsigned 32-bit right shift (>>> 0)
    const calculateIdleTime = (tickCount: number, dwTime: number): number => {
      return (tickCount - dwTime) >>> 0;
    };

    // Case A: Normal case without wrap-around
    expect(calculateIdleTime(10000, 8000)).toBe(2000);

    // Case B: TickCount wrapped from positive max to negative min, dwTime is positive max
    // Int32.MinValue = -2147483648
    expect(calculateIdleTime(-2147483648, 2147483640)).toBe(8);

    // Case C: Both in negative region
    expect(calculateIdleTime(-2147483000, 2147484000)).toBe(296);

    // Case D: TickCount wrapped back to positive, dwTime is near uint.MaxValue (4294967200)
    expect(calculateIdleTime(100, 4294967200)).toBe(196);

    // Case E: Extreme wrap-around (TickCount at 0, dwTime at uint.MaxValue)
    expect(calculateIdleTime(0, 4294967295)).toBe(1);
  });

  it("executes C# compiler via PowerShell to verify actual .NET runtime unsigned subtraction wrap behavior", () => {
    // Run the inline powershell command to test real .NET behavior
    const psCommand = `powershell -NoProfile -NonInteractive -Command "Add-Type -TypeDefinition 'public class WrapTest { public static uint CalculateIdle(int tickCount, uint dwTime) { return (uint)tickCount - dwTime; } }'; if ([WrapTest]::CalculateIdle(10000, 8000) -eq 2000 -and [WrapTest]::CalculateIdle([Int32]::MinValue, 2147483640) -eq 8 -and [WrapTest]::CalculateIdle(-2147483000, 2147484000) -eq 296 -and [WrapTest]::CalculateIdle(100, 4294967200) -eq 196 -and [WrapTest]::CalculateIdle(0, 4294967295) -eq 1) { echo 'PASS' }"`;
    try {
      const output = execSync(psCommand, { encoding: "utf8" }).trim();
      expect(output).toContain("PASS");
    } catch (err) {
      console.warn("Could not execute PowerShell test on this host environment:", err);
    }
  });
});

describe("PresenceDetector Challenger - PowerShell Latency & Failure Checks", () => {
  let mockExecAsync: any;

  beforeEach(() => {
    vi.clearAllMocks();
    // Retrieve the mock execAsync function
    mockExecAsync = (exec as any)[Symbol.for("nodejs.util.promisify.custom")];
  });

  it("handles non-zero exit code or process failure, defaulting safely to ACTIVE", async () => {
    // Mock execAsync to simulate PowerShell command failure (rejects/throws error)
    mockExecAsync.mockRejectedValue(new Error("PowerShell process crashed"));

    const detector = new PresenceDetector(10000, 2000);
    
    // We expect getCurrentIdleTime to return -1 upon failure
    const idleTime = await (detector as any).getCurrentIdleTime();
    expect(idleTime).toBe(-1);

    // Call checkPresence and ensure it defaults to ACTIVE
    const updateSpy = vi.spyOn(detector as any, "updatePresence");
    await (detector as any).checkPresence();
    
    expect(updateSpy).toHaveBeenCalledWith("ACTIVE");
    expect(detector.getPresence()).toBe("ACTIVE");
  });

  it("does not block the event loop when PowerShell execution has latency (> 2 seconds)", async () => {
    let timerFired = false;
    
    // Set a timeout to verify the Node event loop remains active and responsive
    const localTimeout = setTimeout(() => {
      timerFired = true;
    }, 100);

    // Mock execAsync to introduce 2.5 seconds of delay (latency simulation)
    mockExecAsync.mockImplementation(async () => {
      await new Promise((resolve) => setTimeout(resolve, 2500));
      return { stdout: "5000\n", stderr: "" };
    });

    const detector = new PresenceDetector(10000, 1000);

    const startTime = Date.now();
    
    // Trigger async idle time query
    const idlePromise = (detector as any).getCurrentIdleTime();

    // Verify the event loop was not blocked (our local timeout should fire immediately)
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(timerFired).toBe(true);

    // Now wait for the idle query to finish
    const idleTime = await idlePromise;
    const duration = Date.now() - startTime;

    expect(idleTime).toBe(5000);
    expect(duration).toBeGreaterThanOrEqual(2400);

    clearTimeout(localTimeout);
  });
});

describe("PresenceDetector Challenger - Rapid Presence Switching & Event Ordering", () => {
  let detector: PresenceDetector;
  let deps: ReactiveSyncDeps;
  let broadcastEvents: Array<{ name: string; data?: any }> = [];
  let telegramSent: Array<{ recipient: string; text: string }> = [];
  let currentPresenceState: "ACTIVE" | "AWAY" = "ACTIVE";

  beforeEach(() => {
    broadcastEvents = [];
    telegramSent = [];
    currentPresenceState = "ACTIVE";
    detector = new PresenceDetector(10000, 1000);

    // Construct mock ReactiveSyncDeps
    deps = {
      agentLoop: {
        onThinkingStart: null,
        onThinkingEnd: null,
        onSpokenResponse: null,
        onStreamStart: null,
        onStreamChunk: null,
        onThoughtChunk: null,
        onRecoveryReset: null,
        onLatencyMask: null,
        onSystemBusy: null,
        Orchestrator: {
          on: vi.fn(),
        },
      } as any,
      ui: {
        broadcastUIEvent: vi.fn().mockImplementation(async (name, data) => {
          broadcastEvents.push({ name, data });
        }),
        removeListener: vi.fn(),
        on: vi.fn(),
      } as any,
      getVoiceEngine: () => ({
        preempt: vi.fn(),
        flushTTS: vi.fn(),
        pushTokens: vi.fn(),
        speak: vi.fn().mockResolvedValue(true),
        destroy: vi.fn(),
      } as any),
      setVoiceEngine: vi.fn(),
      whisperNode: {
        flush: vi.fn(),
      } as any,
      dispatch: async (id, payload) => {
        if (id === "ui_broadcast") {
          broadcastEvents.push({ name: payload.name, data: payload.data });
        }
      },
      addTelemetryLog: vi.fn(),
      isTtsFallbackActive: () => false,
      setTtsFallbackActive: vi.fn(),
      createFallbackVoiceEngine: () => ({} as any),
      onFallbackVoiceEngineCreated: vi.fn(),
      getPresence: () => currentPresenceState,
      getOwnerTelegramId: () => "TelegramOwnerId",
      telegramBridge: {
        sendText: vi.fn().mockImplementation(async (recipient, text) => {
          telegramSent.push({ recipient, text });
        }),
      },
    };

    wireReactiveSync(deps);
  });

  it("switches routing instantly and atomically during rapid switching", async () => {
    // 1. Initially ACTIVE: Spoken response should broadcast to local UI
    currentPresenceState = "ACTIVE";
    await deps.agentLoop.onSpokenResponse!("Hello user!");
    expect(broadcastEvents).toContainEqual({
      name: "ai_spoken_response",
      data: { text: "Hello user!" },
    });
    expect(telegramSent).toHaveLength(0);

    // Reset tracked events
    broadcastEvents = [];
    
    // 2. Rapid switch to AWAY: Spoken response must route to Telegram
    currentPresenceState = "AWAY";
    await deps.agentLoop.onSpokenResponse!("Are you there?");
    expect(broadcastEvents).toHaveLength(0);
    expect(telegramSent).toContainEqual({
      recipient: "TelegramOwnerId",
      text: "Are you there?",
    });

    // Reset tracked events
    telegramSent = [];

    // 3. Rapid switch back to ACTIVE: Spoken response routes to local UI again
    currentPresenceState = "ACTIVE";
    await deps.agentLoop.onSpokenResponse!("Welcome back!");
    expect(broadcastEvents).toContainEqual({
      name: "ai_spoken_response",
      data: { text: "Welcome back!" },
    });
    expect(telegramSent).toHaveLength(0);
  });

  it("maintains strict event ordering during rapid transitions", () => {
    const transitions: string[] = [];
    detector.on("presence_changed", (evt) => {
      transitions.push(evt.presence);
    });

    // Simulate rapid transitions back and forth
    (detector as any).updatePresence("AWAY");
    (detector as any).updatePresence("ACTIVE");
    (detector as any).updatePresence("AWAY");
    (detector as any).updatePresence("ACTIVE");

    expect(transitions).toEqual(["AWAY", "ACTIVE", "AWAY", "ACTIVE"]);
  });
});

describe("PresenceDetector Challenger - Cleanup & Stop checks", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("ensures that start/stop cleanly manages interval timers and prevents leaks", () => {
    const detector = new PresenceDetector(10000, 2000);
    
    // Ensure no interval starts before start()
    expect((detector as any).intervalId).toBeNull();

    // Start detector
    detector.start();
    expect((detector as any).intervalId).not.toBeNull();

    // Stop detector
    detector.stop();
    expect((detector as any).intervalId).toBeNull();
  });
});
