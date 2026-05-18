import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

vi.mock("../../../src/services/SmartTurnVAD", () => ({
    SmartTurnVAD: vi.fn()
}));

// Mock process.env
const originalEnv = process.env;

import { BootstrapManager } from "../../../src/core/bootstrap/BootstrapManager";
import type { DependencyContainer } from "../../../src/core/DependencyContainer";

describe("BootstrapManager", () => {
    let mockDeps: any;
    let bootstrapManager: BootstrapManager;

    beforeEach(() => {
        process.env = { ...originalEnv };
        vi.clearAllMocks();

        mockDeps = {
            memory: { initialize: vi.fn().mockResolvedValue(undefined), initUHM: vi.fn() },
            registry: { 
                registerLocalSkills: vi.fn().mockResolvedValue(undefined),
                warmUpCache: vi.fn().mockResolvedValue(undefined)
            },
            agentLoop: {
                initModels: vi.fn().mockResolvedValue(undefined),
                Orchestrator: {
                    startAnomalyDetection: vi.fn(),
                    routerPort: 8000
                }
            },
            appWatcher: { start: vi.fn(), setCallback: vi.fn() },
            gitNexusIndexer: { triggerIndex: vi.fn() },
            emailManager: { startIdling: vi.fn().mockResolvedValue(undefined) },
            heartbeat: { start: vi.fn() },
            securityGateway: { isRemoteControlEnabled: vi.fn().mockReturnValue(false) },
            telegram: { startPolling: vi.fn() },
            meta: { startWebhookServer: vi.fn().mockResolvedValue(undefined) },
            cdpBridge: { connect: vi.fn().mockResolvedValue(undefined), watchForApprovalButtons: vi.fn().mockResolvedValue(undefined) },
            vscodeBridge: { connect: vi.fn().mockResolvedValue(undefined) },
            channelRouter: { getRegisteredChannels: vi.fn().mockReturnValue(["telegram", "cdp"]) },
            dispatch: vi.fn().mockResolvedValue(undefined),
            getDefaultRemoteSenderId: vi.fn().mockReturnValue("owner")
        };

        bootstrapManager = new BootstrapManager(mockDeps as unknown as DependencyContainer);
    });

    it("should execute full boot sequence successfully", async () => {
        await bootstrapManager.boot();

        expect(mockDeps.memory.initialize).toHaveBeenCalled();
        expect(mockDeps.registry.registerLocalSkills).toHaveBeenCalled();
        expect(mockDeps.agentLoop.initModels).toHaveBeenCalled();
        expect(mockDeps.agentLoop.Orchestrator.startAnomalyDetection).toHaveBeenCalled();
        expect(mockDeps.appWatcher.start).toHaveBeenCalled();
        expect(mockDeps.gitNexusIndexer.triggerIndex).toHaveBeenCalled();
        expect(mockDeps.emailManager.startIdling).toHaveBeenCalled();
        expect(mockDeps.heartbeat.start).toHaveBeenCalled();
        expect(mockDeps.securityGateway.isRemoteControlEnabled).toHaveBeenCalled();
    });

    it("should initialize UHM successfully", async () => {
        process.env.AI_PROVIDER = "local";
        await bootstrapManager.boot();
        expect(mockDeps.memory.initUHM).toHaveBeenCalled();
    });

    it("should boot RemoteControlHub successfully when enabled", async () => {
        mockDeps.securityGateway.isRemoteControlEnabled.mockReturnValue(true);
        await bootstrapManager.boot();

        expect(mockDeps.telegram.startPolling).toHaveBeenCalled();
        expect(mockDeps.meta.startWebhookServer).toHaveBeenCalled();
        expect(mockDeps.cdpBridge.connect).toHaveBeenCalled();
        expect(mockDeps.vscodeBridge.connect).toHaveBeenCalled();
    });

    it("should handle error in EmailClient startup gracefully", async () => {
        mockDeps.emailManager.startIdling.mockRejectedValue(new Error("IMAP Error"));
        // Should not throw, just log
        await bootstrapManager.boot();
        expect(mockDeps.emailManager.startIdling).toHaveBeenCalled();
    });

    it("should handle error in MetaBridge webhook startup gracefully", async () => {
        mockDeps.securityGateway.isRemoteControlEnabled.mockReturnValue(true);
        mockDeps.meta.startWebhookServer.mockRejectedValue(new Error("Port busy"));
        await bootstrapManager.boot();
        expect(mockDeps.meta.startWebhookServer).toHaveBeenCalled();
    });

    it("should handle error in CDPBridge initial connect gracefully", async () => {
        mockDeps.securityGateway.isRemoteControlEnabled.mockReturnValue(true);
        mockDeps.cdpBridge.connect.mockRejectedValue(new Error("Connection refused"));
        await bootstrapManager.boot();
        expect(mockDeps.cdpBridge.connect).toHaveBeenCalled();
    });
    
    it("should trigger dispatch when appWatcher callback is executed", async () => {
        await bootstrapManager.boot();
        expect(mockDeps.appWatcher.setCallback).toHaveBeenCalled();
        const callback = mockDeps.appWatcher.setCallback.mock.calls[0][0];
        
        await callback("Discord", { type: "Chat", description: "Gaming" });
        expect(mockDeps.dispatch).toHaveBeenCalledWith("agent_input", expect.stringContaining("Discord"));
    });
});
