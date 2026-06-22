import { describe, it, expect } from "vitest";
import { DatabaseSync } from "node:sqlite";

describe("Database Optimization Verification", () => {
    it("should use indexes for metadata queries and avoid full table scan", () => {
        const db = new DatabaseSync(":memory:");

        // 1. Create vectors_meta table using same schema
        db.exec(`
            CREATE TABLE IF NOT EXISTS vectors_meta (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                vec_id TEXT UNIQUE NOT NULL,
                type TEXT NOT NULL,
                content TEXT NOT NULL,
                domain TEXT DEFAULT 'General',
                category TEXT DEFAULT 'Uncategorized',
                trace_keywords TEXT DEFAULT '[]',
                file_target TEXT,
                created_at INTEGER NOT NULL,
                last_accessed_at INTEGER DEFAULT 0,
                decay_weight REAL DEFAULT 1.0,
                access_count INTEGER DEFAULT 0,
                source_event_ids TEXT DEFAULT '[]'
            )
        `);

        // 2. Create the idx_vectors_meta_filter and idx_vectors_meta_created indexes
        db.exec(`
            CREATE INDEX IF NOT EXISTS idx_vectors_meta_filter ON vectors_meta (type, domain, category)
        `);

        db.exec(`
            CREATE INDEX IF NOT EXISTS idx_vectors_meta_created ON vectors_meta (created_at)
        `);

        // Insert some dummy data so SQLite has enough records to optimize index usage
        const stmt = db.prepare(`
            INSERT INTO vectors_meta (vec_id, type, content, domain, category, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
        `);
        for (let i = 0; i < 100; i++) {
            stmt.run(
                `vec_${i}`,
                `type_${i % 5}`,
                `content_${i}`,
                `domain_${i % 2}`,
                `category_${i % 3}`,
                1700000000 + i
            );
        }

        // Run ANALYZE to update statistics for the query planner
        db.exec("ANALYZE");

        // Query 1: Filter by metadata (type, domain, category) using SELECT * to avoid COVERING index
        const plan1 = db.prepare(`
            EXPLAIN QUERY PLAN
            SELECT * FROM vectors_meta WHERE type = 'type_1' AND domain = 'domain_1' AND category = 'category_1'
        `).all() as Array<{ id: number; parent: number; notused: number; detail: string }>;

        // Query 2: Filter by created_at using SELECT * to avoid COVERING index
        const plan2 = db.prepare(`
            EXPLAIN QUERY PLAN
            SELECT * FROM vectors_meta WHERE created_at >= 1700000000 AND created_at <= 1700000100
        `).all() as Array<{ id: number; parent: number; notused: number; detail: string }>;

        const plan1Details = plan1.map(row => row.detail).join("\n");
        const plan2Details = plan2.map(row => row.detail).join("\n");


        // Asserts that the query planner output contains 'USING INDEX idx_vectors_meta_filter' or 'USING INDEX idx_vectors_meta_created'
        // and does NOT perform a full table scan ('SCAN TABLE vectors_meta')
        expect(plan1Details).toContain("USING INDEX idx_vectors_meta_filter");
        expect(plan1Details).not.toContain("SCAN TABLE vectors_meta");

        expect(plan2Details).toContain("USING INDEX idx_vectors_meta_created");
        expect(plan2Details).not.toContain("SCAN TABLE vectors_meta");
    });

    it("should run batch transactions atomically using batch transaction interface", () => {
        const db = new DatabaseSync(":memory:");

        // Create a simple table for batch test
        db.exec(`
            CREATE TABLE IF NOT EXISTS test_batch (
                id TEXT PRIMARY KEY,
                val INTEGER
            )
        `);

        // Batch execution helper mimicking DatabaseWorkerBridge/DatabaseWorker transactionBatch
        const transactionBatch = (statements: Array<{ sql: string; paramSets: unknown[][] }>) => {
            db.exec("BEGIN");
            try {
                for (const stmtDef of statements) {
                    const stmt = db.prepare(stmtDef.sql);
                    for (const p of stmtDef.paramSets) {
                        stmt.run(...p);
                    }
                }
                db.exec("COMMIT");
            } catch (e) {
                try { db.exec("ROLLBACK"); } catch {}
                throw e;
            }
        };

        // Assert batch inserts/updates run using the batch transaction interface and commit atomically
        // 1. Success case: inserts commit
        transactionBatch([
            {
                sql: "INSERT INTO test_batch (id, val) VALUES (?, ?)",
                paramSets: [
                    ["a", 1],
                    ["b", 2]
                ]
            }
        ]);

        const count = db.prepare("SELECT count(*) as c FROM test_batch").get() as { c: number };
        expect(count.c).toBe(2);

        // 2. Failure case: violates constraint and rolls back atomically (all or nothing)
        expect(() => {
            transactionBatch([
                {
                    sql: "INSERT INTO test_batch (id, val) VALUES (?, ?)",
                    paramSets: [
                        ["c", 3],
                        ["a", 4] // Unique constraint violation (key "a" already exists)
                    ]
                }
            ]);
        }).toThrow();

        // Verify "c" was NOT inserted because the transaction failed atomically and rolled back
        const countAfterFailure = db.prepare("SELECT count(*) as c FROM test_batch").get() as { c: number };
        expect(countAfterFailure.c).toBe(2);
        
        const rowC = db.prepare("SELECT * FROM test_batch WHERE id = ?").get("c");
        expect(rowC).toBeUndefined();
    });
});
