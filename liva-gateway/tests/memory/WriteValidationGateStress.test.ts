import { describe, it, expect, vi, beforeEach } from "vitest";
import { WriteValidationGate } from "../../src/memory/WriteValidationGate";
import { ProcessSessionsStep, type StepDependencies } from "../../src/memory/ConsolidationSteps";
import type { ConsolidationContext } from "../../src/memory/ConsolidationPipeline";
import { logger } from "../../src/utils/logger";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn()
    }
}));

describe("WriteValidationGate & Consolidation Integration Stress Testing", () => {
    let gate: WriteValidationGate;

    beforeEach(() => {
        vi.clearAllMocks();
        gate = WriteValidationGate.getInstance();
    });

    describe("WriteValidationGate - Heuristics Validation", () => {
        it("should detect English negation contradictions", async () => {
            const coreFacts = ["User lives in Hanoi", "User loves soccer"];
            
            const res1 = await gate.validateUpdate("User does not live in Hanoi", coreFacts);
            expect(res1).toBe(false);

            const res2 = await gate.validateUpdate("User doesn't love soccer", coreFacts);
            expect(res2).toBe(false);

            // "User never loves soccer" has high word overlap (user, loves, soccer) after stripping negations
            const res3 = await gate.validateUpdate("User never loves soccer", coreFacts);
            expect(res3).toBe(false);
        });

        it("should detect Vietnamese negation contradictions", async () => {
            const coreFacts = ["Người dùng sống ở Hà Nội", "Tôi thích uống cà phê"];

            const res1 = await gate.validateUpdate("Người dùng không sống ở Hà Nội", coreFacts);
            expect(res1).toBe(false);

            const res2 = await gate.validateUpdate("Tôi chưa từng thích uống cà phê", coreFacts);
            expect(res2).toBe(false);

            const res3 = await gate.validateUpdate("Tôi không thích uống cà phê", coreFacts);
            expect(res3).toBe(false);
        });

        it("should allow safe / non-contradictory facts", async () => {
            const coreFacts = ["User lives in Hanoi", "User loves soccer"];

            const res1 = await gate.validateUpdate("User lives in Hanoi and works as a doctor", coreFacts);
            expect(res1).toBe(true);

            const res2 = await gate.validateUpdate("User likes coffee", coreFacts);
            expect(res2).toBe(true);
        });
    });

    describe("WriteValidationGate - Concurrency & Load Stress", () => {
        it("should handle 1,000 rapid concurrent validations without memory corruption or race conditions", async () => {
            const coreFacts = ["User is a developer", "LIVA is an AI assistant"];
            const promises: Promise<boolean>[] = [];

            // Trigger 1000 concurrent updates
            for (let i = 0; i < 1000; i++) {
                const isContradictory = i % 2 === 0;
                const proposed = isContradictory 
                    ? "User is not a developer" 
                    : `User is a developer who loves programming language ${i}`;
                promises.push(gate.validateUpdate(proposed, coreFacts));
            }

            const results = await Promise.all(promises);

            expect(results.length).toBe(1000);
            
            // Check that all odd entries (safe) returned true and even entries (contradictory) returned false
            for (let i = 0; i < 1000; i++) {
                const isContradictory = i % 2 === 0;
                if (isContradictory) {
                    expect(results[i]).toBe(false);
                } else {
                    expect(results[i]).toBe(true);
                }
            }
        });
    });

    describe("WriteValidationGate - LLM Fail-Secure Resilience", () => {
        it("should fail-secure (return false) and not crash when LLM client throws a connection error", async () => {
            const mockAiClient = {
                chat: {
                    completions: {
                        create: vi.fn().mockRejectedValue(new Error("API Timeout or Network Down"))
                    }
                }
            } as any;

            const res = await gate.validateUpdate("User likes coffee", ["User hates coffee"], mockAiClient);
            expect(res).toBe(false); // Fails secure
            expect(logger.error).toHaveBeenCalledWith(
                expect.stringContaining("[SSGM] ❌ Validation Gate failed to process")
            );
        });

        it("should fall back to heuristics and block direct negations when LLM returns malformed JSON", async () => {
            const mockAiClient = {
                chat: {
                    completions: {
                        create: vi.fn().mockResolvedValue({
                            choices: [{ message: { content: "unparseable garbage response text" } }]
                        })
                    }
                }
            } as any;

            // Using direct negation contradiction so the fallback heuristic catches it
            const res = await gate.validateUpdate("User does not like coffee", ["User likes coffee"], mockAiClient);
            expect(res).toBe(false);
        });
    });

    describe("Consolidation integration - ProcessSessionsStep Concurrency & Contradiction Protection", () => {
        it("should successfully consolidate safe facts and reject contradictory facts without crashing the consolidation loop", async () => {
            // Setup mock dependencies
            const mockStructuredMemory = {
                getUnconsolidatedEvents: vi.fn(),
                markConsolidated: vi.fn().mockResolvedValue(undefined),
                upsertVector: vi.fn().mockResolvedValue(undefined),
                setFact: vi.fn().mockResolvedValue(undefined),
                getAllFacts: vi.fn().mockReturnValue([
                    { value: "User lives in Hanoi" },
                    { value: "User hates tea" }
                ]),
                getDb: vi.fn(),
                gcOldEvents: vi.fn(),
                processDLQ: vi.fn(),
                applyMemoryDecay: vi.fn(),
                createSnapshotBackup: vi.fn(),
                graph: {
                    upsertNode: vi.fn().mockResolvedValue(undefined),
                    upsertEdge: vi.fn().mockResolvedValue(undefined),
                }
            } as any;

            const mockEmbeddingService = {
                embed: vi.fn().mockResolvedValue(new Float32Array(128)),
            } as any;

            const mockAiClient = {
                chat: {
                    completions: {
                        create: vi.fn(async (args: any) => {
                            const promptContent = args.messages[0].content || "";
                            if (promptContent.includes("Dữ kiện Cốt lõi")) {
                                const isContradict = promptContent.includes("User loves tea") || promptContent.includes("User doesn't live in Hanoi");
                                return {
                                    choices: [{ message: { content: JSON.stringify({ contradict: isContradict }) } }]
                                };
                            }
                            return {
                                choices: [{
                                    message: {
                                        content: JSON.stringify({
                                            narrative_summary: "User drank some coffee.",
                                            new_user_insights: [
                                                { key: "drink_preference", value: "User loves tea", category: "General" },
                                                { key: "drink_preference", value: "User doesn't live in Hanoi", category: "General" },
                                                { key: "occupation", value: "User is a programmer", category: "Work" }
                                            ],
                                            graph_nodes: [],
                                            graph_edges: []
                                        })
                                    }
                                }]
                            };
                        })
                    }
                }
            } as any;

            const mockDeps: StepDependencies = {
                structuredMemory: mockStructuredMemory,
                embeddingService: mockEmbeddingService,
                aiClient: mockAiClient,
                bookIndex: {
                    addNode: vi.fn(),
                    addEdge: vi.fn()
                } as any,
                contradictionResolver: {
                    resolve: vi.fn().mockResolvedValue(undefined)
                } as any,
                reconsolidationEngine: null,
                synthesisPrompt: "System synthesis prompt"
            };

            const ctx: ConsolidationContext = {
                startedAt: Date.now(),
                totalConsolidated: 0,
                sharedState: {
                    sessions: [
                        {
                            events: [
                                {
                                    eventId: "evt_1",
                                    timestamp: Date.now(),
                                    phi: { facts: [] },
                                    psi: { sentiment: "Neutral" },
                                    rawUserMsg: "I want a cup of tea.",
                                    rawAiReply: "Sure, here it is."
                                }
                            ],
                            startTime: Date.now(),
                            endTime: Date.now()
                        }
                    ]
                }
            };

            const step = new ProcessSessionsStep(mockDeps);
            
            // Execute the consolidation step
            await expect(step.execute(ctx)).resolves.not.toThrow();

            // Verify safe fact was written
            expect(mockStructuredMemory.setFact).toHaveBeenCalledWith("occupation", "User is a programmer", expect.any(Object));

            // Verify contradictory facts were NOT written
            expect(mockStructuredMemory.setFact).not.toHaveBeenCalledWith("drink_preference", "User loves tea", expect.any(Object));
            expect(mockStructuredMemory.setFact).not.toHaveBeenCalledWith("drink_preference", "User doesn't live in Hanoi", expect.any(Object));

            // Verify warning logs were captured
            expect(logger.warn).toHaveBeenCalledWith(
                expect.stringContaining("Insight L3 update blocked by WriteValidationGate")
            );
        });
    });
});
