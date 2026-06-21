---
title: "liva_architecture"
tags:
  - liva/knowledge
  - liva/architecture
author: "worker"
last_update: "2026-06-21T02:21:19Z"
---

# Knowledge: LIVA System Architecture

## Executive Summary
This document outlines the core architecture of the LIVA agent system, showing the interaction between the LLM client, the MCP server, and the Obsidian Vault, including startup sequence, hybrid modes, platform support, and architectural boundaries.

## Detailed Description
### Design Philosophy
LIVA is a **hybrid-intelligence, multi-agent AI desktop assistant** designed for **mid-to-high-end hardware** on **Windows & macOS**. It dynamically routes between local AI inference (GPU) and cloud APIs to maximize performance and hardware efficiency.
- **Hybrid Intelligence**: Routes chat locally and complex tasks to cloud models dynamically.
- **Cross-Platform**: Windows 10/11 (CUDA/Vulkan) and macOS (Apple Silicon Metal).
- **Hardware-Adaptive**: `AutoGPUSetup` detects hardware at boot to auto-configure model size, context length, and thread count.
- **Lean Footprint**: Decoupled Tauri UI (OS WebView native, 30MB-50MB RAM) and Node.js gateway backend (Agent loop, FSM, SQLite, 93 Skills).
- **Architecture Modularity**: The system architecture separates UI rendering, gateway logic, and voice/AI processing.

### Startup Sequence (`npm run desktop` / Tauri Sidecar)
1. `liva-gateway` boots up and initializes `AutoGPUSetup` (Hardware detection & ConfigManager initialization).
2. `openclaw-gateway` → `ModelOrchestrator` performs **Adaptive Engine Selection**:
   - **Local Mode** (`AI_PROVIDER=local`): Spawns `llama-server` (C++ native, port 8000, auto-selects GPU layers based on VRAM).
   - **Cloud Mode** (`AI_PROVIDER=cloud`): Connects to OpenAI-compatible cloud API (Gemini, GPT, Claude, Groq, etc.).
   - **Hybrid Mode** (`AI_PROVIDER=hybrid`): Local for chat, cloud for complex reasoning.
3. `liva-ai-engine` → `voice_engine.py` (Edge-TTS) boots up (skipped on macOS if `ffmpeg` is unavailable).
4. `liva-ui` → Tauri v2 launches. It connects via WebSocket to port 8082.

### Platform Support
- **Windows 10/11 (x64)**: ✅ Primary. Full feature support, CUDA/Vulkan GPU.
- **macOS (Apple Silicon)**: ✅ Supported. Metal GPU, `llama-server` via Homebrew.
- **macOS (Intel)**: ⚠️ Limited. CPU-only inference, cloud mode recommended.
- **Linux (x64)**: 🔜 Planned. Community testing phase.
- **Platform Architecture**: The platform architecture details OS boundaries.

### Architectural Boundaries
- **/core layer NEVER calls database directly**: Must go through the `/memory` layer.
- **/skills are self-contained MCP tools**: Each exports `metadata` + `execute()`.
- **/security guards are applied at AgentLoop level**: Not individual skill level.
- **Remote Control Hub**: Ingress layer handling Telegram long-polling and Chrome DevTools Protocol (CDP) WebSocket connections to Antigravity IDE. Execution requests pass through `SecurityGateway` and `ApprovalEngine`.
- **Tauri Sidecar communication**: Gateway is a daemon process communicating with Tauri UI via a **Dynamic WS Handshake**. Standard output (`stdout`) is strictly guarded to only print a single JSON handshake event: `{event: "GATEWAY_READY", port: <dynamic>, token: <uuid>}`. All other logs are written to `stderr` via Pino.

### Architecture Components Detailed List
1. Ingress Architecture: The ingress architecture handles incoming connections.
2. Gateway Architecture: The gateway architecture manages internal state.
3. Database Architecture: The database architecture ensures clean persistence.
4. Voice Architecture: The voice architecture coordinates audio input/output.
5. Security Architecture: The security architecture guards all actions.
6. Execution Architecture: The execution architecture handles task routing.
7. Model Architecture: The model architecture supports adaptive routing.
8. Memory Architecture: The memory architecture uses a multi-layered brain.
9. Sidecar Architecture: The sidecar architecture handles WebSockets.
10. UI Architecture: The UI architecture renders components.
