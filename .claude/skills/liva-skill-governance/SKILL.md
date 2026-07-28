---
name: liva-skill-governance
description: Audit and maintain LIVA's Claude, Codex, and Obsidian skill knowledge. Use when adding, updating, or removing a skill; checking skill frontmatter or metadata; investigating stale or foreign instructions; validating cross-agent parity; or preparing skill changes for review.
---

# LIVA Skill Governance

## Workflow

1. Read `AGENTS.md`, `CLAUDE.md`, and `git status --short`. Preserve every pre-existing dirty file.
2. Call the Obsidian `search_vault` tool for `skill workflow coding standards testing guidelines`, then read the matched sources. Do not replace this step with text search while the MCP server is available.
3. Run `npm run skills:audit`. Errors block completion; warnings are technical debt that must be classified.
4. Scaffold new skills with the official `skill-creator` scripts. A `SKILL.md` frontmatter contains only `name` and `description`; agent UI and tool dependencies belong in `agents/openai.yaml`.
5. When a skill exists in both `.claude/skills/` and `.agents/skills/`, keep the two directories byte-identical. Vault notes should point to the canonical skill instead of duplicating its full instructions.
6. Validate every changed skill with the official `quick_validate.py`, run `npm run test:skills`, then rerun `npm run skills:audit`.
7. Report scanned counts, errors, warnings, and exact paths. Follow the repository Git boundary: stage only the authorized files and leave commit, merge, and remote operations to the user.

## Metadata dialects

- `SKILL.md`: `name`, `description`.
- `agents/openai.yaml`: `interface`, optional tool `dependencies`, and invocation `policy`.
- Obsidian Vault: `title`, `tags`, `author`, `last_update`.

Never copy keys between these dialects merely to satisfy a validator.

## Stop conditions

Stop and report the evidence when `search_vault` is unavailable, an audit error remains, a scaffold placeholder remains, a relative link is broken, or mirrored skill directories differ.
