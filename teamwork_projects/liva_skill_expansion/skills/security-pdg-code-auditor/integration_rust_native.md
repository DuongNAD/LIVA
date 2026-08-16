# Rust Native Core Integration: security-pdg-code-auditor

## 1. Native Engine Architecture Alignment

### 1.1 Module Domain & Routing
- **Native Implementation**: `liva-native-core/src/mcp/client.rs` (GitNexus MCP Client Bridge), `liva-native-core/src/authorization.rs`, `liva-native-core/src/llm/tool_calling.rs`.
- **Command Routing Matrix**:
  - `gitnexus:explain`: Taint findings and source-to-sink flow extraction. Gated under `ExecPolicy::Auto`.
  - `gitnexus:pdg_query`: Control dependence (CDG) and reaching definition (REACHING_DEF) queries. Gated under `ExecPolicy::Auto`.
  - `gitnexus:impact`: Upstream and downstream blast radius calculation. Gated under `ExecPolicy::Auto`.
  - `gitnexus:detect_changes`: Change detection to verify patch boundaries. Gated under `ExecPolicy::Auto`.
  - `write_markdown`: Persisting security audit findings into `vault/Knowledge/Security - <Audit_Title>.md`. Gated under `ExecPolicy::ProposeOnly`.

### 1.2 Execution Policy (`ExecPolicy`)
```rust
// In liva-native-core/src/llm/tool_calling.rs
// GitNexus PDG analysis tools are read-only graph queries and run automatically:
pub fn for_tool(server: &str, name: &str) -> Self {
    if server == "gitnexus" {
        return Self::Auto;
    }
    if server == NATIVE_SERVER && NATIVE_AUTOEXEC.contains(&name) {
        return Self::Auto;
    }
    // File modifications and code edits require proposal gating
    Self::ProposeOnly
}
```

---

## 2. Tool Schema Mapping & Compact Prompt Rendering

### 2.1 Compact Tool Signature
```
[1] gitnexus/explain: Extract taint flows (source->sink) for target file or symbol
   tham số (* = bắt buộc): target* (string)
[2] gitnexus/pdg_query: Query control dependence (CDG) or reaching data flows (REACHING_DEF)
   tham số (* = bắt buộc): mode* (string: controls|flows), target* (string), variable (string)
[3] gitnexus/impact: Compute blast radius of proposed symbol change
   tham số (* = bắt buộc): target* (string), direction (string), mode (string)
```

### 2.2 Input Schema & Validation Matrix
```json
{
  "explain": {
    "type": "object",
    "properties": {
      "target": { "type": "string", "minLength": 1, "description": "File path or function symbol" }
    },
    "required": ["target"]
  },
  "pdg_query": {
    "type": "object",
    "properties": {
      "mode": { "type": "string", "enum": ["controls", "flows"], "description": "PDG traversal mode" },
      "target": { "type": "string", "minLength": 1, "description": "Symbol or file path" },
      "variable": { "type": "string", "description": "Variable name to trace (required for flows mode)" }
    },
    "required": ["mode", "target"]
  },
  "impact": {
    "type": "object",
    "properties": {
      "target": { "type": "string", "minLength": 1, "description": "Symbol name to evaluate" },
      "direction": { "type": "string", "enum": ["upstream", "downstream", "both"], "default": "upstream" },
      "mode": { "type": "string", "enum": ["graph", "pdg"], "default": "pdg" }
    },
    "required": ["target"]
  }
}
```

---

## 3. State Persistence & Security Vulnerability Registry

### 3.1 Vault Security Node Layout
- Destination: `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Security - <Vulnerability_Title>.md`
- Vault YAML Frontmatter Specification:
  ```yaml
  ---
  title: "Security - Path Traversal Taint Analysis"
  tags:
    - knowledge/security
    - audit/pdg
  author: "user"
  last_update: "2026-08-14T12:00:00+07:00"
  ---
  ```

### 3.2 Signal Logging in SQLite WAL
- If a tool fails or generates false-positive taint paths, quality signals are recorded via `liva-native-core/src/skills/signals.rs`:
  ```sql
  INSERT INTO skill_signals (id, skill_id, signal_kind, evidence_status, weight, merge_key, created_at)
  VALUES (?1, ?2, 'tool_semantic_issue', 'confirmed', 1.0, ?3, datetime('now'));
  ```

---

## 4. Fail-Closed Security & Sandboxing Constraints

### 4.1 Vulnerability Warning Thresholds
1. **Critical Blast Radius Warning**: If `gitnexus_impact` returns risk level `CRITICAL` or `HIGH` (>10 affected execution flows or root authentication modules), the engine forces an explicit warning output and suspends automated edits until user confirmation is logged.
2. **Deterministic Taint Verification**: Taint paths identified by `explain` must have matching CDG/REACHING_DEF verification in `pdg_query` before being classified as confirmed vulnerabilities.
3. **No Autonomous Git Commits**: Patches are generated as unified diffs and staged only with explicit user permission; commits and pushes are never executed autonomously.

```rust
// Risk classification guard in Rust engine
#[derive(Debug, PartialEq, Eq)]
pub enum BlastRisk {
    Low,
    Medium,
    High,
    Critical,
}

pub fn evaluate_blast_radius(callers: usize, affected_flows: usize, is_security_kernel: bool) -> BlastRisk {
    if is_security_kernel || affected_flows >= 10 {
        BlastRisk::Critical
    } else if affected_flows >= 5 || callers >= 15 {
        BlastRisk::High
    } else if affected_flows >= 2 || callers >= 5 {
        BlastRisk::Medium
    } else {
        BlastRisk::Low
    }
}
```

### 4.2 Principal RBAC Authorization
- Principal `CommandPrincipal::TauriDashboard` and `CommandPrincipal::WebSocketDashboard`: Authorized for all GitNexus graph queries and remediation report generation.
- Principal `CommandPrincipal::Telegram`: Limited to high-level security summaries (`status`, `skills:search`).

---

## 5. Verification Checklist & Unit Test Scenarios

### 5.1 Verification Checklist
- [x] Frontmatter in `SKILL.md` conforms strictly to `name` and `description` only.
- [x] Frontmatter in generated vault notes conforms strictly to `title`, `tags`, `author`, `last_update`.
- [x] GitNexus PDG integration accurately parses source-to-sink taint traces.
- [x] Blast radius calculation triggers High/Critical risk alerts when crossing threshold boundaries.
- [x] Proposed patches maintain backward compatibility and pass regression test suites.

### 5.2 Unit Test Scenarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blast_radius_risk_evaluation() {
        assert_eq!(evaluate_blast_radius(2, 1, false), BlastRisk::Low);
        assert_eq!(evaluate_blast_radius(6, 3, false), BlastRisk::Medium);
        assert_eq!(evaluate_blast_radius(16, 5, false), BlastRisk::High);
        assert_eq!(evaluate_blast_radius(1, 1, true), BlastRisk::Critical);
        assert_eq!(evaluate_blast_radius(20, 12, false), BlastRisk::Critical);
    }

    #[test]
    fn test_gitnexus_tool_exec_policy() {
        assert_eq!(ExecPolicy::for_tool("gitnexus", "explain"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("gitnexus", "pdg_query"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("gitnexus", "impact"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("gitnexus", "detect_changes"), ExecPolicy::Auto);
    }
}
```
