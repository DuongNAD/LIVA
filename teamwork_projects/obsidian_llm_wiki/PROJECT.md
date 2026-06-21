# Project: Obsidian LLM Wiki

## Architecture
The system consists of an Obsidian Vault containing structured markdown files and a LIVA-Obsidian MCP Server to read/write/search these files safely.

```
+------------------------------------------+
|               LIVA Agent                 |
+------------------------------------------+
                     | (JSON-RPC over stdio)
                     v
+------------------------------------------+
|       LIVA-Obsidian MCP Server           |
|  - read_markdown                         |
|  - write_markdown                        |
|  - search_vault                          |
+------------------------------------------+
                     |
       (Validates paths within Vault)
                     v
+------------------------------------------+
|             Obsidian Vault               |
|  - Skills/       - Rules/                |
|  - Knowledge/    - Templates/            |
+------------------------------------------+
```

### Components
1. **Obsidian Vault**: Standardized folder structure (`Skills`, `Knowledge`, `Rules`, `Templates`) containing markdown files with Frontmatter metadata.
2. **MCP Server**: Node.js/TypeScript application utilizing the Model Context Protocol SDK to expose tools for vault manipulation and searching.
3. **Vault Validator Script**: A standalone validation script to verify that the Vault contains all required directories, files, and conforms to frontmatter standards.
4. **Test Suite**: Jest test cases verifying MCP functionality and security boundaries.

## Code Layout
- `vault/` - Root of the Obsidian Vault.
  - `Skills/` - LIVA capability files.
  - `Knowledge/` - LIVA semantic/domain knowledge files.
  - `Rules/` - Guidelines and constraints for LIVA.
  - `Templates/` - Frontmatter-equipped markdown templates.
- `src/` - MCP Server source files.
  - `index.ts` - Entry point.
  - `server.ts` - MCP Server initialization, tool registration, and handlers.
  - `vault.ts` - Core vault file operations and search logic.
- `scripts/` - Validation scripts.
  - `validate-vault.ts` - Vault structure and template frontmatter validation.
- `tests/` - Unit and integration tests.
  - `mcp-server.test.ts` - Automated Jest tests verifying the server's operations and path validation constraint.
- `package.json` - Node dependencies and scripts.
- `tsconfig.json` - TypeScript compilation options.
- `jest.config.js` - Jest configuration.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Obsidian Vault Setup | Create `vault/` folders, templates, and sample content | None | DONE |
| 2 | Vault Validation Script | Implement automated structure and frontmatter validation script | 1 | DONE |
| 3 | MCP Server Implementation | Implement MCP server, tools, and path protection logic | 1 | DONE |
| 4 | E2E & Automated Tests | Implement test suite verifying MCP connection, tools, and path limits | 3 | DONE |
| 5 | Challenger & Audit | Run Challengers and Forensic Auditor to ensure correctness and zero cheating | 4 | DONE |

## Interface Contracts

### MCP Tools

#### `read_markdown`
- **Arguments**:
  - `path` (string): Relative path from Vault root to the markdown file.
- **Returns**:
  - `content` (string): Text content of the file.
- **Error conditions**:
  - If target path resolves outside the Vault directory, throw a permission error.
  - If file does not exist, throw a not found error.

#### `write_markdown`
- **Arguments**:
  - `path` (string): Relative path from Vault root to save/update the file.
  - `content` (string): Markdown content including Frontmatter to write.
- **Returns**:
  - `success` (boolean): `true` if written successfully.
- **Error conditions**:
  - If target path resolves outside the Vault directory, throw a permission error.

#### `search_vault`
- **Arguments**:
  - `query` (string): Keyword or phrase to search within the vault.
- **Returns**:
  - `results` (array of objects): Matching markdown files, including relative path, matching line snippets, and titles.
- **Error conditions**:
  - None, returns empty list if no matches.
