import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

// Pre-mock setup to prevent actual imports
process.env.AI_PROVIDER = "local";

vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
    child: vi.fn().mockReturnThis()
  },
}));

// We want to mock everything but spy on call order
const { callSequence, createMockSpy } = vi.hoisted(() => {
  const callSequence: string[] = [];
  const createMockSpy = (name: string, methodName: string, resolvedValue?: any) => {
    return vi.fn().mockImplementation(() => {
      callSequence.push(`${name}.${methodName}`);
      return Promise.resolve(resolvedValue);
    });
  };
  return { callSequence, createMockSpy };
});

vi.mock("../../src/core/UIController", () => {
  return {
    UIController: class extends EventEmitter {
      start = vi.fn();
      broadcastUIEvent = vi.fn();
      broadcastTTSAudio = vi.fn();
    }
  };
});

vi.mock("../../src/SkillRegistry", () => {
  return {
    SkillRegistry: class {
      registerLocalSkills = createMockSpy("registry", "registerLocalSkills");
      reloadLocalSkill = createMockSpy("registry", "reloadLocalSkill");
      getAllSkills = vi.fn().mockReturnValue([]);
      whitelist = {
        load: createMockSpy("registry.whitelist", "load"),
        getAll: vi.fn().mockReturnValue({}),
        dispose: createMockSpy("registry.whitelist", "dispose")
      };
      circuitBreaker = { getOpenCircuits: vi.fn().mockReturnValue(new Set()) };
      warmUpCache = createMockSpy("registry", "warmUpCache");
    }
  };
});

vi.mock("../../src/MemoryManager", () => {
  return {
    MemoryManager: class {
      dispose = createMockSpy("memory", "dispose");
      initialize = createMockSpy("memory", "initialize");
      initUHM = createMockSpy("memory", "initUHM");
      getShortTermHistory = vi.fn().mockResolvedValue([]);
      getSessionState = vi.fn().mockResolvedValue("");
      markLastTurnReflected = vi.fn();
    }
  };
});

vi.mock("../../src/services/EmbeddingService", () => ({
  EmbeddingService: {
    getInstance: vi.fn().mockReturnValue({
      dispose: createMockSpy("EmbeddingService", "dispose"),
      setVramGuardCheck: vi.fn(),
    }),
  },
}));

vi.mock("../../src/memory/SensoryManager", () => ({
  SensoryManager: {
    getInstance: vi.fn().mockReturnValue({
      dispose: createMockSpy("SensoryManager", "dispose"),
    }),
  },
}));

vi.mock("../../src/memory/HeraCompass", () => ({
  HeraCompass: {
    getInstance: vi.fn().mockReturnValue({
      dispose: createMockSpy("HeraCompass", "dispose"),
    }),
  },
}));

vi.mock("../../src/memory/TokenCompressionService", () => ({
  TokenCompressionService: {
    getInstance: vi.fn().mockReturnValue({
      dispose: createMockSpy("TokenCompressionService", "dispose"),
    }),
  },
}));

vi.mock("../../src/core/ZaloPolling", () => {
  return {
    ZaloPolling: class extends EventEmitter {
      stop = createMockSpy("zalo", "stop");
      start = createMockSpy("zalo", "start");
    }
  };
});

vi.mock("../../src/core/HeartbeatManager", () => {
  return {
    HeartbeatManager: class extends EventEmitter {
      stop = createMockSpy("heartbeat", "stop");
      start = createMockSpy("heartbeat", "start");
    }
  };
});

vi.mock("../../src/services/AppWatcherService", () => {
  return {
    AppWatcherService: class extends EventEmitter {
      stop = createMockSpy("appWatcher", "stop");
      start = createMockSpy("appWatcher", "start");
      setCallback = vi.fn();
    }
  };
});

vi.mock("../../src/services/PresenceDetector", () => {
  return {
    PresenceDetector: class extends EventEmitter {
      stop = createMockSpy("presenceDetector", "stop");
    }
  };
});

vi.mock("../../src/services/PowerMonitorService", () => {
  return {
    PowerMonitorService: class extends EventEmitter {
      start = createMockSpy("powerMonitor", "start");
      stop = createMockSpy("powerMonitor", "stop");
    }
  };
});

vi.mock("../../src/core/orchestrators/VoiceOrchestrator", () => {
  return {
    VoiceOrchestrator: class extends EventEmitter {
      initialize = vi.fn().mockResolvedValue(undefined);
      dispose = createMockSpy("voiceOrchestrator", "dispose");
      whisperNode = new EventEmitter();
      voiceEngine = new EventEmitter();
      vadBridge = null;
    }
  };
});

vi.mock("../../src/services/EmailClientManager", () => {
  return {
    EmailClientManager: class extends EventEmitter {
      startIdling = createMockSpy("emailManager", "startIdling");
      dispose = createMockSpy("emailManager", "dispose");
    }
  };
});

vi.mock("../../src/evolution/GitNexusIndexer", () => {
  return {
    GitNexusIndexer: class {
      triggerIndex = createMockSpy("gitNexusIndexer", "triggerIndex");
      dispose = createMockSpy("gitNexusIndexer", "dispose");
    }
  };
});

vi.mock("../../src/channels/TelegramBridge", () => {
  return {
    TelegramBridge: class extends EventEmitter {
      startPolling = createMockSpy("telegram", "startPolling");
      stop = createMockSpy("telegram", "stop");
      setBridges = vi.fn();
    }
  };
});

vi.mock("../../src/channels/MetaBridge", () => {
  return {
    MetaBridge: class extends EventEmitter {
      startWebhookServer = createMockSpy("meta", "startWebhookServer");
      stop = createMockSpy("meta", "stop");
    }
  };
});

vi.mock("../../src/bridges/CDPBridge", () => {
  return {
    CDPBridge: class extends EventEmitter {
      connect = createMockSpy("cdpBridge", "connect");
      dispose = createMockSpy("cdpBridge", "dispose");
      watchForApprovalButtons = vi.fn().mockResolvedValue(undefined);
    }
  };
});

vi.mock("../../src/bridges/VSCodeBridge", () => {
  return {
    VSCodeBridge: class extends EventEmitter {
      connect = createMockSpy("vscodeBridge", "connect");
      dispose = createMockSpy("vscodeBridge", "dispose");
    }
  };
});

vi.mock("../../src/core/SessionOrchestrator", () => {
  return {
    SessionOrchestrator: class extends EventEmitter {
      dispose = createMockSpy("sessions", "dispose");
    }
  };
});

vi.mock("../../src/core/ApprovalEngine", () => {
  return {
    ApprovalEngine: class extends EventEmitter {
      dispose = createMockSpy("approvalEngine", "dispose");
    }
  };
});

vi.mock("chokidar", () => ({
  watch: vi.fn().mockReturnValue({
    close: createMockSpy("fileWatcher", "close"),
    on: vi.fn().mockReturnThis()
  })
}));

import { CoreKernel } from "../../src/core/CoreKernel";

describe("CoreKernel Lifecycle Execution Order", () => {
  let kernel: CoreKernel;

  beforeEach(() => {
    callSequence.length = 0;
    vi.clearAllMocks();
    kernel = new CoreKernel();
  });

  afterEach(async () => {
    if (kernel) {
      await kernel.shutdown();
    }
  });

  it("should execute bootstrap lifecycle hooks in correct order", async () => {
    // Enable remote control to verify all bridges boot
    vi.spyOn(kernel.securityGateway, "isRemoteControlEnabled").mockReturnValue(true);

    // Spy on agentLoop methods
    kernel.agentLoop.initModels = createMockSpy("agentLoop", "initModels");
    kernel.agentLoop.Orchestrator.startAnomalyDetection = createMockSpy("agentLoop.Orchestrator", "startAnomalyDetection");

    await kernel.bootstrap();

    // The first 3 should be run in parallel during bootstrap
    const initialParallelCalls = callSequence.slice(0, 3);
    expect(initialParallelCalls).toContain("memory.initialize");
    expect(initialParallelCalls).toContain("registry.registerLocalSkills");
    expect(initialParallelCalls).toContain("registry.whitelist.load");

    // After that, llama models are initialized
    expect(callSequence[3]).toBe("agentLoop.initModels");

    // Anomaly detection is started
    expect(callSequence).toContain("agentLoop.Orchestrator.startAnomalyDetection");

    // App watcher, indexer, emailManager start
    expect(callSequence).toContain("appWatcher.start");
    expect(callSequence).toContain("gitNexusIndexer.triggerIndex");
    expect(callSequence).toContain("emailManager.startIdling");

    // Heartbeat and power monitor start
    expect(callSequence).toContain("heartbeat.start");
    expect(callSequence).toContain("powerMonitor.start");

    // Remote Control systems start
    expect(callSequence).toContain("telegram.startPolling");
    expect(callSequence).toContain("meta.startWebhookServer");
    expect(callSequence).toContain("cdpBridge.connect");
    expect(callSequence).toContain("vscodeBridge.connect");
  });

  it("should execute shutdown lifecycle hooks in correct order", async () => {
    // Spy on agentLoop methods
    kernel.agentLoop.shutdown = createMockSpy("agentLoop", "shutdown");
    kernel.agentLoop.Orchestrator.killLlamaServer = createMockSpy("agentLoop.Orchestrator", "killLlamaServer");

    // Call bootstrap to setup watchers
    await kernel.bootstrap();

    // Reset sequence so we only track shutdown
    callSequence.length = 0;

    await kernel.shutdown();

    // Step 1: Immediate llama server shutdown to release VRAM
    expect(callSequence[0]).toBe("agentLoop.Orchestrator.killLlamaServer");

    // Step 2: Stop detectors and watchers
    expect(callSequence[1]).toBe("presenceDetector.stop");
    expect(callSequence[2]).toBe("fileWatcher.close");

    // Other components shutdown
    expect(callSequence).toContain("zalo.stop");
    expect(callSequence).toContain("heartbeat.stop");
    expect(callSequence).toContain("appWatcher.stop");
    expect(callSequence).toContain("powerMonitor.stop");
    expect(callSequence).toContain("voiceOrchestrator.dispose");
    expect(callSequence).toContain("memory.dispose");
    expect(callSequence).toContain("SensoryManager.dispose");
    expect(callSequence).toContain("EmbeddingService.dispose");
    expect(callSequence).toContain("emailManager.dispose");
    expect(callSequence).toContain("gitNexusIndexer.dispose");
    expect(callSequence).toContain("telegram.stop");
    expect(callSequence).toContain("meta.stop");
    expect(callSequence).toContain("cdpBridge.dispose");
    expect(callSequence).toContain("approvalEngine.dispose");
    expect(callSequence).toContain("vscodeBridge.dispose");
    expect(callSequence).toContain("sessions.dispose");
    expect(callSequence).toContain("registry.whitelist.dispose");

    // agentLoop.shutdown should be one of the last steps
    expect(callSequence[callSequence.length - 1]).toBe("agentLoop.shutdown");
  });
});
