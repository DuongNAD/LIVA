export interface IDBCountRow {
    c: number;
}

export interface IDBFactRow {
    key: string;
    value: string;
    createdAt: string;
    updatedAt: string;
    ttlDays: number | null;
    source: string | null;
    category: string | null;
    importance: number | null;
    confidenceScore: number | null;
    sourceTurnId: string | null;
    memory_strength: number | null;   // [UHM] Ebbinghaus decay (0.0-1.0)
    last_accessed_at: number | null;  // [UHM] Unix ms of last retrieval
    access_count: number | null;      // Touch/access count for spaced repetition
}

export interface IDBEventRow {
    eventId: string;
    timestamp: number;
    phi_facts: string;
    phi_entities: string;
    psi_sentiment: string;
    psi_intent: string;
    psi_relational: string;
    rawUserMsg: string;
    rawAiReply: string;
    consolidated: number;
    domain: string | null;
    category: string | null;
    trace_keywords: string | null;
    last_accessed_at: number | null;
}

export interface StructuredFact {
    key: string;
    value: string;
    createdAt: string;      // ISO timestamp
    updatedAt: string;      // ISO timestamp
    ttlDays?: number;       // Auto-expire after N days (null = permanent)
    source: string;         // Who created this fact (user, agent, system)
    category?: string;      // Optional categorization
    importance?: number;    // [v4.0] 0.0-1.0 ranking for eviction priority
    confidenceScore?: number; // [v4.0] Data lineage — extraction confidence
    sourceTurnId?: string;  // [v4.0] Data lineage — originating turn ID
    memoryStrength?: number;  // [UHM] Ebbinghaus decay (0.0-1.0)
    lastAccessedAt?: number;  // [UHM] Unix ms of last retrieval
    accessCount?: number;     // Touch/access count for spaced repetition
}
