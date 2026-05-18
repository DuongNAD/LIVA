import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { SessionOrchestrator } from "../../src/core/SessionOrchestrator";
import { MetaBridge } from "../../src/channels/MetaBridge";
import * as crypto from "node:crypto";
import * as http from "node:http";

// Mock safeFetch for MetaBridge
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn().mockResolvedValue({ ok: true })
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn()
    }
}));

describe("System Stress Testing & Security Audit", () => {
    
    describe("SessionOrchestrator - Concurrency & Load", () => {
        let orchestrator: SessionOrchestrator;
        
        beforeEach(() => {
            orchestrator = new SessionOrchestrator();
        });

        afterEach(() => {
            orchestrator.dispose();
        });

        it("should handle 10,000 rapid concurrent session creations without collision", async () => {
            const promises = [];
            for (let i = 0; i < 10000; i++) {
                // Simulate rapid concurrent messages from 1000 different users
                const userId = `user_${i % 1000}`;
                promises.push(new Promise<void>(resolve => {
                    setImmediate(() => {
                        const session = orchestrator.getOrCreateSession(userId, "messenger");
                        orchestrator.appendMessage(session.id, {
                            id: `msg_${i}`,
                            channel: "messenger",
                            senderId: userId,
                            text: `Stress test ${i}`,
                            timestamp: Date.now()
                        });
                        resolve();
                    });
                }));
            }

            await Promise.all(promises);

            // Verify exactly 1000 unique sessions were created
            // We can't access private #sessions easily, but we can verify by retrieving them
            for (let i = 0; i < 1000; i++) {
                const session = orchestrator.getOrCreateSession(`user_${i}`, "messenger");
                expect(session.id).toBeDefined();
                // Each user should have 10 messages (10000 / 1000)
                expect(orchestrator.getSessionHistory(session.id).length).toBe(10);
            }
        });

        it("should enforce history bounds strictly under overflow load", () => {
            const session = orchestrator.getOrCreateSession("overflow_user", "telegram");
            
            // Push 200 messages (limit is 50)
            for (let i = 0; i < 200; i++) {
                orchestrator.appendMessage(session.id, {
                    id: `msg_${i}`,
                    channel: "telegram",
                    senderId: "overflow_user",
                    text: `Spam ${i}`,
                    timestamp: Date.now()
                });
            }

            const history = orchestrator.getSessionHistory(session.id);
            expect(history.length).toBe(50); // Bounded to 50
            expect(history[49].text).toBe("Spam 199"); // Last message kept
            expect(history[0].text).toBe("Spam 150"); // Oldest message kept
        });
    });

    describe("MetaBridge - HMAC Replay & Flood Protection", () => {
        let bridge: MetaBridge;
        let port = 3055;

        beforeEach(async () => {
            vi.clearAllMocks();
            process.env.META_APP_SECRET = "stress_secret";
            process.env.META_PAGE_ACCESS_TOKEN = "stress_token";
            bridge = new MetaBridge(port);
            await bridge.startWebhookServer();
        });

        afterEach(() => {
            bridge.stop();
        });

        const simulatePost = (body: string, signature: string): Promise<number> => {
            return new Promise((resolve) => {
                const req = http.request({
                    hostname: "127.0.0.1",
                    port: port,
                    path: "/webhook",
                    method: "POST",
                    headers: { 
                        "X-Hub-Signature-256": signature,
                        "Content-Type": "application/json",
                        "Content-Length": Buffer.byteLength(body),
                        "Connection": "close"
                    }
                }, (res) => {
                    // Consume data to free socket
                    res.on("data", () => {});
                    res.on("end", () => resolve(res.statusCode || 500));
                });
                req.on("error", (err) => { 
                    console.error("simulatePost Error:", err);
                    resolve(500); 
                });
                req.write(body);
                req.end();
            });
        };

        it("should aggressively reject 1000 requests with invalid signatures (Zero-Trust)", async () => {
            const body = JSON.stringify({ test: "data" });
            const invalidSignature = "sha256=abcdef1234567890";
            
            const results: number[] = [];
            for (let i = 0; i < 10; i++) {
                const batch = [];
                for (let j = 0; j < 100; j++) {
                    batch.push(simulatePost(body, invalidSignature));
                }
                results.push(...(await Promise.all(batch)));
            }

            // All must be 401 Unauthorized
            expect(results.every(code => code === 401)).toBe(true);
        });

        it("should successfully process burst of valid webhook payloads", async () => {
            let processedCount = 0;
            bridge.on("message", () => processedCount++);

            const results: number[] = [];
            const batch = [];
            
            for (let i = 0; i < 100; i++) {
                const body = JSON.stringify({
                    object: "page",
                    entry: [{
                        messaging: [{
                            sender: { id: `sender_${i}` },
                            message: { mid: `m_${i}`, text: "Stress" },
                            timestamp: Date.now()
                        }]
                    }]
                });
                const hash = crypto.createHmac("sha256", "stress_secret").update(body).digest("hex");
                batch.push(simulatePost(body, `sha256=${hash}`));
            }
            
            results.push(...(await Promise.all(batch)));

            // All must be 200 OK
            const badCodes = results.filter(code => code !== 200);
            expect(badCodes).toEqual([]);
            expect(processedCount).toBe(100);
        });
    });
});
