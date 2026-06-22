import { DatabaseSync } from "node:sqlite";
import { PersonalityEvolution } from "../PersonalityEvolution";

export function initStore(db: DatabaseSync, agentId: string): void {
    db.exec("PRAGMA journal_mode = WAL");
    db.exec("PRAGMA synchronous = NORMAL");
    db.exec("PRAGMA busy_timeout = 5000");
    db.exec("PRAGMA wal_autocheckpoint = 500");
    db.exec("PRAGMA cache_size = -8192");
    
    db.exec("PRAGMA page_size = 32768");
    db.exec("PRAGMA mmap_size = 268435456");

    db.exec(`
        CREATE TABLE IF NOT EXISTS facts (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            createdAt TEXT NOT NULL,
            updatedAt TEXT NOT NULL,
            ttlDays INTEGER,
            source TEXT NOT NULL,
            category TEXT,
            importance REAL DEFAULT 0.5,
            confidenceScore REAL DEFAULT 1.0,
            sourceTurnId TEXT
        )
    `);
    const factsColumns = db.prepare("PRAGMA table_info(facts)").all() as Array<{name: string}>;
    const factsColNames = new Set(factsColumns.map(c => c.name));
    if (!factsColNames.has('importance')) {
        db.exec("ALTER TABLE facts ADD COLUMN importance REAL DEFAULT 0.5");
    }
    if (!factsColNames.has('confidenceScore')) {
        db.exec("ALTER TABLE facts ADD COLUMN confidenceScore REAL DEFAULT 1.0");
    }
    if (!factsColNames.has('sourceTurnId')) {
        db.exec("ALTER TABLE facts ADD COLUMN sourceTurnId TEXT");
    }
    if (!factsColNames.has('memory_strength')) {
        db.exec("ALTER TABLE facts ADD COLUMN memory_strength REAL DEFAULT 1.0");
    }
    if (!factsColNames.has('last_accessed_at')) {
        db.exec("ALTER TABLE facts ADD COLUMN last_accessed_at INTEGER DEFAULT 0");
    }
    if (!factsColNames.has('access_count')) {
        db.exec("ALTER TABLE facts ADD COLUMN access_count INTEGER DEFAULT 0");
    }

    db.exec(`
        CREATE TABLE IF NOT EXISTS events (
            eventId TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL,
            phi_facts TEXT,
            phi_entities TEXT,
            psi_sentiment TEXT,
            psi_intent TEXT,
            psi_relational TEXT,
            rawUserMsg TEXT,
            rawAiReply TEXT,
            consolidated INTEGER DEFAULT 0,
            agentId TEXT DEFAULT '${agentId}'
        )
    `);

    const columns = db.prepare("PRAGMA table_info(events)").all() as Array<{name: string}>;
    const colNames = new Set(columns.map(c => c.name));
    if (!colNames.has('domain')) {
        db.exec("ALTER TABLE events ADD COLUMN domain TEXT DEFAULT 'General'");
    }
    if (!colNames.has('category')) {
        db.exec("ALTER TABLE events ADD COLUMN category TEXT DEFAULT 'Uncategorized'");
    }
    if (!colNames.has('trace_keywords')) {
        db.exec("ALTER TABLE events ADD COLUMN trace_keywords TEXT");
    }
    if (!colNames.has('last_accessed_at')) {
        db.exec("ALTER TABLE events ADD COLUMN last_accessed_at INTEGER DEFAULT 0");
    }
    if (!colNames.has('consolidation_status')) {
        db.exec("ALTER TABLE events ADD COLUMN consolidation_status TEXT DEFAULT 'pending'");
    }
    if (!colNames.has('retry_count')) {
        db.exec("ALTER TABLE events ADD COLUMN retry_count INTEGER DEFAULT 0");
    }
    if (!colNames.has('agentId')) {
        db.exec("ALTER TABLE events ADD COLUMN agentId TEXT DEFAULT 'liva_core'");
    }
    db.exec("CREATE INDEX IF NOT EXISTS idx_events_pending ON events(eventId) WHERE consolidation_status = 'pending'");
    db.exec("CREATE INDEX IF NOT EXISTS idx_events_consolidated_ts ON events(consolidated, timestamp) WHERE consolidation_status = 'pending'");

    db.exec("DROP TABLE IF EXISTS lance_dlq");

    db.exec(`
        CREATE TABLE IF NOT EXISTS vector_dlq (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            delete_filter TEXT NOT NULL,
            status TEXT DEFAULT 'pending',
            retry_count INTEGER DEFAULT 0
        )
    `);

    db.exec(`
        CREATE TABLE IF NOT EXISTS turn_layer_nodes (
            turnId TEXT PRIMARY KEY,
            temporal_anchor INTEGER NOT NULL,
            userMsg TEXT,
            aiReply TEXT,
            createdAt TEXT NOT NULL,
            agentId TEXT DEFAULT '${agentId}'
        )
    `);
    db.exec("CREATE INDEX IF NOT EXISTS idx_turns_temporal ON turn_layer_nodes(temporal_anchor)");

    try {
        const turnCols = db.prepare("PRAGMA table_info(turn_layer_nodes)").all() as Array<{name: string}>;
        const turnColNames = new Set(turnCols.map(c => c.name));
        if (!turnColNames.has('agentId')) {
            db.exec("ALTER TABLE turn_layer_nodes ADD COLUMN agentId TEXT DEFAULT 'liva_core'");
        }
    } catch { /* ignore */ }

    db.exec(`
        CREATE TABLE IF NOT EXISTS daily_briefings (
            id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            topics TEXT NOT NULL,
            content TEXT NOT NULL,
            is_read INTEGER DEFAULT 0,
            source TEXT DEFAULT 'tavily',
            expires_at INTEGER NOT NULL
        )
    `);
    db.exec(`DELETE FROM daily_briefings WHERE expires_at < ${Date.now()}`);

    db.exec(`
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT DEFAULT '',
            status TEXT DEFAULT 'pending',
            priority TEXT DEFAULT 'medium',
            result TEXT DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )
    `);

    db.exec(`
        CREATE TABLE IF NOT EXISTS consolidation_checkpoints (
            session_id TEXT PRIMARY KEY,
            last_step INTEGER DEFAULT 0,
            state_data TEXT DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )
    `);

    db.exec(`
        CREATE TABLE IF NOT EXISTS dlq_consolidation (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            failed_step TEXT NOT NULL,
            error_msg TEXT,
            retry_count INTEGER DEFAULT 0,
            status TEXT DEFAULT 'pending',
            created_at INTEGER NOT NULL
        )
    `);

    PersonalityEvolution.initialize(db);
    PersonalityEvolution.getPersonalityState(db, agentId);
}
