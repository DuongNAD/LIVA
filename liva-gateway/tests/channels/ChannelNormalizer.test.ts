/**
 * ChannelNormalizer.test.ts — ChannelRouter unit tests
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ChannelRouter } from "../../src/channels/ChannelNormalizer";
import type { ChannelAdapter, NormalizedMessage, ChannelType } from "../../src/channels/ChannelNormalizer";

// Helper: create a mock adapter
function createMockAdapter(channelName: ChannelType): ChannelAdapter {
    return {
        channelName,
        sendText: vi.fn().mockResolvedValue(undefined),
        sendApprovalCard: vi.fn().mockResolvedValue(undefined),
        sendScreenshot: vi.fn().mockResolvedValue(undefined),
    };
}

// Helper: create a normalized message
function createMsg(channel: ChannelType, senderId: string, text: string): NormalizedMessage {
    return {
        channel,
        senderId,
        text,
        rawPayload: {},
        timestamp: Date.now(),
    };
}

describe("ChannelRouter", () => {
    let router: ChannelRouter;
    let telegramAdapter: ChannelAdapter;
    let messengerAdapter: ChannelAdapter;

    beforeEach(() => {
        router = new ChannelRouter();
        telegramAdapter = createMockAdapter("telegram");
        messengerAdapter = createMockAdapter("messenger");
    });

    describe("register()", () => {
        it("should register an adapter", () => {
            router.register(telegramAdapter);
            expect(router.getRegisteredChannels()).toContain("telegram");
        });

        it("should register multiple adapters", () => {
            router.register(telegramAdapter);
            router.register(messengerAdapter);
            expect(router.getRegisteredChannels()).toHaveLength(2);
        });

        it("should overwrite adapter with same channel name", () => {
            const adapter2 = createMockAdapter("telegram");
            router.register(telegramAdapter);
            router.register(adapter2);
            expect(router.getRegisteredChannels()).toHaveLength(1);
            expect(router.getAdapter("telegram")).toBe(adapter2);
        });
    });

    describe("getAdapter()", () => {
        it("should return registered adapter", () => {
            router.register(telegramAdapter);
            expect(router.getAdapter("telegram")).toBe(telegramAdapter);
        });

        it("should return undefined for unregistered channel", () => {
            expect(router.getAdapter("telegram")).toBeUndefined();
        });
    });

    describe("getRegisteredChannels()", () => {
        it("should return empty array when no adapters registered", () => {
            expect(router.getRegisteredChannels()).toEqual([]);
        });

        it("should return all registered channel names", () => {
            router.register(telegramAdapter);
            router.register(messengerAdapter);
            const channels = router.getRegisteredChannels();
            expect(channels).toContain("telegram");
            expect(channels).toContain("messenger");
        });
    });

    describe("replyText()", () => {
        it("should route text to the correct adapter", async () => {
            router.register(telegramAdapter);
            const msg = createMsg("telegram", "user123", "Hello");

            await router.replyText(msg, "Reply text");

            expect(telegramAdapter.sendText).toHaveBeenCalledWith("user123", "Reply text");
        });

        it("should be a no-op if adapter not found", async () => {
            const msg = createMsg("telegram", "user123", "Hello");
            // Should not throw
            await router.replyText(msg, "Reply text");
        });

        it("should route to correct adapter among multiple", async () => {
            router.register(telegramAdapter);
            router.register(messengerAdapter);
            const msg = createMsg("messenger", "user456", "Hi");

            await router.replyText(msg, "Reply");

            expect(messengerAdapter.sendText).toHaveBeenCalledWith("user456", "Reply");
            expect(telegramAdapter.sendText).not.toHaveBeenCalled();
        });
    });

    describe("sendApproval()", () => {
        it("should forward approval card to correct adapter", async () => {
            router.register(telegramAdapter);
            const msg = createMsg("telegram", "user123", "");

            await router.sendApproval(msg, "Approval Title", "Some action", "approval-001");

            expect(telegramAdapter.sendApprovalCard).toHaveBeenCalledWith(
                "user123", "Approval Title", "Some action", "approval-001"
            );
        });

        it("should be a no-op if adapter not found", async () => {
            const msg = createMsg("instagram", "user789", "");
            await router.sendApproval(msg, "Title", "Body", "id-123");
            // No crash
        });
    });
});
