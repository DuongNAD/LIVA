---
name: personal-knowledge-curator
description: Curate and organize atomic personal knowledge, maintain bidirectional Obsidian graphs, synthesize Maps of Content (MoCs), and bridge long-term memory consolidation. Use when structuring thoughts, cataloging research notes, fixing orphan notes, generating topic indexes, or reconciling knowledge taxonomies.
---

# Personal Knowledge Curator

## Workflow

1. **Vault Discovery & Conflict Detection**:
   - Query the Obsidian Vault via `search_vault` to find existing topic notes, aliases, and related tags before creating new files.
   - Prevent semantic fragmentation by checking if the concept already exists under a synonym or alternate naming scheme.
   - Read candidate reference notes using `read_markdown` to extract active taxonomy patterns and internal link conventions.

2. **Atomic Note Synthesis & Frontmatter Formatting**:
   - Author concise, self-contained atomic notes adhering to the single-responsibility principle (one core idea per note).
   - Format metadata strictly using the **Obsidian Vault Dialect**:
     ```yaml
     ---
     title: "Exact Title Matching Note Concept"
     tags:
       - knowledge/domain
       - topic/subtopic
     author: "user"
     last_update: "2026-08-14T12:00:00+07:00"
     ---
     ```
   - **Dialect Guard**: Never include `SKILL.md` frontmatter keys (`name`, `description`) in Obsidian vault notes.

3. **Bidirectional Link & Graph Curation**:
   - Establish context-rich bidirectional links using standard WikiLinks (`[[Note Title]]` or `[[Note Title|Custom Alias]]`).
   - Group related concepts into parent Map of Content (MoC) hubs (e.g., `Knowledge/Systems Architecture MoC.md`).
   - Ensure every newly created atomic note links to at least one parent hub or sibling concept to prevent orphan nodes.

4. **Taxonomy & Hierarchy Alignment**:
   - Place files in designated vault directories:
     - `Knowledge/`: Domain knowledge, theoretical concepts, architecture references, and technical summaries.
     - `Skills/`: Procedural runbooks, step-by-step guides, and capability specifications.
     - `Rules/`: Invariant constraints, coding standards, security requirements, and policy definitions.
     - `Daily/`: Ephemeral logs, daily journals, and time-stamped meeting notes.

5. **Memory Bridge & Fact Consolidation**:
   - Extract enduring personal preferences, system invariants, or factual statements and register them into LIVA's L1/L3 structured memory via `memory:set_fact`.
   - Maintain consistency between Obsidian vault graph nodes and SQLite relational facts (`turn_layer_nodes` / `psi_relations`).

6. **Validation & Safe Persistence**:
   - Check all internal relative links and WikiLinks for broken references.
   - Save updates using `write_markdown`. For mass structural reorganizations (refactoring folders or renaming >3 notes), provide a dry-run summary plan before committing changes.

## Platform Constraints

- **Execution Mode**: Hybrid. Read operations (`search_vault`, `read_markdown`) execute automatically. Mutating operations (`write_markdown`) operate under proposal mode unless explicitly allowlisted in `LIVA_MCP_AUTOEXEC`.
- **Tool Dependencies**: Requires `obsidian` MCP server (`search_vault`, `read_markdown`, `write_markdown`) and native core memory tools (`memory:search_hybrid`, `memory:set_fact`).
- **Filesystem Boundaries**: All path resolutions must reside within the canonical Obsidian vault root directory (`teamwork_projects/obsidian_llm_wiki/vault/`).
- **Timezone Standard**: All timestamp metadata must use ISO 8601 formatting with local timezone offset (`+07:00`).

## Stop Conditions

Stop and request user clarification or abort immediately when:
- The Obsidian MCP server is unavailable or the local vault root path cannot be resolved.
- A proposed edit would overwrite an unindexed personal journal note or destroy user-authored content without a prior diff preview.
- A path traversal attempt is detected (paths containing `..`, absolute disk paths, or junction links escaping the vault).
- Severe taxonomy ambiguity exists (e.g., conflicting duplicate notes with divergent factual claims).

## Knowledge Note Template

```markdown
---
title: "Reactive Agent Architecture"
tags:
  - knowledge/architecture
  - ai/agents
author: "user"
last_update: "2026-08-14T12:00:00+07:00"
---

# Reactive Agent Architecture

## Overview
Reactive agent architectures process real-time event streams and trigger deterministic or probabilistic decision pipelines without maintaining long-term mutable internal state loops.

## Core Characteristics
- **Event-Driven**: Dispatches actions in response to sensor inputs or message receipts.
- **Stateless Fast-Path**: Leverages low-latency heuristic routers (e.g., zero-token intent matchers) before invoking heavy reasoning LLMs.
- **Fail-Closed Execution**: Rejects unrecognized operations and falls back to user consent gates.

## Related Concepts
- [[Event Driven Microservices]]
- [[LIVA Memory Architecture]]
- [[Deterministic Tool Routing]]
```
