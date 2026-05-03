import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { SessionOrchestrator } from "../../src/core/SessionOrchestrator";

describe("SessionOrchestrator", () => {
    let orchestrator: SessionOrchestrator;

    beforeEach(() => {
        vi.useFakeTimers();
        orchestrator = new SessionOrchestrator();
    });

    afterEach(() => {
        orchestrator.dispose();
        vi.restoreAllMocks();
    });

    describe("Session Lifecycle", () => {
        it("should create a new session if it doesn't exist", () => {
            const session = orchestrator.getOrCreateSession("user123", "telegram");
            expect(session.id).toBe("telegram_user123");
            expect(session.activeIDE).toBeNull();
            expect(orchestrator.activeSessionCount).toBe(1);
        });

        it("should return existing session on subsequent calls", () => {
            const s1 = orchestrator.getOrCreateSession("user123", "telegram");
            const s2 = orchestrator.getOrCreateSession("user123", "telegram");
            expect(s1).toBe(s2);
            expect(orchestrator.activeSessionCount).toBe(1);
        });

        it("should create different sessions for different users/channels", () => {
            orchestrator.getOrCreateSession("user1", "telegram");
            orchestrator.getOrCreateSession("user2", "telegram");
            orchestrator.getOrCreateSession("user1", "zalo");
            expect(orchestrator.activeSessionCount).toBe(3);
        });
    });

    describe("Workspace Management", () => {
        it("should update active IDE and project path", () => {
            orchestrator.getOrCreateSession("user123", "telegram");
            
            let eventFired = false;
            orchestrator.on("workspace_changed", (s) => {
                eventFired = true;
                expect(s.activeIDE).toBe("vscode");
                expect(s.projectPath).toBe("/projects/test");
            });

            orchestrator.switchWorkspace("telegram_user123", "vscode", "/projects/test");
            
            const session = orchestrator.getOrCreateSession("user123", "telegram");
            expect(session.activeIDE).toBe("vscode");
            expect(session.projectPath).toBe("/projects/test");
            expect(eventFired).toBe(true);
        });

        it("should safely ignore switching workspace for non-existent session", () => {
            expect(() => orchestrator.switchWorkspace("invalid_id", "vscode", "/")).not.toThrow();
        });
    });

    describe("Message History", () => {
        const mockMsg = (text: string) => ({
            id: "1", senderId: "u", senderName: "n", channel: "tg", text, timestamp: 1
        });

        it("should append messages and retrieve history", () => {
            const sId = "telegram_u";
            orchestrator.getOrCreateSession("u", "telegram");
            
            orchestrator.appendMessage(sId, mockMsg("hello"));
            orchestrator.appendMessage(sId, mockMsg("world"));

            const history = orchestrator.getSessionHistory(sId);
            expect(history).toHaveLength(2);
            expect(history[0].text).toBe("hello");
            expect(history[1].text).toBe("world");
        });

        it("should respect max history length (50)", () => {
            const sId = "telegram_u";
            orchestrator.getOrCreateSession("u", "telegram");

            for (let i = 0; i < 55; i++) {
                orchestrator.appendMessage(sId, mockMsg(`msg${i}`));
            }

            const history = orchestrator.getSessionHistory(sId);
            expect(history).toHaveLength(50);
            expect(history[0].text).toBe("msg5"); // First 5 dropped
        });

        it("should clear history", () => {
            const sId = "telegram_u";
            orchestrator.getOrCreateSession("u", "telegram");
            orchestrator.appendMessage(sId, mockMsg("hello"));
            
            orchestrator.clearHistory(sId);
            expect(orchestrator.getSessionHistory(sId)).toHaveLength(0);
        });

        it("should safely handle appendMessage on non-existent session (Line 116)", () => {
            // Should not throw
            orchestrator.appendMessage("fake_session", mockMsg("hello"));
            expect(orchestrator.getSessionHistory("fake_session")).toHaveLength(0);
        });

        it("should safely handle clearHistory on non-existent session", () => {
            // Should not throw
            orchestrator.clearHistory("fake_session");
            expect(orchestrator.getSessionHistory("fake_session")).toHaveLength(0);
        });
    });

    describe("Garbage Collection", () => {
        it("should evict idle sessions after timeout (24h)", () => {
            orchestrator.getOrCreateSession("u", "telegram");
            expect(orchestrator.activeSessionCount).toBe(1);

            // Advance time by 25 hours
            vi.advanceTimersByTime(1000 * 60 * 60 * 25);

            expect(orchestrator.activeSessionCount).toBe(0);
        });

        it("should not evict active sessions", () => {
            const sId = "telegram_u";
            orchestrator.getOrCreateSession("u", "telegram");

            // Advance 12 hours
            vi.advanceTimersByTime(1000 * 60 * 60 * 12);
            
            // User interacts
            orchestrator.switchWorkspace(sId, "vscode", "/");

            // Advance another 13 hours (total 25h from start, but 13h from last activity)
            vi.advanceTimersByTime(1000 * 60 * 60 * 13);

            expect(orchestrator.activeSessionCount).toBe(1);
        });
    });
});
