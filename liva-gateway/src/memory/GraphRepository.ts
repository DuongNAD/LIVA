import { DatabaseSync } from "node:sqlite";
import { logger } from "../utils/logger";

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
 * Manages L3 Dynamic Knowledge Graph operations in SQLite.
 */
export class GraphRepository {
    readonly #db: DatabaseSync;

    constructor(db: DatabaseSync) {
        this.#db = db;
    }

    public init(): void {
        try {
            this.#db.exec(`
                CREATE TABLE IF NOT EXISTS l3_nodes (
                    id TEXT PRIMARY KEY,
                    label TEXT NOT NULL,
                    properties TEXT DEFAULT '{}'
                )
            `);

            this.#db.exec(`
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
            try { this.#db.exec("ALTER TABLE l3_edges ADD COLUMN obsolete INTEGER DEFAULT 0"); } catch { /* already exists */ }

            logger.info("[StructuredMemory/Graph] ✅ Graph tables initialized.");
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[StructuredMemory/Graph] ❌ Failed to initialize Graph tables: ${errMsg}`);
        }
    }

    public upsertNode(node: L3Node): void {
        try {
            this.#db.prepare(`
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

    public upsertEdge(edge: L3Edge): void {
        try {
            this.#db.prepare(`
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

    public markEdgeObsolete(source: string, target: string, relation: string): void {
        try {
            this.#db.prepare(`
                UPDATE l3_edges
                SET obsolete = 1
                WHERE source = ? AND target = ? AND relation = ?
            `).run(source, target, relation);
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error marking edge obsolete ${source}->${target}: ${e}`);
        }
    }

    public getActiveEdgesBySource(sourceId: string): L3Edge[] {
        try {
            return this.#db.prepare(`
                SELECT source, target, relation, weight, obsolete
                FROM l3_edges
                WHERE source = ? AND obsolete = 0
            `).all(sourceId) as unknown as L3Edge[];
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error getting edges for source ${sourceId}: ${e}`);
            return [];
        }
    }

    public getAllActiveEdges(): L3Edge[] {
        try {
            return this.#db.prepare(`
                SELECT source, target, relation, weight, obsolete
                FROM l3_edges
                WHERE obsolete = 0
            `).all() as unknown as L3Edge[];
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error getting all active edges: ${e}`);
            return [];
        }
    }

    /**
     * Thực hiện truy vấn Đồ thị Đa bước (Multi-hop Traversal) sử dụng Recursive CTE.
     * Tìm tất cả các Node có thể chạm tới từ startNodeId trong giới hạn maxDepth.
     */
    public multiHopSearch(startNodeId: string, maxDepth: number = 3): any[] {
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
            return this.#db.prepare(query).all(startNodeId, maxDepth) as any[];
        } catch (e: unknown) {
            logger.error(`[GraphRepository] Error in multiHopSearch: ${e}`);
            return [];
        }
    }
}
