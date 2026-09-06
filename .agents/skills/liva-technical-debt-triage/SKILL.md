---
name: liva-technical-debt-triage
description: Prioritize and safely reduce LIVA technical debt using vault evidence, GitNexus impact analysis, and explicit acceptance tests. Use when reviewing debt backlogs, choosing the next cleanup, assessing old branches, removing stale instructions, or preparing a bounded maintenance change.
---

# LIVA Technical Debt Triage

## Workflow

1. Capture `git status --short`, the current branch, and existing dirty files. Read the active debt backlogs and call Obsidian `search_vault` for the affected subsystem and its rules.
2. Exclude candidates that would overwrite unrelated dirty work, restore the retired Node/Python core, require an unresolved product decision, or merge an old branch wholesale.
3. Explore unfamiliar code with GitNexus `query`, inspect candidate symbols with `context`, and run upstream `impact` before changing any existing function, class, or method.
4. Rank at most three candidates by security or data-loss risk, beta-user impact, existence of a deterministic acceptance test, blast radius, and implementation cost.
5. For the selected item, record the expected behavior and validation command before editing. Warn and stop for user approval when GitNexus reports HIGH or CRITICAL risk.
6. Implement one bounded item, run its focused tests and mandatory repository gates (e.g., `cargo check --workspace`, `cargo test -p liva-native-core`, `npm test` in `liva-ui`), verify scorecard impact via `node scripts/scorecard.mjs`, and run GitNexus `detect_changes` to verify the affected symbols and flows.
7. Report remaining debt separately from completed work. Follow the repository Git boundary: stage only authorized files; do not commit, merge, push, pull, fetch, tag, or delete branches.

## Triage output

For each candidate, provide:

- evidence and source path;
- risk and blast radius;
- acceptance command;
- disposition: selected, deferred, blocked, or obsolete.

Do not call debt resolved without fresh command output proving its acceptance condition.
