---
title: "environment_variables"
tags:
  - liva/rule
author: "worker"
last_update: "2026-06-21T02:21:19Z"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Environment Variables

## Rule Statement
Sensitive keys in the gateway must not be kept in plaintext inside `.env`. The system dynamically encrypts keys from `.env` into `liva_vault.json` using AES-256-GCM and decrypts them dynamically into `process.env` at boot.

## Rationale
To ensure Zero-Trust credentials security (Shift-Left approach) and prevent key leaks from plain text environment files during development, sharing, or version control.

## Environment Variables Reference Listing
The following variables are loaded at boot by the `ConfigManager`:

- **LIVA_ENCRYPTION_KEY**: [REQUIRED] 32-byte key for operating the AES-256-GCM `EncryptionEngine`.
- **LIVA_KERNEL_SECRET**: [OPTIONAL] Fallback UUID for internal kernel communication.
- **AI_PROVIDER**: Router strategy. Value: `local` (GGUF via llama-server), `cloud` (OpenAI-compatible API), or `hybrid`.
- **AI_BASE_URL**: Cloud API endpoint (when `AI_PROVIDER` is cloud/hybrid).
- **AI_API_KEY**: Cloud API key.
- **AI_MODEL**: Cloud model name (e.g. gemini-2.5-flash).
- **AI_MODELS_DIR**: Local model directory (default: `~/.liva/models`).
- **ROUTER_MODEL_NAME**: Fast GGUF model name for Intent Routing.
- **EXPERT_MODEL_NAME**: Heavy GGUF model name for Deep Reasoning.
- **LLM_ENDPOINT**: Override LLM API base (default: `http://localhost:8000/v1/chat/completions`).
- **ZALO_OA_ACCESS_TOKEN**: Zalo Bot Creator token (contains `:`).
- **ZALO_USER_ID**: Auto-detected User ID on first message.
- **TAVILY_API_KEY**: Web search API key (falls back to DDG).
- **LIVA_GEOLOCATION_ENABLED**: "true" to enable IP geolocation lookup on boot.
- **EMAIL_HOST** / **EMAIL_PORT** / **EMAIL_USER** / **EMAIL_PASS**: Email IMAP configuration.
- **LIVA_USE_NATIVE**: "true" to use gRPC native engine.
- **LIVA_TTS_ENGINE**: "python" (Edge-TTS) or "kokoro" (offline fallback).
- **NEMOTRON_MODEL_DIR**: ASR model path (default: `./models/nemotron-asr`).
- **NEMOTRON_LANGUAGE**: STT language (default: `vi`).
- **NEMOTRON_CHUNK_MS**: Streaming ASR chunk duration (default: `160`).
- **FF_DISABLE_L2_INJECTION**: "true" to disable L2 semantic memory injection.

## Decryption and Load Flow
1. **Boot Initialization**: `EncryptionEngine` runs immediately before the Event Loop starts.
2. **Auto-Migration**: Detects sensitive plain-text variables (e.g., `AI_API_KEY`, `ZALO_OA_ACCESS_TOKEN`) in `.env`.
3. **Encryption & Vault Write**: Encrypts these secrets with AES-256-GCM using `LIVA_ENCRYPTION_KEY`, writes to `liva_vault.json`, and scrubs the plaintext secrets from `.env`.
4. **Environment Load**: Loads the decrypted keys directly into memory (`process.env`) for runtime access.

## Exceptions
- Plaintext credentials are allowed in `.env` only during first-time setup before initial boot auto-migration encrypts them.
- Synchronous reading/writing for Vault auto-migration is allowed only during early boot sequence before the main Event Loop starts.
