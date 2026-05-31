---
description: Update documentation and create a user guide after verifying a new feature
---

# Feature Completion & Documentation Update

Execute this workflow when a feature implementation has been verified and is ready for release.

> **Source of Truth Principle**: All documentation updates in this workflow must be driven by **what exists in the codebase** (e.g., `package.json` files, model definitions, config files), not by what the AI remembers from the conversation. Verify all version numbers, file paths, and tech references against actual source files before writing.

## 1. Identify the Completed Feature
Review the recent conversation history, `task.md`, and `implementation_plan.md` to identify the specific feature that was just completed.
*   **Goal**: Ensure you have a clear understanding of the feature's name, purpose, and impact.

## 2. Update Project Management Artifacts
Reflect the completion status in the core project tracking files.
*   **`implementation_plan.md`**: Mark all relevant steps as `[x]` (Completed). Add a final note in the verification section if needed.
*   **`task.md`**: Mark the top-level task and sub-tasks as `[x]`.

## 3. Update Project Documentation
Update the high-level project documentation to keep the roadmap and status in sync.
*   **`docs/project/backlog.md`**:
    *   Move the text describing this feature from the "Backlog" or "In Progress" section to "Completed" (or delete it if the file convention prefers deletion).
    *   **Technical Debt**: Check the "Technical Debt" and "Active" sections — mark any items resolved by this feature as `[x]`.
*   **`docs/project/feature-status.md`**: Update the status of the feature to "Completed" / "Live". Update version numbers if applicable.
*   **`docs/project/roadmap.md`**: Check off the milestone or item corresponding to this feature.

## 4. Update Project Rules & Modular Skills (If Applicable)
If the feature changed any of the following, update the global/workspace rules or the modular skill set:
*   **Architecture or stack** (new service, new dependency, version change) → Update `GEMINI.md`.
*   **Critical files** (new model, new service, new config file) → Update `GEMINI.md` references.
*   **Reusable capabilities or integrations** (new background job queues, platform/Capacitor plugins, native Android OAuth flow, feed selectors, security middleware) → **MUST** update the relevant `.skills/` directory (or create a new skill directory under `.skills/` with its `SKILL.md`, `scripts/`, `examples/`, and `resources/` following the modular 2.0 standard).
*   **Common gotchas or naming conventions** → Update `GEMINI.md` common gotchas section.

## 5. Update Knowledge Base & Other Docs
*   Scan for other relevant documentation (e.g., architecture docs, API references) that might be outdated by this change and update them.
*   *Critically*: Ensure that no "knowledge items" (KIs) contradict the new reality of the codebase.

## 6. Update Database ER Diagram (If Applicable)
If the feature involved **any database schema changes**, update `docs/architecture/database-er-diagram.md`.

### 6.1 Detecting Database Changes
Review the feature implementation for:
*   **New tables** added via Sequelize models in `server/models/`
*   **New columns** added to existing models
*   **Changed relationships** (foreign keys, associations)
*   **New or modified migrations** in `server/migrations/`

### 6.2 Updating the Diagram
If changes are detected, update the Mermaid ER diagram:
1.  **Add new entities** using the existing format:
    ```mermaid
    ENTITY_NAME {
        type columnName PK/FK
        ...
    }
    ```
2.  **Add new relationships** using the notation:
    *   `||--o{` = one-to-many
    *   `||--||` = one-to-one
    *   `|o--o|` = zero-to-one on both sides
3.  **Update the "Key Architectural Patterns" section** if the change introduces a new pattern worth documenting.

### 6.3 Verification
*   Confirm the Mermaid diagram renders correctly (preview in VS Code or GitHub).
*   Ensure all FK references in the diagram match the actual model associations.
*   **Live schema check** (MCP): Run `mcp_postgres_query` against `information_schema.columns` to confirm column names and types match the diagram:
    ```sql
    SELECT table_name, column_name, data_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
    ORDER BY table_name, ordinal_position;
    ```
    Flag any discrepancies between the query output and the Mermaid ER diagram before marking complete.

## 7. Create User Guide (If User-Facing)
**Skip this step** if the feature is purely infrastructure, refactoring, dependency updates, or CI/CD changes with no user-visible impact.

For user-facing features, create a new guide file in `docs/guides/` (e.g., `docs/guides/how-to-use-[feature-name].md`).
This guide must be formatted in clean, human-readable Markdown and include:
*   **Purpose**: A clear explanation of *why* this feature exists and what problem it solves.
*   **Usage Instructions**: Step-by-step guide on how to use it, including screenshots or code snippets if applicable.
*   **Key Considerations**: "Gotchas," constraints, best practices, or things to keep in mind when using it.

## 8. Update CHANGELOG.md
Review the code changes made for this feature and update `CHANGELOG.md` in the project root.

### 8.1 Handle `[Unreleased]` Section
Check if `CHANGELOG.md` already has an `[Unreleased]` section with accumulated entries:
*   **If entries exist under `[Unreleased]`**: Consolidate them into the new version section. Do not lose or duplicate them.
*   **If no `[Unreleased]` section exists**: Create the version section directly.

### 8.2 Determine Version Impact

> [!IMPORTANT]
> **Default: always bump PATCH** (z in x.y.z). The AI must never autonomously choose a MINOR or MAJOR bump.

*   **PATCH** (x.y.**Z+1**): The default for every `/feature-complete` run — bug fixes, new features, tweaks, migrations, documentation changes. All changes are PATCH unless the user explicitly says otherwise.
*   **MINOR** (x.**Y+1**.0): Only when the **user explicitly instructs** a minor version bump.
*   **MAJOR** (**X+1**.0.0): Only when the **user explicitly instructs** a major version bump.

### 8.3 Review Code Changes
Execute local read-only Git terminal commands to generate an objective list of local changes:
1.  Run `git log vX.Y.Z-1..HEAD` to list all local commits included in this release.
2.  Run `git diff vX.Y.Z-1..HEAD --name-status` to enumerate changed, added, and deleted files.
3.  Scan the diff output to identify:
    *   **Added**: New features, endpoints, UI components, or capabilities
    *   **Changed**: Modified behavior, refactored code, updated dependencies
    *   **Fixed**: Bug fixes resolved as part of this feature
    *   **Deprecated**: Features or patterns marked for future removal
    *   **Migration**: Any database migrations that must run (include filename and brief description)

### 8.4 Write the Entry
Replace the `[Unreleased]` header (if present) with the version section, following [Keep a Changelog](https://keepachangelog.com/) format:

```markdown
## [x.y.z] - YYYY-MM-DD

### Added
- **Feature Name** — Brief description of what was added

### Changed
- **Component Name** — What changed and why

### Migration
- **Migration NNN**: `NNN_migration_name.ts` — Brief description
```

### 8.5 Version Bump (If Applicable)
If the version number increased, the version sync script MUST be run to ensure environments do not drift.

// If your project uses a version sync script, run it:
```powershell
node scripts/update-version.js X.Y.Z [versionCode]
```

This automated script will cleanly update:
*   `package.json` across all workspaces
*   Any platform-specific version files (e.g., `build.gradle` for Android)
*   `README.md` and `docs/project/backlog.md` texts

> **Android note**: `versionCode` in `build.gradle` must **always increment** for each release submitted to Google Play, calculated using the mandatory 2.0 formula: `versionCode = major * 10000 + minor * 100 + patch` (e.g. `v2.8.19` yields `20819`). If unsure of the next code, check the current value in `client/android/app/build.gradle`.

## 9. Final Verification
*   Verify that all links added between documents are valid.
*   Confirm `docs/guides` contains the new file (if a user guide was created in step 7).
*   Confirm `CHANGELOG.md` has been updated with the new version entry.
*   Confirm version numbers are consistent across all `package.json` files and `build.gradle`.
*   If the ER diagram was updated (step 6), confirm it renders correctly.
*   Run the workspace type check from the root directory using the npm workspace filter, building shared types first to verify zero type-safety regression:
    ```powershell
    npm run build -w <shared-types-package>; npm run type-check -w server
    ```
*   **Tag listing**: Run `git tag --list` in the terminal to confirm the previous release tag is visible and the new version does not conflict with an existing tag.
*   **Issue cross-reference** (GitHub MCP): Use the `call_mcp_tool` wrapper with `ServerName: "github-mcp-server"` and `ToolName: "list_issues"` (state: open) to check if any open issues were resolved by this feature. If so, add a note in the CHANGELOG entry and consider using `add_issue_comment` to link the resolved issue to the relevant commit or PR.

## 10. Commit, Tag & Push (USER ACTION — Required)

> [!IMPORTANT]
> The AI's git boundary ends at staging. **All steps below are USER-ONLY.** The AI must explicitly call these out at the end of every `/feature-complete` run so the user knows exactly what to execute.

After the AI completes all documentation updates and stages all files, the user must run the following three commands in order:

```bash
# 1. Commit all staged changes
git commit -m "docs: feature-complete vX.Y.Z — <Feature Name>"

# 2. Create the release tag (starting from v2.8.10 onwards)
git tag vX.Y.Z

# 3. Push the commit AND the tag together
git push origin main --tags
```

**Why the tag matters:**
- Creates a permanent, named snapshot of the exact code shipped
- Enables accurate `git diff vX.Y.Z-1..vX.Y.Z` diffs in future `/feature-complete` runs (step 8.3)
- Foundation for GitHub Releases and Play Store build traceability

> **Android note**: If this release targets the Play Store, also push the tag explicitly:
> ```bash
> git push origin vX.Y.Z
> ```
> GitHub Actions can be configured to trigger production Android builds on tag pushes.

