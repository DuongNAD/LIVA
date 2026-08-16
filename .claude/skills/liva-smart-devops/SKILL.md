---
name: liva-smart-devops
description: Diagnose CI/CD build failures, analyze container runtime logs, troubleshoot Docker and Kubernetes configurations, and propose safe remediation steps. Use when debugging pipeline crashes, checking container health, triaging infrastructure errors, or planning deployment rollbacks.
---

# LIVA Smart DevOps

## Workflow

1. **Incident Triage & Log Inspection**:
   - Collect raw execution logs from failing CI/CD jobs (e.g., GitHub Actions, cargo build logs) or container runtimes (`docker logs --tail 200 <container_id>`).
   - Isolate exact error lines, exit codes, panic traces, and dependency resolution failures using regex pattern matching.
   - Separate transient infrastructure errors (network timeouts, rate limits, lock contention) from deterministic code or configuration errors.

2. **Environment & Container State Diagnostics**:
   - Check status of running containers, open ports, and healthcheck probes (`docker ps -a`, `docker inspect`).
   - Validate configuration files (`Dockerfile`, `docker-compose.yml`, `.github/workflows/*.yml`, `Cargo.toml`).
   - Verify environment variable availability and check for missing secrets without leaking plaintext credential values.

3. **Blast Radius & Code Impact Analysis**:
   - Run GitNexus impact analysis before proposing configuration or build file modifications:
     - `gitnexus_impact({target: "config_or_module", direction: "upstream"})`
   - Evaluate whether dependency upgrades or pipeline changes introduce breaking changes across dependent services.

4. **Root Cause Analysis & Remediation Formulation**:
   - Formulate the minimal actionable remediation patch:
     - Dependency lockfile mismatch -> Pin compatible crate/package version.
     - OOM / Resource starvation -> Adjust container memory limits or concurrency threads.
     - Port collision / Network disconnect -> Reconfigure internal Docker bridge network bindings.
     - Flaky / Concurrency test failure -> Fix race conditions or isolate shared database states.

5. **Two-Phase Confirmation for Mutating Actions**:
   - **Read-Only Diagnostics**: Run non-destructive inspection commands (`docker ps`, `docker logs`, `git status`, `cargo test --no-run`) directly.
   - **Destructive / Mutating Actions**: Present a structured plan and request explicit user confirmation before executing state-changing commands (`docker compose down`, `docker stop`, `docker rm`, container pruning, or infrastructure deployments).
   - **Git Invariant**: Strictly adhere to the Git boundary (`git add` is permitted for staging; `git commit`, `git push`, `git checkout -b`, and `git merge` are USER-ONLY actions).

6. **Post-Mortem Documentation**:
   - Record incident findings and prevention runbooks into `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/DevOps - <Incident_Title>.md` using `write_markdown`.
   - Adhere strictly to the Obsidian frontmatter standard (`title`, `tags: [liva/knowledge, liva/devops, ops/incident]`, `author: "codex"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Strict Two-Phase Confirmation for mutations; Auto-execution for read-only status and log inspections.
- **Tool Dependencies**: Requires shell execution harness, `gitnexus` MCP tools (`impact`, `detect_changes`), and `obsidian` MCP tools (`write_markdown`, `search_vault`).
- **Operating Environment**: Compatible with Windows PowerShell and Linux Bash container runtimes. Commands must avoid interactive prompts (`-y` flags where appropriate on non-destructive commands).
- **Prohibited Remote Git Operations**: Autonomous execution of `git push`, `git pull`, `git commit`, or remote tag management is strictly forbidden.

## Stop Conditions

Stop and report immediately when:
- A proposed remediation requires restarting production database containers with potential data loss.
- An infrastructure command requests root access, sudo escalation, or credential extraction.
- Git remote push or destructive branch manipulation is implied or requested autonomously.
- Container diagnostic logs contain unmasked secrets, private keys, or API tokens.
