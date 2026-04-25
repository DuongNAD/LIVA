import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// DEEP MOCKING: Prevent any actual ML or DB initializations
// ============================================================

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

vi.mock("../../src/core/UIController", () => {
    return {
        UIController: class {
            on = vi.fn();
            emit = vi.fn();
            start = vi.fn();
        }
    };
});

vi.mock("../../src/SkillRegistry", () => {
    return {
        SkillRegistry: class {
            registerLocalSkills = vi.fn().mockResolvedValue(undefined);
        }
    };
});

vi.mock("../../src/MemoryManager", () => {
    return {
        MemoryManager: class {
            dispose = vi.fn();
            initialize = vi.fn().mockResolvedValue(undefined);
        }
    };
});

vi.mock("../../src/services/VoiceEngine", () => {
    return {
        VoiceEngine: class {
            destroy = vi.fn();
            on = vi.fn();
        }
    };
});

vi.mock("../../src/services/WhisperNode", () => {
    return {
        WhisperNode: class {
            flush = vi.fn();
            destroy = vi.fn();
        }
    };
});

vi.mock("../../src/memory/SensoryManager", () => ({
    SensoryManager: {
        getInstance: vi.fn().mockReturnValue({
            dispose: vi.fn(),
        }),
    },
}));

vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: vi.fn().mockReturnValue({
            dispose: vi.fn(),
        }),
    },
}));

vi.mock("../../src/services/KokoroVoiceEngine", () => {
    return {
        KokoroVoiceEngine: class {
            static getInstance() { return new this(); }
            destroy = vi.fn();
            on = vi.fn();
        }
    };
});

vi.mock("../../src/services/WhisperJSNode", () => {
    return {
        WhisperJSNode: class {
            static getInstance() { return new this(); }
            flush = vi.fn();
            destroy = vi.fn();
            on = vi.fn();
        }
    };
});

vi.mock("../../src/core/ZaloPolling", () => {
    return {
        ZaloPolling: class {
            static create = vi.fn().mockResolvedValue(new this());
            stop = vi.fn();
            start = vi.fn();
            on = vi.fn();
        }
    };
});

vi.mock("../../src/core/HeartbeatManager", () => {
    return {
        HeartbeatManager: class {
            static create = vi.fn().mockResolvedValue(new this());
            stop = vi.fn();
            start = vi.fn();
            on = vi.fn();
        }
    };
});

vi.mock("../../src/services/AppWatcherService", () => {
    return {
        AppWatcherService: class {
            static create = vi.fn().mockResolvedValue(new this());
            stop = vi.fn();
            start = vi.fn();
            on = vi.fn();
            setCallback = vi.fn();
        }
    };
});

vi.mock("../../src/skills/BrowserHarness", () => ({
    shutdownBrowserHarness: vi.fn().mockResolvedValue(undefined),
}));

import fs from "node:fs";
vi.mock("fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("fs")>();
    return {
        ...actual,
        default: {
            ...actual,
            watch: vi.fn().mockReturnValue({ close: vi.fn() }),
        },
        watch: vi.fn().mockReturnValue({ close: vi.fn() }),
    };
});

// Import Kernel AFTER mocks
import { CoreKernel } from "../../src/core/CoreKernel";
import { KokoroVoiceEngine } from "../../src/services/KokoroVoiceEngine";
import { EmbeddingService } from "../../src/services/EmbeddingService";

describe("CoreKernel — Shutdown & Resource Management", () => {
    let kernel: CoreKernel;
    
    beforeEach(() => {
        vi.clearAllMocks();
        // Spying on global clearInterval to avoid touching True Private fields
        vi.spyOn(global, "clearInterval");
        
        kernel = new CoreKernel();
    });
    
    afterEach(() => {
        vi.restoreAllMocks();
    });

    it("should successfully clean up all resources on happy path", async () => {
        // Trigger bootstrap to initialize interval and watcher (if any)
        await kernel.bootstrap();
        
        const mockEmbeddingService = EmbeddingService.getInstance();
        const mockVoiceEngine = (kernel as any).voiceEngine;
        
        // Execute shutdown
        kernel.shutdown();
        
        // BEHAVIORAL ASSERTION: Instead of checking private field, verify clearInterval was called
        // Since we don't know the exact ID, we just check it was called at least once (for GC timer)
        // Note: this assumes gcIntervalId was set during bootstrap
        expect(global.clearInterval).toHaveBeenCalled();
        
        // Assert other resources were destroyed
        expect(mockVoiceEngine.destroy).toHaveBeenCalled();
        expect(mockEmbeddingService.dispose).toHaveBeenCalled();
    });

    it("should handle partial shutdown failures gracefully (Negative Test)", async () => {
        await kernel.bootstrap();
        
        const mockEmbeddingService = EmbeddingService.getInstance();
        const mockVoiceEngine = (kernel as any).voiceEngine;
        
        // NEGATIVE TEST: Inject failure into VoiceEngine.destroy
        mockVoiceEngine.destroy.mockImplementation(() => {
            throw new Error("VoiceEngine crashed during destroy");
        });
        
        // Execute shutdown - it SHOULD NOT throw an exception thanks to safeExec
        expect(() => kernel.shutdown()).not.toThrow();
        
        // Verify VoiceEngine.destroy was indeed called and failed
        expect(mockVoiceEngine.destroy).toHaveBeenCalled();
        
        // THE CRITICAL ASSERTION:
        // Even though VoiceEngine threw an error, the shutdown sequence MUST CONTINUE
        // and successfully reach EmbeddingService.dispose()
        expect(mockEmbeddingService.dispose).toHaveBeenCalled();
    });
});
