import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

import { EventPipeline } from "../../../src/core/events/EventPipeline";
import type { DependencyContainer } from "../../../src/core/DependencyContainer";

describe("EventPipeline", () => {
    let mockDeps: any;
    let pipeline: EventPipeline;
    let uiHandlers: Record<string, Function>;
    let whisperHandlers: Record<string, Function>;
    let voiceHandlers: Record<string, Function>;

    beforeEach(() => {
        vi.clearAllMocks();

        uiHandlers = {};
        whisperHandlers = {};
        voiceHandlers = {};

        mockDeps = {
            ui: {
                on: vi.fn((event, handler) => { uiHandlers[event] = handler; }),
                broadcastUIEvent: vi.fn(),
                sendSkillsList: vi.fn(),
                sendSystemStatus: vi.fn(),
                removeListener: vi.fn()
            },
            whisperNode: {
                on: vi.fn((event, handler) => { whisperHandlers[event] = handler; }),
                pushAudioChunk: vi.fn(),
                flush: vi.fn()
            },
            voiceEngine: {
                on: vi.fn((event, handler) => { voiceHandlers[event] = handler; }),
                preempt: vi.fn(),
                pushTokens: vi.fn(),
                flushTTS: vi.fn()
            },
            agentLoop: {
                Orchestrator: {
                    on: vi.fn(),
                    startAnomalyDetection: vi.fn()
                },
                onThinkingStart: null as Function | null,
                onThinkingEnd: null as Function | null,
                onSpokenResponse: null as Function | null,
                onStreamStart: null as Function | null,
                onStreamChunk: null as Function | null,
                onExecApprovalRequired: null as Function | null,
            },
            registry: {
                getAllSkills: vi.fn().mockReturnValue([{ name: "skill1", description: "desc" }])
            },
            dispatch: vi.fn().mockResolvedValue(undefined),
            addTelemetryLog: vi.fn()
        };

        pipeline = new EventPipeline(mockDeps as unknown as DependencyContainer);
        pipeline.wireListeners();
    });

    describe("Audio Pipeline", () => {
        it("should pipe audio input from UI to whisper", () => {
            const buf = Buffer.from("test");
            uiHandlers["audio_input"](buf);
            expect(mockDeps.whisperNode.pushAudioChunk).toHaveBeenCalledWith(buf);
        });

        it("should preempt voice and flush whisper on UI interrupt", () => {
            uiHandlers["interrupt"]();
            expect(mockDeps.voiceEngine.preempt).toHaveBeenCalled();
            expect(mockDeps.whisperNode.flush).toHaveBeenCalled();
        });

        it("should dispatch agent_input on transcription ready", async () => {
            await whisperHandlers["transcription_ready"]("hello AI");
            expect(mockDeps.dispatch).toHaveBeenCalledWith("agent_input", "hello AI");
        });

        it("should broadcast ai_audio_chunk to UI", () => {
            voiceHandlers["audio_base64"]("base64data");
            expect(mockDeps.ui.broadcastUIEvent).toHaveBeenCalledWith("ai_audio_chunk", { audio: "base64data" });
        });
    });

    describe("Dashboard Events", () => {
        it("should send skills list to dashboard", () => {
            uiHandlers["get_skills_list"]({});
            expect(mockDeps.ui.sendSkillsList).toHaveBeenCalledWith({}, [{ name: "skill1", description: "desc", isCoreSkill: false }]);
        });

        it("should send system status", () => {
            uiHandlers["get_system_status"]({});
            expect(mockDeps.ui.sendSystemStatus).toHaveBeenCalled();
        });
    });

    describe("Reactive Sync (AgentLoop -> UI)", () => {
        it("should broadcast thinking start and clear voice", async () => {
            await mockDeps.agentLoop.onThinkingStart();
            expect(mockDeps.voiceEngine.preempt).toHaveBeenCalled();
            expect(mockDeps.dispatch).toHaveBeenCalledWith("ui_broadcast", { name: "ai_thinking_start" });
        });

        it("should broadcast spoken response but ignore HEARTBEAT_OK", async () => {
            await mockDeps.agentLoop.onSpokenResponse("Normal text");
            expect(mockDeps.dispatch).toHaveBeenCalledWith("ui_broadcast", expect.objectContaining({ name: "ai_spoken_response", data: { text: "Normal text" } }));

            mockDeps.dispatch.mockClear();
            await mockDeps.agentLoop.onSpokenResponse("HEARTBEAT_OK");
            expect(mockDeps.dispatch).not.toHaveBeenCalled();
        });

        it("should broadcast stream chunk and push tokens", async () => {
            await mockDeps.agentLoop.onStreamChunk("chunk");
            expect(mockDeps.voiceEngine.pushTokens).toHaveBeenCalledWith("chunk");
            expect(mockDeps.dispatch).toHaveBeenCalledWith("ui_broadcast", expect.objectContaining({ name: "ai_stream_chunk" }));
        });

        it("should handle exec approval required flow and resolve timeout", async () => {
            vi.useFakeTimers();
            const promise = mockDeps.agentLoop.onExecApprovalRequired("test_tool", "cmd", "reason");
            
            // Should dispatch approval required
            expect(mockDeps.dispatch).toHaveBeenCalledWith("ui_broadcast", expect.objectContaining({ name: "exec_approval_required" }));
            
            // Advance timeout 30s
            vi.advanceTimersByTime(30000);
            
            const result = await promise;
            expect(result.approved).toBe(false); // Automatically rejected on timeout
            
            vi.useRealTimers();
        });
    });
});
