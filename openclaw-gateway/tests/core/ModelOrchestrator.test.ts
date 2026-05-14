import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ModelOrchestrator, type TaskToken } from "../../src/core/ModelOrchestrator";
import { safeFetch } from "../../src/utils/HttpClient";
import { spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import treeKill from "tree-kill";

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

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn()
}));

vi.mock("node:child_process", () => ({
    spawn: vi.fn(() => ({
        pid: 12345,
        on: vi.fn(),
        kill: vi.fn()
    }))
}));

vi.mock("node:fs", () => ({
    default: {
        existsSync: vi.fn().mockReturnValue(false),
        readFileSync: vi.fn().mockReturnValue("{}")
    }
}));

vi.mock("node:net", () => {
    return {
        default: {
            createServer: vi.fn(() => {
                let errorCb: Function;
                return {
                    listen: vi.fn(function(this: any, port, host, cb) { 
                        // simulate success immediately
                        if (cb) cb(); 
                    }),
                    close: vi.fn((cb) => cb && cb()),
                    on: vi.fn((event, cb) => {
                        if (event === "error") errorCb = cb;
                    }),
                    address: vi.fn(() => ({ port: port === 0 ? 8080 : port }))
                };
            })
        }
    };
});

describe("ModelOrchestrator — Single Expert Architecture (P4)", () => {
    let orchestrator: ModelOrchestrator;
    const originalEnv = { ...process.env };

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        orchestrator = new ModelOrchestrator();
        process.env.LIVA_USE_NATIVE = "false";
        process.env.AI_PROVIDER = "local";
    });

    afterEach(() => {
        orchestrator.killLlamaServer();
        process.env = { ...originalEnv };
        vi.useRealTimers();
    });

    describe("Token Factory", () => {
        it("should mint a ROUTER_START_AUTH token", () => {
            const factory = ModelOrchestrator.getAuthorizedTokenFactory();
            expect(factory.issueToken("ROUTER_START_AUTH")).toBe("ROUTER_START_AUTH");
        });
    });

    describe("startSingleExpert (alias: startRouter)", () => {
        it("should reject invalid auth token", async () => {
            await expect(orchestrator.startRouter("INVALID" as any)).rejects.toThrow("Unauthorized");
        });

        it("should activate in native mode immediately", async () => {
            process.env.LIVA_USE_NATIVE = "true";
            const token = ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH");
            await orchestrator.startRouter(token);
            expect(orchestrator.getStatus().routerActive).toBe(true);
        });

        it("should spawn llama-server, wait for healthcheck, and resolve", async () => {
            // Mock readHardwareConfig to hit the VRAM logic
            (fs.existsSync as any).mockReturnValue(true);
            (fs.readFileSync as any).mockReturnValue(JSON.stringify({ vram_mb: 8192, ram_mb: 32000, cpu_threads: 8, is_battery: true }));
            
            vi.mocked(safeFetch).mockResolvedValueOnce({} as any);

            const token = ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH");
            
            const startPromise = orchestrator.startRouter(token);
            await vi.advanceTimersByTimeAsync(1000); // Trigger setInterval
            await startPromise;

            expect(spawn).toHaveBeenCalled();
            expect(orchestrator.getStatus().routerActive).toBe(true);
            expect(orchestrator.routerPort).toBe(8000);
        });

        it("should handle timeout if healthcheck never passes", async () => {
            vi.mocked(safeFetch).mockRejectedValue(new Error("Timeout"));

            const token = ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH");
            const startPromise = orchestrator.startRouter(token);
            
            // Advance past the 90s timeout
            await vi.advanceTimersByTimeAsync(91000);
            
            await startPromise; // Resolves anyway
            expect(orchestrator.getStatus().routerActive).toBe(false);
        });
    });

    describe("killLlamaServer (alias: stopRouter)", () => {
        it("should do nothing if server is not running", async () => {
            await expect(orchestrator.killLlamaServer()).resolves.not.toThrow();
        });

        it("should tree-kill llama process if running", async () => {
            vi.mocked(safeFetch).mockResolvedValueOnce({} as any);
            const token = ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH");
            const pR = orchestrator.startRouter(token);
            await vi.advanceTimersByTimeAsync(1000);
            await pR;

            await orchestrator.killLlamaServer();
            expect(treeKill).toHaveBeenCalled();
            expect(orchestrator.getStatus().routerActive).toBe(false);
        });
    });

    describe("Anomaly Detection", () => {
        it("should restart router on 3 consecutive failures", async () => {
            vi.mocked(safeFetch).mockResolvedValueOnce({} as any); // Start succeeds
            
            const token = ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH");
            const startPromise = orchestrator.startRouter(token);
            await vi.advanceTimersByTimeAsync(1000);
            await startPromise;

            // Start anomaly detection
            orchestrator.startAnomalyDetection();

            // Fail ping 1
            vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Fail 1"));
            await vi.advanceTimersByTimeAsync(15000);
            
            // Fail ping 2
            vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Fail 2"));
            await vi.advanceTimersByTimeAsync(15000);

            // Mock success for the restart
            vi.mocked(safeFetch).mockResolvedValueOnce({} as any);

            // Fail ping 3 -> Should trigger restart
            vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Fail 3"));
            await vi.advanceTimersByTimeAsync(15000);
            
            // Need to flush promises for restart logic
            await vi.advanceTimersByTimeAsync(3000); // Delay in restartRouter (2000) + Health check (500)

            // It should have restarted and be active again
            expect(orchestrator.getStatus().routerActive).toBe(true);
        });
    });
});
