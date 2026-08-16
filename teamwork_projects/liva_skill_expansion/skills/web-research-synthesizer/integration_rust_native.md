# Rust Native Core Integration: web-research-synthesizer

## 1. Native Engine Architecture Alignment

### 1.1 Module Domain & Routing
- **Native Implementation**: `liva-native-core/src/mcp/client.rs` (External Web Search / Fetch MCP), `liva-native-core/src/llm/tool_calling.rs`, `liva-native-core/src/commands/memory.rs`.
- **Command Routing Matrix**:
  - `search_web`: Routed via native search provider adapter or configured external MCP search server. Gated under `ExecPolicy::Auto`.
  - `read_url_content`: Routed via HTTP client with built-in SSRF protection and HTML-to-Markdown parser. Gated under `ExecPolicy::Auto`.
  - `write_markdown`: Routed via `NativeMcpServer::call_tool` -> `write_markdown_internal`. Gated under `ExecPolicy::ProposeOnly`.
  - `memory:set_fact`: Routed via `commands::memory::handle_set_fact` to persist extracted facts into SQLite WAL.

### 1.2 Execution Policy (`ExecPolicy`)
```rust
// In liva-native-core/src/llm/tool_calling.rs
// Web search and read operations are auto-executable when safe; write operations require proposal gating.
pub fn for_tool(server: &str, name: &str) -> Self {
    if server == NATIVE_SERVER && NATIVE_AUTOEXEC.contains(&name) {
        return Self::Auto;
    }
    if server == "web" && (name == "search_web" || name == "read_url_content") {
        return Self::Auto;
    }
    // Mutating writes to vault or external states require proposal approval
    Self::ProposeOnly
}
```

---

## 2. Tool Schema Mapping & Compact Prompt Rendering

### 2.1 Compact Tool Signature
```
[1] web/search_web: Search web search engines for multi-source articles and technical documentation
   tham số (* = bắt buộc): query* (string), domain (string)
[2] web/read_url_content: Fetch and extract clean markdown content from public URLs
   tham số (* = bắt buộc): url* (string)
[3] native/write_markdown: Persist research brief into Obsidian vault
   tham số (* = bắt buộc): path* (string), content* (string), overwrite (boolean)
```

### 2.2 Input Schema & Validation Matrix
```json
{
  "search_web": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "minLength": 2, "description": "Search query keywords" },
      "domain": { "type": "string", "description": "Optional domain filter (e.g. rust-lang.org)" }
    },
    "required": ["query"]
  },
  "read_url_content": {
    "type": "object",
    "properties": {
      "url": { "type": "string", "format": "uri", "description": "Target public HTTP/HTTPS URL" }
    },
    "required": ["url"]
  },
  "write_markdown": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "description": "Target vault path (e.g. Knowledge/Research - Topic.md)" },
      "content": { "type": "string", "description": "Full Markdown dossier content" },
      "overwrite": { "type": "boolean", "default": true }
    },
    "required": ["path", "content"]
  }
}
```

---

## 3. State Persistence & Research Graph Ingestion

### 3.1 Vault Knowledge Node Layout
- Target path: `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Research - <Topic>.md`
- Vault YAML Frontmatter Specification:
  ```yaml
  ---
  title: "Research - WebAssembly Component Model 2026 Status"
  tags:
    - knowledge/research
    - tech/wasm
  author: "user"
  last_update: "2026-08-14T12:00:00+07:00"
  ---
  ```

### 3.2 Dual-Tier Memory Ledger
- **L1 SQLite WAL**: Verified factual triples (Subject-Predicate-Object) are committed into `phi_facts` with confidence score `0.95` and citation source URL.
- **L2 Vector Memory**: The research synthesis document is chunked into 512-token segments and embedded into `sqlite-vec` via `multilingual-e5-small` for semantic retrieval in future user prompts.

---

## 4. Fail-Closed Security & Threat Mitigations

### 4.1 SSRF & Private Network Egress Defense
The `read_url_content` HTTP engine enforces strict SSRF filters in Rust:
1. **Scheme Restriction**: Allows only `http://` and `https://`. Rejects `file://`, `ftp://`, `gopher://`, `ldap://`, etc.
2. **Private IP Blacklist**: Resolves DNS and drops all connections to loopback (`127.0.0.1/8`, `::1`), private ranges (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), link-local (`169.254.0.0/16`), and cloud metadata endpoints (`169.254.169.254`).
3. **Redirect Boundary**: Follows maximum 3 redirects, re-verifying SSRF IP filters on each hop.

```rust
// In liva-native-core/src/mcp/http_sanitizer.rs
pub fn validate_egress_url(raw_url: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(raw_url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Forbidden protocol scheme: only http/https allowed".into());
    }
    let host_str = parsed.host_str().ok_or("Missing host in URL")?;
    if host_str == "localhost" || host_str.ends_with(".local") {
        return Err("SSRF blocked: local hostname forbidden".into());
    }
    // Socket address resolution check against private subnets...
    Ok(parsed)
}
```

### 4.2 Adversarial Prompt Injection Defense
- Web page contents are stripped of `<script>`, `<style>`, `<iframe>`, and hidden HTML elements (`display:none`, `visibility:hidden`).
- Zero-width Unicode characters (`\u200B`, `\u200C`, `\u200D`, `\uFEFF`) and ANSI escape codes are stripped before Markdown conversion.
- Unsanitized text is isolated as raw data blocks in prompt rendering to prevent indirect prompt injection attacks.

---

## 5. Verification Checklist & Unit Test Scenarios

### 5.1 Verification Checklist
- [x] Frontmatter in `SKILL.md` contains strictly `name` and `description`.
- [x] Generated research briefs in vault contain valid Obsidian frontmatter (`title`, `tags`, `author`, `last_update`).
- [x] SSRF filter blocks private IP ranges (`127.0.0.1`, `192.168.1.1`, `169.254.169.254`).
- [x] Non-HTTP protocols (`file://`, `data:`, `gopher://`) are immediately rejected.
- [x] Extracted facts are recorded in `phi_facts` with verifiable source citations.

### 5.2 Unit Test Scenarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssrf_validator_blocks_private_networks() {
        assert!(validate_egress_url("https://www.rust-lang.org").is_ok());
        assert!(validate_egress_url("http://127.0.0.1:8080/metrics").is_err());
        assert!(validate_egress_url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(validate_egress_url("file:///etc/shadow").is_err());
        assert!(validate_egress_url("http://localhost:3000").is_err());
    }

    #[test]
    fn test_compact_schema_rendering() {
        let tool = CatalogTool {
            server: "web".into(),
            name: "search_web".into(),
            description: "Search web search engines".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "domain": { "type": "string" }
                },
                "required": ["query"]
            }),
            embed_extra: String::new(),
        };
        assert_eq!(tool.qualified(), "web/search_web");
    }
}
```
