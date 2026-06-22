import { describe, it, expect, vi, beforeEach } from "vitest";
import { EventEmitter } from "node:events";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Mock FileExplorer
const mockFileExplorerInstance = {
    listDirectory: vi.fn(),
    readFile: vi.fn(),
};
vi.mock("../../src/services/FileExplorer", () => ({
    FileExplorer: vi.fn().mockImplementation(() => mockFileExplorerInstance),
}));

// Mock HierarchicalGraphRAG
const mockGraphRagInstance = {
    system1Search: vi.fn(),
    system2DeepDive: vi.fn(),
};
vi.mock("../../src/evolution/HierarchicalGraphRAG", () => ({
    HierarchicalGraphRAG: vi.fn().mockImplementation(() => mockGraphRagInstance),
}));

// Mock GitNexusIndexer
const mockGitIndexerInstance = {
    triggerIndex: vi.fn(),
};
vi.mock("../../src/evolution/GitNexusIndexer", () => ({
    GitNexusIndexer: vi.fn().mockImplementation(() => mockGitIndexerInstance),
}));

// Mock child_process for taskkill fallback in panic handler
const mockExec = vi.fn();
vi.mock("child_process", () => ({
    exec: (...args: any[]) => mockExec(...args),
}));

import { TelegramCommandHandler } from "../../src/channels/TelegramCommandHandler";

describe("TelegramCommandHandler", () => {
    let handler: TelegramCommandHandler;
    let mockBot: any;
    let mockCdpBridge: any;
    let mockAutoAcceptDaemon: any;
    let mockBridge: any;
    let mockAgentLoop: any;
    let mockSessions: any;
    let mockMemory: any;

    const commandHandlers: Record<string, (...args: any[]) => any> = {};
    const actionHandlers: Array<{ pattern: RegExp | string; handler: (...args: any[]) => any }> = [];

    beforeEach(() => {
        vi.clearAllMocks();
        mockFileExplorerInstance.listDirectory.mockReset();
        mockFileExplorerInstance.readFile.mockReset();
        mockGraphRagInstance.system1Search.mockReset();
        mockGraphRagInstance.system2DeepDive.mockReset();
        mockGitIndexerInstance.triggerIndex.mockReset();
        mockExec.mockReset();

        mockBot = {
            command: vi.fn().mockImplementation((cmd, h) => {
                commandHandlers[cmd] = h;
            }),
            action: vi.fn().mockImplementation((pattern, h) => {
                actionHandlers.push({ pattern, handler: h });
            }),
        };

        mockCdpBridge = {
            send: vi.fn().mockResolvedValue({}),
        };

        mockAutoAcceptDaemon = {
            disable: vi.fn(),
        };

        mockBridge = new EventEmitter() as any;

        mockAgentLoop = {
            bargeIn: vi.fn(),
        };

        mockSessions = {};

        mockMemory = {
            getShortTermHistory: vi.fn(),
        };

        handler = new TelegramCommandHandler();
        handler.registerHandlers(
            mockBot,
            mockCdpBridge,
            mockAutoAcceptDaemon,
            mockBridge,
            mockAgentLoop,
            mockSessions,
            mockMemory
        );
    });

    it("should register all expected command and action handlers", () => {
        expect(mockBot.command).toHaveBeenCalledWith("start", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("help", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("status", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("panic", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("ask", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("latest", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("stop", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("ls", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("cat", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("graph_index", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("graph_s1", expect.any(Function));
        expect(mockBot.command).toHaveBeenCalledWith("graph_s2", expect.any(Function));
        expect(mockBot.action).toHaveBeenCalledWith(expect.any(RegExp), expect.any(Function));
    });

    it("should reply to /start command with Welcome text and chat ID", async () => {
        const ctx = {
            chat: { id: 98765 },
            reply: vi.fn(),
        } as any;

        await commandHandlers["start"](ctx);
        expect(ctx.reply).toHaveBeenCalledWith(
            expect.stringContaining("Xin chào! Tôi là LIVA Remote Control Hub"),
            expect.objectContaining({ parse_mode: "Markdown" })
        );
        expect(ctx.reply).toHaveBeenCalledWith(
            expect.stringContaining("98765"),
            expect.any(Object)
        );
    });

    it("should reply to /help command with help commands list", async () => {
        const ctx = {
            reply: vi.fn(),
        } as any;

        await commandHandlers["help"](ctx);
        expect(ctx.reply).toHaveBeenCalledWith(
            expect.stringContaining("LIVA Remote Control Hub"),
            expect.objectContaining({ parse_mode: "Markdown" })
        );
    });

    it("should reply to /status command with status status text", async () => {
        const ctx = {
            reply: vi.fn(),
        } as any;

        await commandHandlers["status"](ctx);
        expect(ctx.reply).toHaveBeenCalledWith(expect.stringContaining("🟢 Hệ thống LIVA đang hoạt động"));
    });

    describe("/panic command", () => {
        it("should disable auto-accept and close browser via CDP", async () => {
            const ctx = {
                reply: vi.fn(),
            } as any;

            await commandHandlers["panic"](ctx);
            expect(mockAutoAcceptDaemon.disable).toHaveBeenCalled();
            expect(mockCdpBridge.send).toHaveBeenCalledWith("Browser.close");
            expect(ctx.reply).toHaveBeenCalledWith(expect.stringContaining("PANIC: IDE đã bị đóng"));
        });

        it("should fallback to taskkill if CDP bridge close fails", async () => {
            const ctx = {
                reply: vi.fn(),
            } as any;
            mockCdpBridge.send.mockRejectedValueOnce(new Error("CDP error"));

            await commandHandlers["panic"](ctx);
            
            // Wait for dynamic import and callback
            await new Promise((r) => setTimeout(r, 10));

            expect(mockCdpBridge.send).toHaveBeenCalled();
            expect(mockExec).toHaveBeenCalledWith(expect.stringContaining("taskkill /F /IM Antigravity.exe"));
            expect(ctx.reply).toHaveBeenCalledWith(expect.stringContaining("PANIC: IDE đã bị đóng"));
        });
    });

    describe("/ask command", () => {
        it("should prompt user when query is missing", async () => {
            const ctx = {
                message: { text: "/ask " },
                reply: vi.fn(),
            } as any;

            await commandHandlers["ask"](ctx);
            expect(ctx.reply).toHaveBeenCalledWith(
                expect.stringContaining("Vui lòng nhập câu hỏi sau lệnh"),
                expect.any(Object)
            );
        });

        it("should emit message event via TelegramBridge when query is present", async () => {
            const ctx = {
                message: { text: "/ask check the logs" },
                from: { id: 111, first_name: "TestUser" },
                update: { message: { text: "/ask check the logs" } },
                reply: vi.fn(),
            } as any;

            const emitSpy = vi.spyOn(mockBridge, "emit");
            await commandHandlers["ask"](ctx);

            expect(emitSpy).toHaveBeenCalledWith(
                "message",
                expect.objectContaining({
                    channel: "telegram",
                    senderId: "111",
                    senderName: "TestUser",
                    text: "check the logs",
                })
            );
        });
    });

    describe("/ls and ls:callback command", () => {
        it("should call FileExplorer listDirectory and reply directory contents", async () => {
            const ctx = {
                message: { text: "/ls src" },
                reply: vi.fn(),
            } as any;

            mockFileExplorerInstance.listDirectory.mockResolvedValueOnce([
                { name: "main.ts", isDirectory: false, size: 2048 },
                { name: "components", isDirectory: true, size: 0 },
            ]);

            await commandHandlers["ls"](ctx);

            expect(mockFileExplorerInstance.listDirectory).toHaveBeenCalledWith("src");
            expect(ctx.reply).toHaveBeenCalledWith(
                expect.stringContaining("main.ts"),
                expect.objectContaining({
                    reply_markup: expect.objectContaining({
                        inline_keyboard: expect.arrayContaining([
                            expect.arrayContaining([
                                expect.objectContaining({ text: expect.stringContaining("components") }),
                            ]),
                        ]),
                    }),
                })
            );
        });

        it("should support ls callback query on directory selection click", async () => {
            const ctx = {
                match: ["ls:src/components", "src/components"],
                editMessageText: vi.fn(),
                answerCbQuery: vi.fn(),
            } as any;

            mockFileExplorerInstance.listDirectory.mockResolvedValueOnce([
                { name: "Button.ts", isDirectory: false, size: 1024 },
            ]);

            const lsAction = actionHandlers.find((a) => a.pattern.toString().includes("ls:"));
            expect(lsAction).toBeDefined();

            await lsAction!.handler(ctx);

            expect(mockFileExplorerInstance.listDirectory).toHaveBeenCalledWith("src/components");
            expect(ctx.editMessageText).toHaveBeenCalledWith(
                expect.stringContaining("Button.ts"),
                expect.any(Object)
            );
            expect(ctx.answerCbQuery).toHaveBeenCalled();
        });
    });

    describe("/cat command", () => {
        it("should prompt user when file path is missing", async () => {
            const ctx = {
                message: { text: "/cat" },
                reply: vi.fn(),
            } as any;

            await commandHandlers["cat"](ctx);
            expect(ctx.reply).toHaveBeenCalledWith(
                expect.stringContaining("Cần cung cấp đường dẫn tệp"),
                expect.any(Object)
            );
        });

        it("should fetch file contents and reply with markdown codeblocks", async () => {
            const ctx = {
                message: { text: "/cat src/app.ts" },
                reply: vi.fn(),
            } as any;

            mockFileExplorerInstance.readFile.mockResolvedValueOnce("console.log('test')");

            await commandHandlers["cat"](ctx);

            expect(mockFileExplorerInstance.readFile).toHaveBeenCalledWith("src/app.ts");
            expect(ctx.reply).toHaveBeenCalledWith(
                expect.stringContaining("console.log('test')"),
                expect.objectContaining({ parse_mode: "Markdown" })
            );
        });
    });

    describe("/graph commands", () => {
        it("/graph_index should trigger AST build indexing", async () => {
            const ctx = {
                reply: vi.fn(),
            } as any;

            await commandHandlers["graph_index"](ctx);
            expect(mockGitIndexerInstance.triggerIndex).toHaveBeenCalledWith(0);
            expect(ctx.reply).toHaveBeenCalledWith(expect.stringContaining("Đang khởi chạy AST Graph Builder"));
        });

        it("/graph_s1 should perform system1Search and reply matches", async () => {
            const ctx = {
                message: { text: "/graph_s1 DbClient" },
                reply: vi.fn(),
            } as any;

            mockGraphRagInstance.system1Search.mockResolvedValueOnce([
                { name: "DbClient", type: "class", filePath: "src/db.ts" },
            ]);

            await commandHandlers["graph_s1"](ctx);
            expect(mockGraphRagInstance.system1Search).toHaveBeenCalledWith("DbClient");
            expect(ctx.reply).toHaveBeenCalledWith(
                expect.stringContaining("[CLASS] `DbClient`"),
                expect.any(Object)
            );
        });

        it("/graph_s2 should perform system2DeepDive call graph search and reply call tree", async () => {
            const ctx = {
                message: { text: "/graph_s2 fetchUser" },
                reply: vi.fn(),
            } as any;

            mockGraphRagInstance.system2DeepDive.mockResolvedValueOnce([
                { caller: "fetchUser", callee: "db.query" },
            ]);

            await commandHandlers["graph_s2"](ctx);
            expect(mockGraphRagInstance.system2DeepDive).toHaveBeenCalledWith("fetchUser", 1);
            expect(ctx.reply).toHaveBeenCalledWith(
                expect.stringContaining("fetchUser"),
                expect.any(Object)
            );
        });
    });

    describe("/latest and /stop commands", () => {
        it("/latest should reply with latest AI assistant message", async () => {
            const ctx = {
                reply: vi.fn(),
            } as any;

            mockMemory.getShortTermHistory.mockResolvedValueOnce([
                { role: "user", content: "hello" },
                { role: "assistant", content: "Hi! How can I help you?" },
            ]);

            await commandHandlers["latest"](ctx);
            expect(ctx.reply).toHaveBeenCalledWith(
                expect.stringContaining("Hi! How can I help you?"),
                expect.objectContaining({ parse_mode: "Markdown" })
            );
        });

        it("/stop should trigger Agent bargeIn and abort execution", async () => {
            const ctx = {
                reply: vi.fn(),
            } as any;

            await commandHandlers["stop"](ctx);
            expect(mockAgentLoop.bargeIn).toHaveBeenCalled();
            expect(ctx.reply).toHaveBeenCalledWith(expect.stringContaining("Đã dừng phản hồi"));
        });
    });
});
