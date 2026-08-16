# Rust Native Core Integration: personal-knowledge-curator

## 1. Native Engine Architecture Alignment

### 1.1 Module Domain & Routing
- **Native Implementation**: `liva-native-core/src/mcp/server.rs` (`NativeMcpServer`), `liva-native-core/src/commands/memory.rs`, and `liva-native-core/src/agent/memory.rs`.
- **Command Routing Matrix**:
  - `search_vault`: Routed via `NativeMcpServer::call_tool` -> `search_vault_internal`. Auto-executed under `ExecPolicy::Auto`.
  - `read_markdown`: Routed via `NativeMcpServer::call_tool` -> `read_markdown_internal`. Auto-executed under `ExecPolicy::Auto`.
  - `write_markdown`: Routed via `NativeMcpServer::call_tool` -> `write_markdown_internal`. Gated under `ExecPolicy::ProposeOnly` unless explicitly allowlisted in `LIVA_MCP_AUTOEXEC`.
  - `memory:search_hybrid`: Routed via `commands::memory::handle_search_hybrid`. Authorized for `TauriDashboard`, `WebSocketDashboard`.
  - `memory:set_fact`: Routed via `commands::memory::handle_set_fact`. Authorized for `TauriDashboard`, `WebSocketDashboard`.

### 1.2 Execution Policy (`ExecPolicy`)
```rust
// In liva-native-core/src/llm/tool_calling.rs
const NATIVE_AUTOEXEC: &[&str] = &[
    "read_markdown",
    "search_vault",
    "control_smarthome",
    "control_volume",
    "control_media",
];
```
- `search_vault` and `read_markdown` are registered in `NATIVE_AUTOEXEC` and return immediately without user prompt interruptions.
- `write_markdown` requires user consent confirmation or explicit runtime elevation via `LIVA_MCP_AUTOEXEC=native/write_markdown`.

---

## 2. Tool Schema Mapping & Compact Prompt Rendering

### 2.1 Compact Tool Signature
To fit within the 4096-token context window of small local LLMs (2B–4B), parameters are rendered as compact single-line signatures with `*` designating required arguments:

```
[1] native/search_vault: Search notes in Obsidian vault by query keyword or tag
   tham số (* = bắt buộc): query* (string), limit (number)
[2] native/read_markdown: Read full content of a markdown note from Obsidian vault
   tham số (* = bắt buộc): path* (string)
[3] native/write_markdown: Create or update a markdown note in Obsidian vault
   tham số (* = bắt buộc): path* (string), content* (string), overwrite (boolean)
```

### 2.2 Input Schema & Validation Matrix
```json
{
  "search_vault": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "minLength": 1, "description": "Search term or tag" },
      "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 10 }
    },
    "required": ["query"]
  },
  "read_markdown": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "minLength": 1, "description": "Relative path within vault root" }
    },
    "required": ["path"]
  },
  "write_markdown": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "minLength": 1, "description": "Relative path within vault root" },
      "content": { "type": "string", "description": "Markdown body including Obsidian YAML frontmatter" },
      "overwrite": { "type": "boolean", "default": false }
    },
    "required": ["path", "content"]
  }
}
```

---

## 3. State Persistence & Memory Synchronization

### 3.1 Vault Storage Architecture
- **Root Directory**: `teamwork_projects/obsidian_llm_wiki/vault/`
- **Subdirectory Layout**:
  - `Knowledge/*.md`: Conceptual notes, technical frameworks, system architectures.
  - `Skills/*.md`: Procedural guides, runbooks, operational playbooks.
  - `Rules/*.md`: Invariant rules, agent guidelines, security mandates.
  - `Daily/YYYY-MM-DD.md`: Daily logs, scratchpads, fleeting notes.

### 3.2 L1/L3 Dual-Write Synchronization
When a knowledge note establishes a permanent factual invariant:
1. Note is persisted into `vault/Knowledge/<Note Title>.md` using `write_markdown`.
2. Factual proposition is inserted into SQLite WAL `phi_facts` table:
   ```sql
   INSERT INTO phi_facts (id, subject, predicate, object, confidence, source_note, created_at)
   VALUES (?1, ?2, ?3, ?4, 1.0, ?5, datetime('now'))
   ON CONFLICT(subject, predicate) DO UPDATE SET
     object = excluded.object,
     source_note = excluded.source_note,
     updated_at = datetime('now');
   ```
3. Semantic relationship edge is inserted into `psi_relations` table for bidirectional graph queries.

---

## 4. Fail-Closed Security & Sandboxing Constraints

### 4.1 3-Layer Path Sandbox (`resolve_path`)
All filesystem I/O operations must pass the triple-check sandbox in `liva-native-core/src/mcp/server.rs`:
1. **Lexical Inspection**: Rejects absolute drive paths (`C:\...`, `/etc/...`), null bytes, and path traversal tokens (`..`).
2. **Prefix Containment**: Verifies that the normalized relative path resolves strictly inside the vault root.
3. **Canonical Filesystem Resolution**: Resolves the nearest existing ancestor on the host filesystem (`to_ton_tai_gan_nhat`) and confirms that canonical symlink targets and Windows Directory Junctions (`mklink /J`) remain within the canonical vault root boundary.

```rust
// Path sandbox validation contract
pub fn resolve_vault_path(root: &Path, rel_path: &str) -> Result<PathBuf, String> {
    if rel_path.contains("..") || Path::new(rel_path).is_absolute() {
        return Err("Path traversal violation: absolute paths and '..' are forbidden".into());
    }
    let target = root.join(rel_path);
    let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
    let canonical_target = target.canonicalize().unwrap_or_else(|_| target.clone());
    if !canonical_target.starts_with(&canonical_root) {
        return Err("Security boundary violation: target escapes vault sandbox".into());
    }
    Ok(target)
}
```

### 4.2 Principal RBAC Authorization
- Principal `CommandPrincipal::TauriDashboard` and `CommandPrincipal::WebSocketDashboard`: Full read/write access with interactive confirmation for file overwrite operations.
- Principal `CommandPrincipal::Telegram`: Strictly read-only query dispatch (`search_vault`, `read_markdown`). Mutating commands (`write_markdown`) fail closed with `403 Forbidden`.

---

## 5. Verification Checklist & Unit Test Scenarios

### 5.1 Verification Checklist
- [x] Frontmatter in `SKILL.md` conforms strictly to `name` and `description` only.
- [x] Frontmatter in generated vault notes conforms strictly to `title`, `tags`, `author`, `last_update`.
- [x] Path traversal attacks (`../`, `..\`, absolute root paths, symlink escapes) are rejected with error code.
- [x] Auto-exec allowlist enforces `ProposeOnly` for non-whitelisted mutating file writes.
- [x] Dual-write to SQLite WAL `phi_facts` maintains transactional integrity.

### 5.2 Unit Test Scenarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_path_sandbox_rejects_traversal() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path();

        assert!(resolve_vault_path(vault_root, "Knowledge/ValidNote.md").is_ok());
        assert!(resolve_vault_path(vault_root, "../outside.md").is_err());
        assert!(resolve_vault_path(vault_root, "..\\windows_escape.md").is_err());
        assert!(resolve_vault_path(vault_root, "/etc/passwd").is_err());
        assert!(resolve_vault_path(vault_root, "C:\\Windows\\System32\\calc.exe").is_err());
    }

    #[test]
    fn test_exec_policy_for_pkm_tools() {
        assert_eq!(ExecPolicy::for_tool(NATIVE_SERVER, "search_vault"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool(NATIVE_SERVER, "read_markdown"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool(NATIVE_SERVER, "write_markdown"), ExecPolicy::ProposeOnly);
    }
}
```
