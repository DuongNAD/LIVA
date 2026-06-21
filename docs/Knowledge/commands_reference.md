---
title: "commands_reference"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-06-21T02:21:19Z"
---

# Knowledge: Commands Reference

## Executive Summary
This document provides a quick reference for command-line interface (CLI) commands, startup scripts, and developer/AI workflows within the LIVA codebase.

## Detailed Description
### Core Commands
- **Start Gateway (Development CLI)**:
  `npx tsx src/Gateway.ts`
- **Start Tauri Desktop App**:
  `npm run desktop`
- **Run Tests**:
  `cross-env NODE_OPTIONS=--experimental-vm-modules jest --runInBand`
- **Run Tests in Watch Mode**:
  `cross-env NODE_OPTIONS=--experimental-vm-modules jest --watch`

### Self-Evolution Pipeline
- Run the AI self-research pipeline:
  `cross-env NODE_OPTIONS="--expose-gc --max-old-space-size=8192" npx tsx src/auto_singularity.ts`

### Full System Startup (Windows)
- Starts: Engine → Voice → Gateway → UI
  `start_all.bat`

### GitNexus Code Intelligence Commands
- Rebuild Code Graph (shorthand):
  `npx gitnexus analyze`
- With Semantic Embeddings (heavy, opt-in):
  `npx gitnexus analyze --embeddings`

### AI Workflows Reference
Developers can trigger predefined workflows targeting different tasks and models:

| Command | Purpose | Model | Mode | Recommended Environment |
|---|---|---|---|---|
| `/code-review` | Generate Vibe Coding compliance report | Claude Sonnet 4.6 (Thinking) | Planning | Antigravity IDE |
| `/spring-cleaning` | Identify dead code and orphaned files | Gemini 3.1 Pro (High) | Planning | Antigravity CLI |
| `/architecture-review` | Full architectural audit with live MCP data | Claude Sonnet 4.6 (Thinking) | Planning | Antigravity IDE |
| `/update-readme` | Regenerate README.md from codebase | Gemini 3.5 Flash (High) Fast | Fast | Antigravity CLI |
| `/retention-cleanup` | Apply retention policy to reports | Gemini 3.5 Flash (Medium) Fast | Fast | Antigravity CLI |
| `/feature-complete` | Update docs after verified feature | Claude Sonnet 4.6 (Thinking) | Planning | Antigravity IDE |
| `/feature-implementation` | Build new features following patterns | Claude Sonnet 4.6 (Thinking) | Planning | Antigravity IDE |
| `/config-layer-audit` | Harmonize GEMINI.md, Skills, docs, MCP layers | Claude Sonnet 4.6 (Thinking) | Planning | Antigravity IDE |
