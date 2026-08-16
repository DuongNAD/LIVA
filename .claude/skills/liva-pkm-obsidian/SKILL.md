---
name: liva-pkm-obsidian
description: Curate personal knowledge, capture daily notes, manage structured metadata, and retrieve concepts from the local Obsidian Vault. Use when capturing notes, organizing ideas, linking markdown documents, or searching vault knowledge.
---

# LIVA PKM Obsidian

## Workflow

1. **Vault Discovery**: Before drafting or updating notes, query the local vault via Obsidian MCP (`search_vault`) to identify existing knowledge nodes, taxonomy, and avoid duplication.
2. **Read & Extract Context**: Use `read_markdown` on relevant target files to understand current schema, links, and structure.
3. **Structured Frontmatter Compliance**:
   - Every vault markdown note must include Obsidian dialect YAML frontmatter:
     ```yaml
     ---
     title: "Exact Title Matching Topic"
     tags:
       - liva/topic
       - category/subtopic
     author: "user" # or agent name
     last_update: "YYYY-MM-DDTHH:mm:ss+07:00"
     ---
     ```
   - Never inject `SKILL.md` dialect keys (`name`, `description`) into vault notes.
4. **Link & Graph Synthesis**:
   - Connect related topics using standard `[[WikiLinks]]` or markdown links.
   - Maintain index files and folder structures (`Knowledge/`, `Skills/`, `Rules/`).
5. **Write / Update (`write_markdown`)**:
   - Safely append or update note contents preserving existing markdown headers and code blocks.

## Stop Conditions

Stop and report when:
- The Obsidian MCP server is disconnected or vault directory is missing.
- Conflicting note titles or inconsistent tag taxonomies are detected without user direction.
- A proposed edit would overwrite unstructured personal journal entries without explicit confirmation.
