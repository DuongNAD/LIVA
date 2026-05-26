import { DatabaseWorkerBridge } from "./DatabaseWorkerBridge";
import { logger } from "../utils/logger";
import OpenAI from "openai";
import { EmbeddingService } from "../services/EmbeddingService";

export interface L3Node {
    id: string;          // e.g., "User", "ProjectX", "LIVA"
    label: string;       // e.g., "PERSON", "PROJECT", "SYSTEM"
    properties: string;  // JSON string of properties
}

export interface L3Edge {
    source: string;      // Node ID
    target: string;      // Node ID
    relation: string;    // e.g., "LIKES", "WORKING_ON"
    weight: number;      // Confidence/strength (0.0 to 1.0)
    obsolete: number;    // 0 = active, 1 = outdated/contradicted
}

/**
 * GraphRepository
 * Manages L3 Dynamic Knowledge Graph operations in SQLite asynchronously via DatabaseWorker.
 */
export class GraphRepository {
    readonly #db: DatabaseWorkerBridge;

    constructor(db: DatabaseWorkerBridge) {
        this.#db = db;
    }

    public async init(): Promise<void> {
        try {
            await this.#db.exec(`
                CREATE TABLE IF NOT EXISTS l3_nodes (
                    id TEXT PRIMARY KEY,
                    label TEXT NOT NULL,
                    properties TEXT DEFAULT '{}'
                )
            `);

            await this.#db.exec(`
                CREATE TABLE IF NOT EXISTS l3_edges (
                    source TEXT NOT NULL,
                    target TEXT NOT NULL,
                    relation TEXT NOT NULL,
                    weight REAL DEFAULT 1.0,
                    obsolete INTEGER DEFAULT 0,
                    PRIMARY KEY (source, target, relation),
                    FOREIGN KEY(source) REFERENCES l3_nodes(id),
                    FOREIGN KEY(target) REFERENCES l3_nodes(id)
                )
            `);

            // Safe migration for obsolete column
            try { await this.#db.exec("ALTER TABLE l3_edges ADD COLUMN obsolete INTEGER DEFAULT 0"); } catch { /* already exists */ }

            logger.info("[StructuredMemory/Graph] ✅ Graph tables initialized.");
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[StructuredMemory/Graph] ❌ Failed to initialize Graph tables: ${errMsg}`);
        }
    }

    public async upsertNode(node: L3Node): Promise<void> {
        try {
            await this.#db.prepare(`
                INSERT INTO l3_nodes (id, label, properties)
                VALUES (?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    label = excluded.label,
                    properties = excluded.properties
            `).run(node.id, node.label, node.properties);
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error upserting node ${node.id}: ${e}`);
        }
    }

    public async upsertEdge(edge: L3Edge): Promise<void> {
        try {
            await this.#db.prepare(`
                INSERT INTO l3_edges (source, target, relation, weight, obsolete)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(source, target, relation) DO UPDATE SET
                    weight = excluded.weight,
                    obsolete = excluded.obsolete
            `).run(edge.source, edge.target, edge.relation, edge.weight, edge.obsolete);
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error upserting edge ${edge.source}->${edge.target}: ${e}`);
        }
    }

    public async markEdgeObsolete(source: string, target: string, relation: string): Promise<void> {
        try {
            await this.#db.prepare(`
                UPDATE l3_edges
                SET obsolete = 1
                WHERE source = ? AND target = ? AND relation = ?
            `).run(source, target, relation);
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error marking edge obsolete ${source}->${target}: ${e}`);
        }
    }

    public async getActiveEdgesBySource(sourceId: string): Promise<L3Edge[]> {
        try {
            return await this.#db.prepare(`
                SELECT source, target, relation, weight, obsolete
                FROM l3_edges
                WHERE source = ? AND obsolete = 0
            `).all(sourceId) as unknown as L3Edge[];
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error getting edges for source ${sourceId}: ${e}`);
            return [];
        }
    }

    public async getAllActiveEdges(): Promise<L3Edge[]> {
        try {
            return await this.#db.prepare(`
                SELECT source, target, relation, weight, obsolete
                FROM l3_edges
                WHERE obsolete = 0
            `).all() as unknown as L3Edge[];
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error getting all active edges: ${e}`);
            return [];
        }
    }

    public async getAllActiveNodes(): Promise<L3Node[]> {
        try {
            return await this.#db.prepare(`
                SELECT id, label, properties
                FROM l3_nodes
            `).all() as unknown as L3Node[];
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error getting all active nodes: ${e}`);
            return [];
        }
    }

    /**
     * Group L3 graph nodes into communities using Label Propagation,
     * generate community summaries via a lightweight LLM, and embed/upsert them.
     */
    public async buildCommunitySummaries(
        aiClient: OpenAI, 
        embedding: EmbeddingService,
        upsertVector: (record: any) => void
    ): Promise<void> {
        logger.info("[GraphRepository] Starting GraphRAG community detection and summarization...");

        // 1. Load active nodes and edges
        const [nodes, edges] = await Promise.all([
            this.getAllActiveNodes(),
            this.getAllActiveEdges()
        ]);

        if (nodes.length === 0) {
            logger.info("[GraphRepository] No nodes found in the graph. Skipping community detection.");
            return;
        }

        // 2. Build adjacency mapping (undirected for community propagation)
        const adj = new Map<string, Map<string, number>>();
        for (const edge of edges) {
            if (edge.obsolete === 1) continue;
            
            if (!adj.has(edge.source)) adj.set(edge.source, new Map());
            if (!adj.has(edge.target)) adj.set(edge.target, new Map());

            adj.get(edge.source)!.set(edge.target, edge.weight);
            adj.get(edge.target)!.set(edge.source, edge.weight);
        }

        // 3. Label Propagation Algorithm (5 iterations)
        const labels = new Map<string, string>();
        for (const node of nodes) {
            labels.set(node.id, node.id); // initial label: self
        }

        const iterations = 5;
        for (let iter = 0; iter < iterations; iter++) {
            // [v27 Tech Debt] Deterministic shuffle using Mulberry32 PRNG seeded from iteration
            // Ensures identical community assignments across runs for the same graph
            const seedStr = `graph_community_v1_iter${iter}_${nodes.map(n => n.id).join(',')}`;
            const rng = GraphRepository.#createMulberry32(seedStr);
            const shuffledNodes = [...nodes].sort(() => rng() - 0.5);

            for (const node of shuffledNodes) {
                const u = node.id;
                const neighbors = adj.get(u);
                if (!neighbors || neighbors.size === 0) continue;

                const labelWeights = new Map<string, number>();
                // Group neighbors' labels
                for (const [v, weight] of neighbors.entries()) {
                    const vLabel = labels.get(v)!;
                    labelWeights.set(vLabel, (labelWeights.get(vLabel) || 0) + weight);
                }

                let maxLabel = labels.get(u)!;
                let maxWeight = -1;
                for (const [lbl, wt] of labelWeights.entries()) {
                    if (wt > maxWeight) {
                        maxWeight = wt;
                        maxLabel = lbl;
                    }
                }
                labels.set(u, maxLabel);
            }
        }

        // 4. Group nodes by label
        const communities = new Map<string, { nodes: L3Node[]; edges: L3Edge[] }>();
        for (const node of nodes) {
            const lbl = labels.get(node.id)!;
            if (!communities.has(lbl)) {
                communities.set(lbl, { nodes: [], edges: [] });
            }
            communities.get(lbl)!.nodes.push(node);
        }

        // Map active edges to communities (only if both endpoints share the same community label)
        for (const edge of edges) {
            if (edge.obsolete === 1) continue;
            const srcLabel = labels.get(edge.source);
            const tgtLabel = labels.get(edge.target);
            if (srcLabel && srcLabel === tgtLabel) {
                communities.get(srcLabel)!.edges.push(edge);
            }
        }

        // 5. Filter communities (size >= 2)
        const validCommunities = Array.from(communities.entries())
            .filter(([_, data]) => data.nodes.length >= 2)
            .map(([label, data]) => ({ label, ...data }));

        logger.info(`[GraphRepository] Grouped graph into ${validCommunities.length} valid communities (size >= 2).`);

        // 6. Summarize communities with concurrency control (batch size of 3)
        const batchSize = 3;
        const summarizeCommunity = async (comm: { label: string; nodes: L3Node[]; edges: L3Edge[] }) => {
            const { label, nodes: commNodes, edges: commEdges } = comm;
            
            const nodesList = commNodes.map(n => `- Node: ${n.id} (${n.label}), Properties: ${n.properties}`).join("\n");
            const edgesList = commEdges.map(e => `- Relationship: ${e.source} -[${e.relation}]-> ${e.target} (weight: ${e.weight})`).join("\n");

            const prompt = `Generate a high-level community summary for the following sub-graph of user concepts.
Summarize the main themes, key relationships, and core activities in 2-3 sentences.
Return ONLY the summary text in English or Vietnamese depending on context, preserving Vietnamese names.

[NODES]
${nodesList}

[RELATIONSHIPS]
${edgesList}`;

            try {
                const response = await aiClient.chat.completions.create({
                    model: "router",
                    messages: [
                        { role: "system", content: "You are the LIVA GraphRAG Community Summarizer." },
                        { role: "user", content: prompt }
                    ],
                    temperature: 0.2,
                    max_tokens: 300
                });

                const summaryText = response.choices[0]?.message?.content?.trim();
                if (summaryText) {
                    const summaryWithHeader = `[Community Summary - ${label}]: ${summaryText}`;
                    const vector = await embedding.embed(summaryText);
                    
                    upsertVector({
                        vecId: `community_${label}_${Date.now()}`,
                        type: 'ANCHOR',
                        content: summaryWithHeader,
                        vector,
                        domain: 'Community',
                        category: 'CommunitySummary'
                    });
                }
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.error(`[GraphRepository] Failed to summarize community ${label}: ${msg}`);
            }
        };

        for (let i = 0; i < validCommunities.length; i += batchSize) {
            const batch = validCommunities.slice(i, i + batchSize);
            await Promise.all(batch.map(c => summarizeCommunity(c)));
        }

        logger.info("[GraphRepository] GraphRAG community summarization complete.");
    }

    /**
     * Thực hiện truy vấn Đồ thị Đa bước (Multi-hop Traversal) sử dụng Recursive CTE.
     * Tìm tất cả các Node có thể chạm tới từ startNodeId trong giới hạn maxDepth.
     */
    public async multiHopSearch(startNodeId: string, maxDepth: number = 3): Promise<any[]> {
        try {
            const query = `
                WITH RECURSIVE traverse(source, target, relation, depth) AS (
                    SELECT source, target, relation, 1
                    FROM l3_edges
                    WHERE source = ? AND obsolete = 0
                    
                    UNION
                    
                    SELECT e.source, e.target, e.relation, t.depth + 1
                    FROM l3_edges e
                    JOIN traverse t ON e.source = t.target
                    WHERE e.obsolete = 0 AND t.depth < ?
                )
                SELECT * FROM traverse;
            `;
            return await this.#db.prepare(query).all(startNodeId, maxDepth) as any[];
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error in multiHopSearch: ${e}`);
            return [];
        }
    }

    /**
     * [v27 Tech Debt] Mulberry32 — Fast, deterministic PRNG for Label Propagation.
     * Converts a string seed into a numeric hash (FNV-1a variant), then generates
     * uniformly distributed pseudo-random numbers in [0, 1).
     * Guarantees identical community assignments across runs for the same graph.
     */
    static #createMulberry32(seedStr: string): () => number {
        // FNV-1a-inspired hash to convert string seed to numeric
        let h = 0xdeadbeef;
        for (let i = 0; i < seedStr.length; i++) {
            h = Math.imul(h ^ seedStr.charCodeAt(i), 2654435761);
        }
        let seed = ((h ^ h >>> 16) >>> 0);

        // Mulberry32 generator — returns [0, 1)
        return function () {
            let t = seed += 0x6D2B79F5;
            t = Math.imul(t ^ t >>> 15, t | 1);
            t ^= t + Math.imul(t ^ t >>> 7, t | 61);
            return ((t ^ t >>> 14) >>> 0) / 4294967296;
        };
    }
}
