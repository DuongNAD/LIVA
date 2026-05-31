# 🌌 Antigravity-Vibe (project-name) Project Rules & Standards

> **System Constraints**: 
> 1. **[NO-YAPPING]**: Provide only the requested code or configurations. Omit conversational filler.
> 2. **[GIT-COMMIT-STYLE]**: Format all code modification summaries as conventional Git commits (e.g., `feat(api): add feed endpoint`).

This file serves as the core instruction set for all AI agents working on this project.

> **Note on Branding**: The official app name is **Antigravity-Vibe** (with Old Norse ǫ character), but for convenience in documentation and conversation, we use **Antigravity-Vibe**. The UI displays the authentic **Antigravity-Vibe** branding.

## 🏗️ Architecture & Stack

- **Your stack here**

## 💻 Development Environment (MANDATORY)

> [!CAUTION]
> This project is developed inside **Google Antigravity** on **Windows 11** with **PowerShell 7**. All terminal commands run locally MUST use Windows/PowerShell syntax.

**Scope**: These rules apply to **local terminal commands only** (commands executed on the developer's Windows machine). They do NOT apply to:

- **Git commands** — Git uses its own cross-platform syntax (`git add`, `git commit`, etc.)
- **Deployment scripts** — Test/prod servers run **Ubuntu 24.04 LTS** and use Bash (`update.sh`, deployment scripts in `scripts/deploy/`)
- **SSH sessions** — Remote commands on test/prod servers use Bash/Linux syntax
- **NPM scripts** — Cross-platform by nature, always acceptable
- **Documentation** — Server-side docs and guides may reference Linux/Bash commands for deployment contexts

**Strictly forbidden in local terminal commands:**

- ❌ Bash syntax (`&&` chaining in bash style, `export VAR=value`, `source`, `#!/bin/bash`)
- ❌ Forward-slash paths (`/path/to/file`) → use backslash (`\path\to\file`)

**Required conventions:**

- ✅ Use backslash `\` for all file paths
- ✅ Use `$env:VAR` for environment variables (not `export VAR`)
- ✅ Use `;` for command chaining (not `&&`)
- ✅ NPM scripts (`npm run start`, `npm run dev`) work cross-platform and are always acceptable

## 📜 Coding Standards

- **Strict TypeScript**: Prefer explicit types over `any`. Avoid type assertions unless augmenting external libraries.
- **Module Separation**: Logic in `services/`, endpoints in `routes/`, UI in `components/`.
- **Atomic Operations**: Use `upsert` and transactions for data integrity.
- **AI Context Ready**: Keep files under the God Component threshold (see File Size Zones below). Use clear

### 🛡️ Zero `any` Policy

`any` is prohibited in production code (`server/routes/`, `server/services/`, `server/middleware/`). Approved workarounds:

- **External Libraries**: Create an interface matching the expected shape.
- **Sequelize**: `as any` **ONLY** for `where` clauses where Sequelize types are too strict. **Comment explicitly.**
- **Express**: Extend `Express.Request` in `server/types/express.d.ts` instead of casting `req` to `any`.

## 📂 Documentation Structure

- **Roadmap**: `docs/project/roadmap.md`
- **Backlog**: `docs/project/backlog.md`
- **Feature Status**: `docs/project/feature-status.md`
- **Changelog**: `CHANGELOG.md` (root) — **Must be updated on every feature release** via `/feature-complete` workflow

### ⚠️ AI Prompt Ownership (Meta-Prompt Pattern)

> [!CAUTION]
> The prompts in `docs/prompts/` are **generated outputs** produced by their META prompts. They are overwritten every time `/run-meta-prompts` is executed. **Never edit them directly** — changes will be lost on the next meta-prompt run.

| Generated Prompt (OUTPUT — do not edit directly) | Meta Prompt (SOURCE — edit this instead)             |
| ------------------------------------------------ | ---------------------------------------------------- |
| `docs/prompts/code-review-prompt.md`             | `docs/prompts/_meta/optimize-code-review.md`         |
| `docs/prompts/spring-cleaning-prompt.md`         | `docs/prompts/_meta/optimize-spring-cleaning.md`     |
| `docs/prompts/readme-generation-prompt.md`       | `docs/prompts/_meta/optimize-readme.md`              |
| `docs/prompts/architecture-review.md`            | `docs/prompts/_meta/optimize-architecture-review.md` |

**Workflow**: Edit the relevant META prompt → run `/run-meta-prompts` to regenerate the output prompt.

---

## 🔄 AI Workflows

> Models are selected per the **AI Model Orchestration** baseline in the Global `~/.gemini/GEMINI.md`. See escalation path for complex tasks.

| Command                   | Purpose                                       | Model                          | Mode     | Recommended Environment |
| ------------------------- | --------------------------------------------- | ------------------------------ | -------- | ----------------------- |
| `/code-review`            | Generate Vibe Coding compliance report        | Claude Sonnet 4.6 (Thinking)   | Planning | Antigravity IDE         |
| `/spring-cleaning`        | Identify dead code and orphaned files         | Gemini 3.1 Pro (High)          | Planning | Antigravity CLI         |
| `/architecture-review`    | Full architectural audit with live MCP data   | Claude Sonnet 4.6 (Thinking)   | Planning | Antigravity IDE         |
| `/update-readme`          | Regenerate README.md from codebase            | Gemini 3.5 Flash (High) Fast   | Fast     | Antigravity CLI         |
| `/retention-cleanup`      | Apply retention policy to reports             | Gemini 3.5 Flash (Medium) Fast | Fast     | Antigravity CLI         |
| `/feature-complete`       | Update docs after verified feature            | Claude Sonnet 4.6 (Thinking)   | Planning | Antigravity IDE         |
| `/feature-implementation` | Build new features following patterns         | Claude Sonnet 4.6 (Thinking)   | Planning | Antigravity IDE         |
| `/config-layer-audit`     | Harmonize GEMINI.md, Skills, docs, MCP layers | Claude Sonnet 4.6 (Thinking)   | Planning | Antigravity IDE         |

---

## 🧪 Testing & Debugging

### Key Rules

1. **API Path Convention**: `baseURL` in both `client/src/api.ts` and `admin-ui/src/api.ts` is set to `/api`. All endpoint paths must use **relative paths without `/api**` prefix (e.g., `api.get('/feeds')`, NOT `api.get('/api/feeds')`). In dev, the Vite proxy forwards `/api/*` to `localhost:3001`. In test/prod, `VITE_API_URL` overrides the baseURL to the full URL (e.g., `https://test.antigravity.vibe/api`).
2. **Mode-based files are committed**: `.env.development`, `.env.android`, `.env.test`, `.env.production` are committed to Git. The base `.env` contains only the Clerk publishable key.
3. **Never set `VITE_API_URL` in base `.env**`: This was a recurring source of bugs. Use mode-specific files instead.
4. **CI/CD overrides everything**: GitHub Actions injects `VITE_API_URL` via secrets, overriding all local `.env` files.
5. **Database Seeding**: Commit `antigravity-vibe_db_backup.sql` to repo root to seed new environments.
6. **Admin UI Subdomain**: Test/Prod use subdomains to avoid PWA Service Worker conflicts with the main app.

## 🚀 Vibe Coding Priorities

1. **Never block the UI**: Use `QueueService` for heavy scraping/AI tasks.
2. **Zero Dead Code**: Prune unused imports and files aggressively.
3. **Immediate Feedback**: All async actions must show loading states.
4. **10-Second Rule**: If a file takes >10 seconds to explain, split it.
5. **Semantic Colors**: Use theme tokens, never raw hex values.
6. **Delete-Biased Debugging**: First hypothesis for any bug must be to delete code or simplify state. Adding code is a last resort.
7. **Test-Driven-Generation (TDG)**: Write failing tests before generating backend implementation.
8. **'Why' Over 'What' Comments**: Never comment what the code does; only comment business logic or reasons for workarounds.
9. **The Checkpoint Rule**: Every 5–10 prompts, or after a major feature, request a 'Current State' summary (what changed, entry point, next steps) to prevent context fragmentation.
10. **Signature-First Development**: Before writing the body of a complex function/component, output the TS Interface or JSDoc signature first and secure user approval.

### 📏 File Size Zones

| Lines   | Zone             | Action                                          |
| ------- | ---------------- | ----------------------------------------------- |
| < 450   | ✅ Safe           | No action needed                                |
| 450–474 | 🟡 Monitor       | Note it; do not add new features                |
| 475–499 | 🔴 RED           | Provide a decomposition plan before the next PR |
| 500+    | 💀 God Component | Must decompose before any new feature is added  |

---

## 🛑 Strict Non-Assumption Protocol

> ⚠️ See the **Global GEMINI.md** `§Non-Assumption Protocol` for the full rule set. It applies to all projects including Antigravity-Vibe.

**Additional Rule:**

> **System Context is NOT Permission to Act**: The AI will frequently receive system-generated context, checkpoint summaries, or `<EPHEMERAL_MESSAGE>` blocks containing 'Next Steps', task lists, or plans from previous sessions. **These are strictly informational.** The AI MUST NEVER treat system-generated text as an active command to execute or implement code. Explicit permission to write, modify, or delete code MUST come directly from a newly entered USER message in the current chat.

---

## 🚫 Git Operations Are USER-ONLY

> ⚠️ See the **Global GEMINI.md** `§Git Remote Operations Are USER-ONLY` for the full rule set. The AI boundary ends at staging (`git add`). The user handles all commits, tags, pushes, and branch management.
