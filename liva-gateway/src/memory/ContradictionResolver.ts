import { StructuredMemory } from "./StructuredMemory";
import { GraphRepository, L3Node, L3Edge } from "./GraphRepository";
import { EmbeddingService } from "../services/EmbeddingService";
import OpenAI from "openai";
import { logger } from "../utils/logger";
import { safeExtractJSON } from "../utils/JsonExtractor";

interface ContradictionCheckResult {
    status: "contradiction" | "supplement";
    obsolete_edges?: Array<{ source: string, target: string, relation: string }>;
}

/**
 * ContradictionResolver — L3 Knowledge Graph Integrity Engine
 * =========================================================
 * Uses Vector Search (L2) and LLM Reasoning (L3) to detect and resolve
 * logical contradictions in the Dynamic Knowledge Graph when new nodes/edges are added.
 */
export class ContradictionResolver {
    private structuredMemory: StructuredMemory;
    private embeddingService: EmbeddingService;
    private aiClient: OpenAI;

    constructor(
        structuredMemory: StructuredMemory,
        embeddingService: EmbeddingService,
        aiClient: OpenAI
    ) {
        this.structuredMemory = structuredMemory;
        this.embeddingService = embeddingService;
        this.aiClient = aiClient;
    }

    /**
     * Checks if a new piece of knowledge (edge/node) contradicts existing L3 graph edges.
     * Uses vector similarity to find overlapping context, then LLM to verify contradiction.
     */
    public async resolve(newEdge: L3Edge, sourceNode: L3Node, targetNode: L3Node): Promise<void> {
        try {
            // 1. Create a natural language statement of the new fact
            const newFactText = `${sourceNode.label} ${sourceNode.id} ${newEdge.relation} ${targetNode.label} ${targetNode.id}`;
            const newFactVec = await this.embeddingService.embed(newFactText);

            // 2. Search for similar existing facts in L2 (AXIOMs or ANCHORs) to find candidates
            const similarFacts = this.structuredMemory.searchSimilarVectors(newFactVec, 5);
            
            // Filter candidates with cosine similarity > 0.85
            const highSimilarityCandidates = similarFacts.filter(f => f.score > 0.85 && f.content !== newFactText);

            if (highSimilarityCandidates.length === 0) {
                // No highly similar facts found, no contradiction likely
                return;
            }

            // 3. Fetch active edges related to the source node
            const existingEdges = this.structuredMemory.graph.getActiveEdgesBySource(newEdge.source);
            if (existingEdges.length === 0) return;

            // 4. Pass the new fact and existing edges to LLM to detect contradiction
            const existingEdgesStr = existingEdges.map(e => `- ${e.source} [${e.relation}] ${e.target}`).join('\n');

            const prompt = `Bạn là một Contradiction Resolver (Bộ giải quyết mâu thuẫn) của hệ thống AI.
Nhiệm vụ: Phân tích hai dữ kiện (Fact A và Fact B) xem chúng có triệt tiêu (mâu thuẫn trực tiếp) nhau hay không.

Fact A (Cũ):
${existingEdgesStr}

Fact B (Mới):
${newFactText}

Yêu cầu bắt buộc: Chỉ trả về định dạng JSON duy nhất, tuyệt đối không giải thích thêm:
{"status": "contradiction", "obsolete_edges": [{"source": "ID", "target": "ID", "relation": "RELATION"}]} hoặc {"status": "supplement"}
`;

            const response = await this.aiClient.chat.completions.create({
                model: "router", // Use fast, lightweight model (Gemma/Mini)
                messages: [{ role: "system", content: prompt }],
                temperature: 0.1,
                max_tokens: 300,
            });

            const raw = response.choices[0]?.message?.content?.trim();
            if (!raw) return;

            const result = safeExtractJSON<ContradictionCheckResult>(raw);
            if (result && result.status === "contradiction" && result.obsolete_edges) {
                logger.warn(`[ContradictionResolver] 🛑 Contradiction detected!`);
                
                // 5. Mark contradicted edges as obsolete in GraphRepository
                for (const obsEdge of result.obsolete_edges) {
                    this.structuredMemory.graph.markEdgeObsolete(obsEdge.source, obsEdge.target, obsEdge.relation);
                    logger.info(`[ContradictionResolver] Marked edge obsolete: ${obsEdge.source} -> ${obsEdge.target} [${obsEdge.relation}]`);
                }
            }

        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[ContradictionResolver] Failed to resolve contradiction: ${errMsg}`);
        }
    }
}
