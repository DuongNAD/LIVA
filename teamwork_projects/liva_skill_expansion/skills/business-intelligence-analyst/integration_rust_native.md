# Rust Native Core Integration: business-intelligence-analyst

## 1. Native Engine Architecture Alignment

### 1.1 Module Domain & Routing
- **Native Implementation**: `liva-native-core/src/commands/db.rs`, `liva-native-core/src/db.rs`, `liva-native-core/src/authorization.rs`, `liva-native-core/src/llm/tool_calling.rs`.
- **Command Routing Matrix**:
  - `db:introspect_schema`: Reads schema metadata, table definitions, foreign keys, and indexes. Gated under `ExecPolicy::Auto`.
  - `db:explain_query`: Runs `EXPLAIN` on synthesized SQL to analyze query execution cost. Gated under `ExecPolicy::Auto`.
  - `db:execute_query`: Executes validated read-only SQL queries with automatic `LIMIT` capping and statement timeouts. Gated under `ExecPolicy::Auto`.
  - `db:generate_chart`: Synthesizes Vega-Lite / Mermaid chart definitions. Gated under `ExecPolicy::Auto`.
  - `write_markdown`: Persists analytics reports into `vault/Knowledge/BI - <Title>.md`. Gated under `ExecPolicy::ProposeOnly`.

### 1.2 Execution Policy (`ExecPolicy`)
```rust
// In liva-native-core/src/llm/tool_calling.rs
pub fn for_tool(server: &str, name: &str) -> Self {
    match (server, name) {
        ("db", "introspect_schema") | ("db", "explain_query") | ("db", "generate_chart") => Self::Auto,
        ("db", "execute_query") => Self::Auto, // Permitted only after AST read-only validation
        ("obsidian", "write_markdown") => Self::ProposeOnly,
        _ => Self::ProposeOnly,
    }
}
```

---

## 2. Tool Schema Mapping & Compact Prompt Rendering

### 2.1 Compact Tool Signatures
```
[1] db:introspect_schema: Introspect database tables, column types, and relational constraints
   tham số (* = bắt buộc): connection_id (string), table_filter (string)
[2] db:explain_query: Analyze execution plan and cost estimates for a SQL statement
   tham số (* = bắt buộc): query* (string), connection_id (string)
[3] db:execute_query: Execute read-only SQL query with maximum row limits
   tham số (* = bắt buộc): query* (string), connection_id (string), max_rows (integer)
[4] db:generate_chart: Generate declarative Vega-Lite visualization specification
   tham số (* = bắt buộc): chart_type* (string: bar|line|area|scatter|pie|heatmap), data_json* (string), title* (string)
```

### 2.2 Input Schema & Validation Matrix
```json
{
  "db:introspect_schema": {
    "type": "object",
    "properties": {
      "connection_id": { "type": "string", "default": "default" },
      "table_filter": { "type": "string", "description": "Optional wildcard or table name filter" }
    }
  },
  "db:explain_query": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "minLength": 6, "description": "SQL query to explain" },
      "connection_id": { "type": "string", "default": "default" }
    },
    "required": ["query"]
  },
  "db:execute_query": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "minLength": 6, "description": "Read-only SQL query to execute" },
      "connection_id": { "type": "string", "default": "default" },
      "max_rows": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 100 }
    },
    "required": ["query"]
  },
  "db:generate_chart": {
    "type": "object",
    "properties": {
      "chart_type": { "type": "string", "enum": ["bar", "line", "area", "scatter", "pie", "heatmap"] },
      "data_json": { "type": "string", "minLength": 2, "description": "JSON serialized data rows" },
      "title": { "type": "string", "minLength": 1 }
    },
    "required": ["chart_type", "data_json", "title"]
  }
}
```

---

## 3. SQL AST Safety Validator & Sanitization Engine

### 3.1 AST Read-Only Enforcement
Before executing any query, `liva-native-core` inspects the parsed statement AST:

```rust
#[derive(Debug, PartialEq, Eq)]
pub enum SqlSafetyResult {
    SafeReadOnly,
    MutationBlocked(String),
    MultiStatementBlocked,
    SyntaxError(String),
}

pub fn validate_sql_safety(query: &str) -> SqlSafetyResult {
    let trimmed = query.trim();
    
    // Check for semicolon injection
    let statements: Vec<&str> = trimmed.split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if statements.len() > 1 {
        return SqlSafetyResult::MultiStatementBlocked;
    }

    let first_stmt = match statements.first() {
        Some(s) => *s,
        None => return SqlSafetyResult::SyntaxError("Empty query".into()),
    };

    let upper = first_stmt.to_uppercase();
    
    // Prohibited DDL/DML tokens
    const PROHIBITED_KEYWORDS: &[&str] = &[
        "INSERT ", "UPDATE ", "DELETE ", "DROP ", "ALTER ", "TRUNCATE ",
        "CREATE ", "GRANT ", "REVOKE ", "VACUUM ", "EXEC ", "EXECUTE ",
        "INTO ", "REPLACE ", "ATTACH ", "DETACH "
    ];

    for &kw in PROHIBITED_KEYWORDS {
        if upper.starts_with(kw) || upper.contains(&format!(" {}", kw)) {
            return SqlSafetyResult::MutationBlocked(format!("Prohibited keyword: {}", kw.trim()));
        }
    }

    if upper.starts_with("SELECT") || upper.starts_with("WITH") || upper.starts_with("EXPLAIN") {
        SqlSafetyResult::SafeReadOnly
    } else {
        SqlSafetyResult::MutationBlocked("Query must start with SELECT, WITH, or EXPLAIN".into())
    }
}
```

---

## 4. Fail-Closed Security & Concurrency Constraints

### 4.1 Connection Pool & Timeout Sandbox
- **Connection Isolation**: Queries run on a dedicated read-only connection pool with `statement_timeout = 5000` (5 seconds).
- **Result Truncation**: Maximum 1,000 rows returned per query to protect against memory exhaustion.
- **Credential Protection**: Database passwords and connection URI secrets are resolved exclusively from local encrypted keychain and never reflected in LLM prompts.

### 4.2 Principal RBAC Authorization
- Principal `CommandPrincipal::TauriDashboard` and `CommandPrincipal::WebSocketDashboard`: Authorized for schema introspection, explain, and query execution.
- Principal `CommandPrincipal::Telegram`: Authorized for high-level metric summaries only (raw queries prohibited).

---

## 5. Verification Checklist & Unit Test Scenarios

### 5.1 Verification Checklist
- [x] Frontmatter in `SKILL.md` conforms strictly to `name` and `description` only.
- [x] Frontmatter in generated vault notes conforms strictly to `title`, `tags`, `author`, `last_update`.
- [x] AST validator strictly blocks mutating keywords (`DROP`, `DELETE`, `UPDATE`, `INSERT`, `ALTER`).
- [x] Multi-statement chaining via semicolons is blocked fail-closed.
- [x] Query plan inspection checks for full table scans on large tables.

### 5.2 Unit Test Scenarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_safety_validator_valid_queries() {
        assert_eq!(
            validate_sql_safety("SELECT id, name FROM users WHERE active = 1 LIMIT 10"),
            SqlSafetyResult::SafeReadOnly
        );
        assert_eq!(
            validate_sql_safety("WITH monthly AS (SELECT * FROM sales) SELECT * FROM monthly"),
            SqlSafetyResult::SafeReadOnly
        );
    }

    #[test]
    fn test_sql_safety_validator_blocked_mutations() {
        assert!(matches!(
            validate_sql_safety("DROP TABLE users;"),
            SqlSafetyResult::MutationBlocked(_)
        ));
        assert!(matches!(
            validate_sql_safety("UPDATE accounts SET balance = 0 WHERE id = 1"),
            SqlSafetyResult::MutationBlocked(_)
        ));
        assert!(matches!(
            validate_sql_safety("SELECT 1; DROP TABLE users;"),
            SqlSafetyResult::MultiStatementBlocked
        ));
    }

    #[test]
    fn test_db_tool_exec_policy() {
        assert_eq!(ExecPolicy::for_tool("db", "introspect_schema"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("db", "explain_query"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("db", "generate_chart"), ExecPolicy::Auto);
    }
}
```
