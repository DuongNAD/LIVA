import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ModelOrchestrator, type TaskToken } from "../../src/core/ModelOrchestrator";

// ============================================================
// Mocks
// ============================================================
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

vi.mock("tree-kill", () => ({
    default: vi.fn((pid, signal, cb) => cb && cb()),
}));




// ============================================================
// TEST GROUP 1: TaskToken Factory
// ============================================================
describe("ModelOrchestrator — Token Factory", () => {
    it("should expose getAuthorizedTokenFactory static method", () => {
        const factory = ModelOrchestrator.getAuthorizedTokenFactory();
        expect(factory).toBeDefined();
        expect(typeof factory.issueToken).toBe("function");
    });

    it("should mint a ROUTER_START_AUTH token", () => {
        const factory = ModelOrchestrator.getAuthorizedTokenFactory();
        const token = factory.issueToken("ROUTER_START_AUTH");
        expect(token).toBe("ROUTER_START_AUTH");
    });

    it("should mint an EXPERT_START_AUTH token", () => {
        const factory = ModelOrchestrator.getAuthorizedTokenFactory();
        const token = factory.issueToken("EXPERT_START_AUTH");
        expect(token).toBe("EXPERT_START_AUTH");
    });
});

// ============================================================
// TEST GROUP 2: ModelOrchestrator State Management
// ============================================================
describe("ModelOrchestrator — State Management", () => {
    let orchestrator: ModelOrchestrator;

    beforeEach(() => {
        // Need to clean up process listeners to avoid leaks
        orchestrator = new ModelOrchestrator();
    });

    afterEach(() => {
        orchestrator.stopRouter();
    });

    it("should initialize with both models inactive", () => {
        const status = orchestrator.getStatus();
        expect(status.routerActive).toBe(false);
        expect(status.expertActive).toBe(false);
    });

    describe("startRouter", () => {
        it("should reject invalid auth token", async () => {
            await expect(
                orchestrator.startRouter("INVALID_TOKEN" as TaskToken<"ROUTER_START_AUTH">)
            ).rejects.toThrow("Unauthorized");
        });

        it("should activate router in native mode (LIVA_USE_NATIVE=true)", async () => {
            const originalEnv = process.env.LIVA_USE_NATIVE;
            process.env.LIVA_USE_NATIVE = "true";

            const factory = ModelOrchestrator.getAuthorizedTokenFactory();
            const token = factory.issueToken("ROUTER_START_AUTH");

            await orchestrator.startRouter(token);

            const status = orchestrator.getStatus();
            expect(status.routerActive).toBe(true);

            process.env.LIVA_USE_NATIVE = originalEnv;
        });

        it("should be idempotent (second call ignored if already started)", async () => {
            const originalEnv = process.env.LIVA_USE_NATIVE;
            process.env.LIVA_USE_NATIVE = "true";

            const factory = ModelOrchestrator.getAuthorizedTokenFactory();
            const token = factory.issueToken("ROUTER_START_AUTH");

            await orchestrator.startRouter(token);
            // Second call should resolve immediately (no error)
            await expect(orchestrator.startRouter(token)).resolves.not.toThrow();

            process.env.LIVA_USE_NATIVE = originalEnv;
        });
    });

    describe("startExpert", () => {
        it("should reject invalid auth token", async () => {
            await expect(
                orchestrator.startExpert("WRONG_TOKEN" as TaskToken<"EXPERT_START_AUTH">)
            ).rejects.toThrow("Unauthorized");
        });

        it("should handle cloud mode (skip local spawn)", async () => {
            const originalProvider = process.env.AI_PROVIDER;
            process.env.AI_PROVIDER = "cloud";

            const factory = ModelOrchestrator.getAuthorizedTokenFactory();
            const token = factory.issueToken("EXPERT_START_AUTH");

            await orchestrator.startExpert(token);

            const status = orchestrator.getStatus();
            expect(status.expertActive).toBe(true);

            process.env.AI_PROVIDER = originalProvider;
        });
    });

    describe("stopExpert", () => {
        it("should handle cloud mode stop", async () => {
            const originalProvider = process.env.AI_PROVIDER;
            process.env.AI_PROVIDER = "cloud";

            await expect(orchestrator.stopExpert()).resolves.not.toThrow();

            process.env.AI_PROVIDER = originalProvider;
        });

        it("should resolve immediately if no expert process", async () => {
            await expect(orchestrator.stopExpert()).resolves.not.toThrow();
        });
    });

    describe("stopRouter", () => {
        it("should not throw when no router process exists", () => {
            expect(() => orchestrator.stopRouter()).not.toThrow();
        });
    });

    describe("getStatus", () => {
        it("should return both flags as boolean", () => {
            const status = orchestrator.getStatus();
            expect(typeof status.routerActive).toBe("boolean");
            expect(typeof status.expertActive).toBe("boolean");
        });
    });
});

// ============================================================
// TEST GROUP 3: Event Emission
// ============================================================
describe("ModelOrchestrator — Events", () => {
    let orchestrator: ModelOrchestrator;

    beforeEach(() => {
        orchestrator = new ModelOrchestrator();
    });

    afterEach(() => {
        orchestrator.stopRouter();
        orchestrator.removeAllListeners();
    });

    it("should be an EventEmitter", () => {
        expect(typeof orchestrator.on).toBe("function");
        expect(typeof orchestrator.emit).toBe("function");
    });

    it("should emit suspend_peripherals when starting expert", (done) => {
        // Only in local mode
        const originalProvider = process.env.AI_PROVIDER;
        process.env.AI_PROVIDER = "local";

        orchestrator.on("suspend_peripherals", () => {
            process.env.AI_PROVIDER = originalProvider;
            done();
        });

        const factory = ModelOrchestrator.getAuthorizedTokenFactory();
        const token = factory.issueToken("EXPERT_START_AUTH");

        // This will trigger suspend_peripherals and then fail on spawn (expected)
        orchestrator.startExpert(token).catch(() => {
            // Expected to fail (no llama-server.exe in test)
        });
    });
});
