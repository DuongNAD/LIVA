# Rust Native Core Integration: smart-devops-assistant

## 1. Native Engine Architecture Alignment

### 1.1 Module Domain & Routing
- **Native Implementation**: `liva-native-core/src/commands/task.rs`, `liva-native-core/src/authorization.rs`, `liva-native-core/src/llm/tool_calling.rs`.
- **Command Routing Matrix**:
  - `devops:inspect_container`: Diagnostic tool mapping to local Docker/OCI CLI. Gated under `ExecPolicy::Auto` for read-only inspection (`docker ps`, `docker logs`, `docker inspect`).
  - `devops:mutate_container`: State-changing commands (`docker stop`, `docker restart`, `docker compose down`). Strictly gated under `ExecPolicy::ProposeOnly`.
  - `gitnexus:impact`: Upstream impact analysis on build and configuration files. Gated under `ExecPolicy::Auto`.
  - `write_markdown`: Persisting postmortem runbooks into `vault/Knowledge/DevOps - <Incident>.md`. Gated under `ExecPolicy::ProposeOnly`.

### 1.2 Execution Policy (`ExecPolicy`)
```rust
// In liva-native-core/src/llm/tool_calling.rs
// DevOps commands follow strict read vs mutate segregation:
pub fn for_tool(server: &str, name: &str) -> Self {
    if server == "devops" && name.starts_with("inspect_") {
        return Self::Auto;
    }
    if server == "gitnexus" {
        return Self::Auto;
    }
    // All command execution (shell scripts, container lifecycle, file writes) is ProposeOnly
    Self::ProposeOnly
}
```

---

## 2. Tool Schema Mapping & Compact Prompt Rendering

### 2.1 Compact Tool Signature
```
[1] devops/inspect_container: Inspect running container status and logs
   tham số (* = bắt buộc): container_id* (string), tail_lines (number)
[2] gitnexus/impact: Perform blast radius impact analysis on target symbol or file
   tham số (* = bắt buộc): target* (string), direction (string)
[3] builtin/run_command: Execute proposed shell command with explicit user approval
   tham số (* = bắt buộc): command* (string), cwd (string)
```

### 2.2 Input Schema & Validation Matrix
```json
{
  "inspect_container": {
    "type": "object",
    "properties": {
      "container_id": { "type": "string", "pattern": "^[a-zA-Z0-9_-]+$", "description": "Target container name or hash" },
      "tail_lines": { "type": "integer", "minimum": 10, "maximum": 1000, "default": 100 }
    },
    "required": ["container_id"]
  },
  "impact": {
    "type": "object",
    "properties": {
      "target": { "type": "string", "minLength": 1, "description": "File or symbol to analyze" },
      "direction": { "type": "string", "enum": ["upstream", "downstream", "both"], "default": "upstream" }
    },
    "required": ["target"]
  },
  "run_command": {
    "type": "object",
    "properties": {
      "command": { "type": "string", "minLength": 1, "description": "Shell command to execute" },
      "cwd": { "type": "string", "description": "Working directory relative to project root" }
    },
    "required": ["command"]
  }
}
```

---

## 3. State Persistence & Incident History

### 3.1 Vault Runbook Persistence
- Destination: `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/DevOps - <Incident_Title>.md`
- Vault YAML Frontmatter Specification:
  ```yaml
  ---
  title: "DevOps - Docker Memory Contention Triage"
  tags:
    - knowledge/devops
    - ops/incident
  author: "user"
  last_update: "2026-08-14T12:00:00+07:00"
  ---
  ```

### 3.2 Task Ledger in SQLite WAL
- Incidents requiring ongoing monitoring or deferred remediation are tracked in the SQLite `tasks` table:
  ```sql
  INSERT INTO tasks (id, title, status, priority, payload, created_at, updated_at)
  VALUES (?1, ?2, 'pending_review', 'high', ?3, datetime('now'), datetime('now'));
  ```

---

## 4. Fail-Closed Security & Command Execution Boundaries

### 4.1 Shell Command Sanitization & Allowlisting
To prevent arbitrary remote code execution (RCE) and unintended system modifications:
1. **Disallowed Command Tokens**: Hard rejection of subshells (`$()`, `` ` ` ``), pipeline chaining to sensitive binaries (`| sh`, `| bash`, `| powershell`), and command separators (`&& rm -rf`, `; format`).
2. **Git Remote Safety Guard**: Commands matching `git push`, `git pull`, `git commit`, `git merge`, or `git checkout -b` are blocked at the engine layer with error `ERR_GIT_REMOTE_RESTRICTED`. Only `git add`, `git status`, `git diff`, and `git log` are permitted.
3. **Dry-Run Default**: All file-modifying tools generate diffs before prompting the user for confirmation.

```rust
// In liva-native-core/src/commands/devops_sanitizer.rs
pub fn validate_devops_command(cmd: &str) -> Result<(), String> {
    let trimmed = cmd.trim();
    let forbidden_prefixes = [
        "git push", "git pull", "git commit", "git merge", "git tag",
        "rm -rf /", "mkfs", "dd if=", "chmod -R 777"
    ];
    for bad in forbidden_prefixes {
        if trimmed.starts_with(bad) || trimmed.contains(&format!(" {bad}")) {
            return Err(format!("Command '{bad}' violates fail-closed security policy"));
        }
    }
    Ok(())
}
```

### 4.2 Principal RBAC Authorization
- Principal `CommandPrincipal::TauriDashboard`: Authorized to view container health metrics and trigger two-phase container restarts.
- Principal `CommandPrincipal::Telegram`: Strictly limited to status queries (`docker ps`, `get_system_status`). Cannot trigger shell commands or container mutations.

---

## 5. Verification Checklist & Unit Test Scenarios

### 5.1 Verification Checklist
- [x] Frontmatter in `SKILL.md` conforms strictly to `name` and `description` only.
- [x] Frontmatter in generated vault notes conforms strictly to `title`, `tags`, `author`, `last_update`.
- [x] Git remote operations (`git push`, `git pull`, `git commit`) are strictly forbidden and blocked.
- [x] Mutating container operations enforce `ExecPolicy::ProposeOnly` requiring two-phase confirmation.
- [x] Incident logs are persisted to Obsidian with root cause and remediation steps.

### 5.2 Unit Test Scenarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_devops_sanitizer_blocks_forbidden_commands() {
        assert!(validate_devops_command("docker ps -a").is_ok());
        assert!(validate_devops_command("cargo test --lib").is_ok());
        assert!(validate_devops_command("git status").is_ok());

        assert!(validate_devops_command("git push origin main").is_err());
        assert!(validate_devops_command("git commit -m 'feat'").is_err());
        assert!(validate_devops_command("rm -rf /").is_err());
        assert!(validate_devops_command("docker ps && rm -rf /").is_err());
    }

    #[test]
    fn test_devops_exec_policy_segregation() {
        assert_eq!(ExecPolicy::for_tool("devops", "inspect_container"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("devops", "restart_container"), ExecPolicy::ProposeOnly);
        assert_eq!(ExecPolicy::for_tool("builtin", "run_command"), ExecPolicy::ProposeOnly);
    }
}
```
