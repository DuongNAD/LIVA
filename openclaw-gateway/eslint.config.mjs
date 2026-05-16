import tseslint from 'typescript-eslint';

export default tseslint.config(
  ...tseslint.configs.recommended,
  {
    ignores: [
      "dist/",
      "node_modules/",
      "tests/",
      "src/evolution/",
      "coverage/",
      "vitest.config.ts",
      "src/test_*.ts",
      "src/utils/auth_google_script.ts",
      "src/utils/generate_skeleton.ts",
      "scripts/",
      "watchdog.js",
      "test_*.ts",
      "*.cjs",
      "build-sea.js"
    ]
  },
  {
    rules: {
      "no-console": "error",
      "@typescript-eslint/no-explicit-any": "warn",
      "@typescript-eslint/no-unused-vars": ["warn", { "argsIgnorePattern": "^_" }],

      // [Phase 4] Banned imports — AI_CONTEXT §12 enforcement
      // NOTE: electron is ALLOWED in openclaw-gateway (for liva-ui IPC)
      "no-restricted-imports": ["error", {
        "paths": [
          { "name": "@xenova/transformers", "message": "BANNED: Use EmbeddingService → GPU /v1/embeddings" },
          { "name": "@huggingface/transformers", "message": "BANNED: CPU Tensor blocks Event Loop. Use llama-server /v1/embeddings" },
          { "name": "@lancedb/lancedb", "message": "BANNED: Use sqlite-vec within node:sqlite" },
          // NOTE: electron is allowed in openclaw-gateway for liva-ui Electron IPC
          { "name": "axios", "message": "BANNED: Use safeFetch() from src/utils/HttpClient.ts" },
          { "name": "puppeteer", "message": "BANNED: Use playwright-core (2MB, API only)" },
          { "name": "request", "message": "BANNED: Use safeFetch() from src/utils/HttpClient.ts" },
          { "name": "got", "message": "BANNED: Use safeFetch() from src/utils/HttpClient.ts" },
          { "name": "node-fetch", "message": "BANNED: Use safeFetch() from src/utils/HttpClient.ts" },
          { "name": "fuse.js", "message": "BANNED: Use FTS5 (SQLite)" },
          { "name": "sqlite3", "message": "BANNED: Use native node:sqlite (built-in)" },
          { "name": "sqlite", "message": "BANNED: Use native node:sqlite (built-in)" },
          { "name": "node-llama-cpp", "message": "BANNED: Native C++ bindings → Segfault risk. Use llama-server HTTP API" }
        ]
      }]
    }
  }
);
