import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { PersistentQueue } from "../../../src/core/queue/PersistentQueue";
import * as path from "node:path";
import * as fs from "node:fs";

vi.mock("../../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

describe("PersistentQueue", () => {
    const TEST_DB_PATH = path.join(process.cwd(), "test_persistent_queue.db");
    let queue: PersistentQueue;

    beforeEach(() => {
        // Clean up any previous test DB
        if (fs.existsSync(TEST_DB_PATH)) {
            fs.unlinkSync(TEST_DB_PATH);
        }
        queue = new PersistentQueue(TEST_DB_PATH);
    });

    afterEach(() => {
        queue.dispose();
        // Clean up test DB files (including WAL and SHM)
        for (const suffix of ["", "-wal", "-shm"]) {
            const f = TEST_DB_PATH + suffix;
            if (fs.existsSync(f)) {
                try { fs.unlinkSync(f); } catch { }
            }
        }
    });

    describe("enqueue / dequeueAll", () => {
        it("should enqueue and dequeue messages in FIFO order", () => {
            queue.enqueue("zalo", "message 1");
            queue.enqueue("zalo", "message 2");
            queue.enqueue("zalo", "message 3");

            const messages = queue.dequeueAll("zalo");
            expect(messages).toEqual(["message 1", "message 2", "message 3"]);
        });

        it("should return empty array when no messages", () => {
            const messages = queue.dequeueAll("zalo");
            expect(messages).toEqual([]);
        });

        it("should remove messages after dequeue", () => {
            queue.enqueue("zalo", "test");
            queue.dequeueAll("zalo");
            expect(queue.isEmpty("zalo")).toBe(true);
        });
    });

    describe("Channel isolation", () => {
        it("should keep messages from different channels separate", () => {
            queue.enqueue("zalo", "zalo msg");
            queue.enqueue("telegram", "telegram msg");

            const zaloMsgs = queue.dequeueAll("zalo");
            const telegramMsgs = queue.dequeueAll("telegram");

            expect(zaloMsgs).toEqual(["zalo msg"]);
            expect(telegramMsgs).toEqual(["telegram msg"]);
        });
    });

    describe("count / isEmpty", () => {
        it("should count pending messages for a channel", () => {
            expect(queue.count("zalo")).toBe(0);

            queue.enqueue("zalo", "msg 1");
            queue.enqueue("zalo", "msg 2");

            expect(queue.count("zalo")).toBe(2);
            expect(queue.isEmpty("zalo")).toBe(false);
        });

        it("should report empty correctly", () => {
            expect(queue.isEmpty("zalo")).toBe(true);
        });
    });

    describe("Persistence", () => {
        it("should survive queue recreation (simulating process restart)", () => {
            queue.enqueue("zalo", "persisted message");
            queue.dispose();

            // Re-open the same DB
            const queue2 = new PersistentQueue(TEST_DB_PATH);
            const messages = queue2.dequeueAll("zalo");
            expect(messages).toEqual(["persisted message"]);
            queue2.dispose();
        });
    });
});
