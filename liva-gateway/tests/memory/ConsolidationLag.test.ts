import { describe, it, expect, vi, beforeEach } from "vitest";
import { monitorEventLoopDelay } from "node:perf_hooks";
import { ProcessSessionsStep, type StepDependencies } from "../../src/memory/ConsolidationSteps";
import type { ConsolidationContext } from "../../src/memory/ConsolidationPipeline";
import { safeExtractJSON } from "../../src/utils/JsonExtractor";

// Mock logger to keep test output clean
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Mock JSON Extractor to parse naturally
vi.mock("../../src/utils/JsonExtractor", () => ({
    safeExtractJSON: vi.fn((text: string) => {
        try {
            return JSON.parse(text);
        } catch {
            return null;
        }
    }),
}));

function spinCPU(ms: number) {
    const start = Date.now();
    while (Date.now() - start < ms) {
        // Sync CPU spin
    }
}

describe("Consolidation Event Loop Lag", () => {
    let deps: StepDependencies;
    let ctx: ConsolidationContext;

    beforeEach(() => {
        vi.clearAllMocks();

        deps = {
            structuredMemory: {
                getUnconsolidatedEvents: vi.fn().mockResolvedValue([]),
                markConsolidated: vi.fn().mockResolvedValue(undefined),
                upsertVector: vi.fn().mockResolvedValue(undefined),
                setFact: vi.fn().mockResolvedValue(undefined),
                getAllFacts: vi.fn().mockReturnValue([]),
                getDb: vi.fn().mockReturnValue({
                    prepare: vi.fn().mockReturnValue({ all: vi.fn().mockReturnValue([]), run: vi.fn() }),
                    exec: vi.fn(),
                }),
                gcOldEvents: vi.fn().mockResolvedValue(undefined),
                processDLQ: vi.fn().mockResolvedValue(undefined),
                applyMemoryDecay: vi.fn().mockResolvedValue({ decayed: 0, archived: 0 }),
                createSnapshotBackup: vi.fn().mockResolvedValue(undefined),
                graph: {
                    upsertNode: vi.fn().mockResolvedValue(undefined),
                    upsertEdge: vi.fn().mockResolvedValue(undefined),
                    buildCommunitySummaries: vi.fn().mockResolvedValue(undefined),
                },
            } as any,
            embeddingService: {
                embed: vi.fn().mockImplementation(async () => {
                    // Simulate minor CPU work for embedding
                    spinCPU(5);
                    return new Float32Array(128);
                }),
            } as any,
            aiClient: {
                chat: {
                    completions: {
                        create: vi.fn().mockImplementation(async () => {
                            // Simulate significant CPU work for LLM parsing / processing
                            spinCPU(15);
                            return {
                                choices: [{
                                    message: {
                                        content: JSON.stringify({
                                            narrative_summary: "Summarized session narrative.",
                                            new_user_insights: [{ key: "pref.color", value: "blue", category: "personal" }],
                                            graph_nodes: [{ id: "user", label: "Person", properties: "{}" }],
                                            graph_edges: [{ source: "user", target: "liva", relation: "interacts" }]
                                        })
                                    }
                                }]
                            };
                        }),
                    },
                },
            } as any,
            bookIndex: {
                addNode: vi.fn(),
                addEdge: vi.fn(),
            } as any,
            contradictionResolver: {
                resolve: vi.fn().mockResolvedValue(undefined),
            } as any,
            reconsolidationEngine: null,
            synthesisPrompt: "Test synthesis",
        };

        ctx = {
            startedAt: Date.now(),
            totalConsolidated: 0,
            sharedState: {},
        };
    });

    it("should process consolidation sessions and keep p99 event loop lag under 50ms", async () => {
        // Setup multiple mock sessions
        const totalSessions = 10;
        const sessions = Array.from({ length: totalSessions }, (_, i) => ({
            events: [
                {
                    eventId: `evt_${i}_1`,
                    timestamp: Date.now() - 100000,
                    phi: { facts: ["user likes coding"] },
                    psi: { sentiment: "happy" },
                    rawUserMsg: "I love programming in Node.js",
                    rawAiReply: "That is great to hear!",
                    domain: "Development",
                    category: "General"
                }
            ],
            startTime: Date.now() - 100000,
            endTime: Date.now() - 95000,
        }));

        ctx.sharedState.sessions = sessions;

        // Start Event Loop delay monitor
        const detector = monitorEventLoopDelay({ resolution: 5 });
        detector.enable();

        // Temporarily override NODE_ENV to force setImmediate yielding path in AsyncChunker
        const originalEnv = process.env.NODE_ENV;
        process.env.NODE_ENV = "production";

        try {
            const step = new ProcessSessionsStep(deps);
            await step.execute(ctx);
        } finally {
            process.env.NODE_ENV = originalEnv;
            detector.disable();
        }

        // p99 latency in milliseconds
        const p99 = detector.percentile(99) / 1e6;
        
        // Assert that even under heavy simulated CPU processing, p99 Event Loop lag is under 50ms
        expect(p99).toBeLessThan(50);
    });
});
