# Rust Native Core Integration: enterprise-doc-rag-auditor

## 1. Native Engine Architecture Alignment

### 1.1 Module Domain & Routing
- **Native Modules**: `liva-native-core/src/commands/rag.rs`, `liva-native-core/src/db.rs`, `liva-native-core/src/mcp/server.rs`, `liva-native-core/src/llm/tool_calling.rs`.
- **Command Routing Matrix**:
  - `rag:ingest_doc`: Ingests and processes multi-page documents (PDF, DOCX, scanned images) through layout analysis and chunking pipelines. Gated under `ExecPolicy::ProposeOnly`.
  - `rag:query_hybrid`: Executes hybrid dual-index retrieval (sqlite-vec vector KNN + SQLite FTS5 sparse BM25 with Reciprocal Rank Fusion). Gated under `ExecPolicy::Auto`.
  - `rag:extract_tables`: Extracts structured tabular structures and financial figures into JSON. Gated under `ExecPolicy::Auto`.
  - `rag:redline_diff`: Compares raw contract clauses against standard statutory baselines and generates semantic diffs. Gated under `ExecPolicy::Auto`.
  - `write_markdown`: Persists the contract audit dossier into `vault/Knowledge/Contract Audit - <Title>.md`. Gated under `ExecPolicy::ProposeOnly`.

### 1.2 Execution Policy (`ExecPolicy`)
```rust
// In liva-native-core/src/llm/tool_calling.rs
pub fn for_tool(server: &str, name: &str) -> Self {
    match (server, name) {
        ("rag", "query_hybrid") | ("rag", "extract_tables") | ("rag", "redline_diff") => Self::Auto,
        ("rag", "ingest_doc") => Self::ProposeOnly,
        ("obsidian", "write_markdown") => Self::ProposeOnly,
        _ => Self::ProposeOnly,
    }
}
```

---

## 2. Tool Schema Mapping & Compact Prompt Rendering

### 2.1 Compact Tool Signatures
```
[1] rag:query_hybrid: Query indexed enterprise documents using hybrid vector + FTS5 search
   tham số (* = bắt buộc): query* (string), top_k (integer), doc_id (string), threshold (number)
[2] rag:ingest_doc: Parse, chunk, and index enterprise document into vector & FTS stores
   tham số (* = bắt buộc): file_path* (string), doc_type (string: contract|financial|manual), chunk_size (integer)
[3] rag:redline_diff: Compare contract clause against baseline and calculate risk score
   tham số (* = bắt buộc): target_clause* (string), clause_type* (string: liability|indemnity|termination|ip|data_privacy), baseline_id (string)
```

### 2.2 Input Schema & Validation Matrix
```json
{
  "rag:query_hybrid": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "minLength": 1, "description": "Search query or legal concept" },
      "top_k": { "type": "integer", "minimum": 1, "maximum": 50, "default": 10 },
      "doc_id": { "type": "string", "description": "Optional document filter" },
      "threshold": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": 0.65 }
    },
    "required": ["query"]
  },
  "rag:ingest_doc": {
    "type": "object",
    "properties": {
      "file_path": { "type": "string", "minLength": 1, "description": "Relative path to target document" },
      "doc_type": { "type": "string", "enum": ["contract", "financial", "manual", "general"], "default": "general" },
      "chunk_size": { "type": "integer", "minimum": 128, "maximum": 2048, "default": 512 }
    },
    "required": ["file_path"]
  },
  "rag:redline_diff": {
    "type": "object",
    "properties": {
      "target_clause": { "type": "string", "minLength": 10, "description": "Raw clause text to evaluate" },
      "clause_type": { 
        "type": "string", 
        "enum": ["liability", "indemnity", "termination", "ip", "data_privacy", "general"],
        "description": "Category of legal clause" 
      },
      "baseline_id": { "type": "string", "description": "Identifier for reference playbook" }
    },
    "required": ["target_clause", "clause_type"]
  }
}
```

---

## 3. State Persistence & SQLite WAL Schema

### 3.1 Dual-Engine Indexing Storage Layout
The RAG pipeline stores structural metadata and vector embeddings in the local SQLite database using WAL mode and `sqlite-vec`:

```sql
-- Document registry
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    file_path TEXT NOT NULL UNIQUE,
    file_hash TEXT NOT NULL,
    page_count INTEGER NOT NULL,
    doc_type TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Hierarchical chunks
CREATE TABLE IF NOT EXISTS document_chunks (
    id TEXT PRIMARY KEY,
    doc_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    parent_chunk_id TEXT,
    page_number INTEGER NOT NULL,
    clause_anchor TEXT,
    content TEXT NOT NULL,
    token_count INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Full-text keyword search index (FTS5)
CREATE VIRTUAL TABLE IF NOT EXISTS document_fts USING fts5(
    chunk_id UNINDEXED,
    content,
    tokenize = 'porter unicode61'
);

-- Vector embeddings (sqlite-vec extension)
CREATE VIRTUAL TABLE IF NOT EXISTS document_chunks_vec USING vec0(
    chunk_id TEXT PRIMARY KEY,
    embedding FLOAT[384] -- multilingual-e5-small dimension
);
```

### 3.2 Reciprocal Rank Fusion (RRF) Implementation
```rust
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub chunk_id: String,
    pub doc_id: String,
    pub page_number: usize,
    pub content: String,
    pub rrf_score: f64,
}

pub fn fuse_rrf(vector_hits: Vec<(String, usize)>, fts_hits: Vec<(String, usize)>) -> Vec<(String, f64)> {
    use std::collections::HashMap;
    const K: f64 = 60.0;
    let mut scores: HashMap<String, f64> = HashMap::new();

    for (id, rank) in vector_hits {
        *scores.entry(id).or_default() += 1.0 / (K + rank as f64);
    }
    for (id, rank) in fts_hits {
        *scores.entry(id).or_default() += 1.0 / (K + rank as f64);
    }

    let mut ranked: Vec<(String, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}
```

---

## 4. Fail-Closed Security & Sandboxing Constraints

### 4.1 Strict Filesystem Sandbox
- Documents must be resolved via `mcp::server::resolve_path`. Any attempts to access paths outside permitted directories or containing directory traversal tokens (`..`, absolute root paths, invalid junctions) return a fail-closed `CallToolResult::error`.

### 4.2 Principal RBAC Authorization
- Principal `CommandPrincipal::TauriDashboard` and `CommandPrincipal::WebSocketDashboard`: Authorized for document ingestion and hybrid querying.
- Principal `CommandPrincipal::Telegram`: Authorized only for high-level summaries (`rag:query_hybrid` with strict result truncation).

### 4.3 Clause Risk Scoring Engine
```rust
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum ClauseRisk {
    Low,
    Medium,
    High,
    Critical,
}

pub fn evaluate_clause_risk(clause_type: &str, content: &str) -> ClauseRisk {
    let lower = content.to_lowercase();
    match clause_type {
        "liability" => {
            if lower.contains("uncapped") || lower.contains("without limitation") {
                ClauseRisk::Critical
            } else if lower.contains("sole discretion") || !lower.contains("aggregate liability") {
                ClauseRisk::High
            } else {
                ClauseRisk::Medium
            }
        }
        "indemnity" => {
            if lower.contains("unilateral") || (lower.contains("customer shall indemnify") && !lower.contains("vendor shall indemnify")) {
                ClauseRisk::High
            } else {
                ClauseRisk::Low
            }
        }
        "termination" => {
            if lower.contains("immediate termination without cause") {
                ClauseRisk::High
            } else {
                ClauseRisk::Low
            }
        }
        _ => ClauseRisk::Low,
    }
}
```

---

## 5. Verification Checklist & Unit Test Scenarios

### 5.1 Verification Checklist
- [x] Frontmatter in `SKILL.md` conforms strictly to `name` and `description` only.
- [x] Frontmatter in generated vault notes conforms strictly to `title`, `tags`, `author`, `last_update`.
- [x] Dual vector-FTS index synchronizes within a single SQLite transaction.
- [x] RRF score calculation properly balances dense semantic ranking and sparse keyword ranking.
- [x] High/Critical risk clauses trigger structured redline diff recommendations.

### 5.2 Unit Test Scenarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clause_risk_evaluation() {
        assert_eq!(
            evaluate_clause_risk("liability", "Customer agrees to uncapped liability for all damages."),
            ClauseRisk::Critical
        );
        assert_eq!(
            evaluate_clause_risk("liability", "Neither party aggregate liability shall exceed 12 months fees."),
            ClauseRisk::Medium
        );
        assert_eq!(
            evaluate_clause_risk("indemnity", "Customer shall indemnify Vendor against all claims."),
            ClauseRisk::High
        );
    }

    #[test]
    fn test_rrf_fusion_ordering() {
        let vector_hits = vec![("chunk_1".to_string(), 1), ("chunk_2".to_string(), 2)];
        let fts_hits = vec![("chunk_2".to_string(), 1), ("chunk_3".to_string(), 2)];
        let fused = fuse_rrf(vector_hits, fts_hits);

        // chunk_2 is present in both at top ranks, so it must rank #1
        assert_eq!(fused[0].0, "chunk_2");
        assert!(fused[0].1 > fused[1].1);
    }

    #[test]
    fn test_rag_tool_exec_policy() {
        assert_eq!(ExecPolicy::for_tool("rag", "query_hybrid"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("rag", "extract_tables"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("rag", "ingest_doc"), ExecPolicy::ProposeOnly);
    }
}
```
