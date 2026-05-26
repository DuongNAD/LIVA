import { defineConfig } from "vitest/config";
import * as path from "node:path";

export default defineConfig({
  resolve: {
    alias: {
      "@skills": path.resolve(__dirname, "src/skills"),
      "@utils": path.resolve(__dirname, "src/utils"),
      "@core": path.resolve(__dirname, "src/core"),
      "@memory": path.resolve(__dirname, "src/memory"),
      "@security": path.resolve(__dirname, "src/security"),
      "@mcp": path.resolve(__dirname, "src/mcp"),
      "@services": path.resolve(__dirname, "src/services"),
      "@evolution": path.resolve(__dirname, "src/evolution"),
      "@sandbox": path.resolve(__dirname, "src/sandbox"),
      "@deployment": path.resolve(__dirname, "src/deployment"),
    },
  },
  test: {
    // Test files pattern — mirrors src/ structure
    include: ["tests/**/*.test.ts"],
    // Timeout per test
    testTimeout: 30000,
    hookTimeout: 30000,
    // Limit forks pool size to prevent CPU/memory exhaustion and worker crashes
    maxWorkers: process.env.CI ? 2 : 3,
    minWorkers: 1,
    // Use forks for stability, threads for coverage (see package.json scripts)
    pool: "threads",
    // Force exit workers after 30s to prevent open handle deadlocks, allowing coverage generation
    teardownTimeout: 30000,
    setupFiles: ["./tests/setup.ts"],
    // Reporter
    reporters: ["verbose"],
    // Coverage Configuration (KPI: 70% overall, 95% security/memory)
    coverage: {
      provider: "istanbul",
      reportsDirectory: "./coverage",
      reporter: ["text-summary", "lcov"],
      // Generate coverage report even when threshold checks fail
      reportOnFailure: true,
      // Limit concurrency to prevent V8 fragment merge deadlock
      processingConcurrency: 4,
      include: ["src/**/*.ts"],
      exclude: [
        "src/**/*.d.ts",
        "src/auto_singularity.ts",
        "src/services/**",  // External service wrappers (Google, Zalo API)
        "src/utils/auth_google_script.ts",
        "src/utils/googleAuth.ts",
        "src/utils/generate_skeleton.ts",
        // Skills relying on runtime-injected globals (getOrCreateBrowser, logger without import)
        "src/skills/web/WebBrowser.ts",
        "src/skills/web/ComputerUse.ts",
        "src/skills/web/GeminiSurfer.ts",
        "src/skills/social/SendZaloRPA.ts",
        "src/skills/social/SendMessengerRPA.ts",
        "src/servers/**",  // Server bootstrap files (VoiceRelay)
        // Utility files with hard system deps (CDP, Docker, Playwright)
        "src/utils/ChromeLauncher.ts",
        "src/utils/DockerSandbox.ts",
        "src/utils/PlaywrightBrowser.ts",
        "src/utils/HotRollback.ts",
        // [Audit V2] Tech Debt — exclude R&D / external protocol modules
        "src/mcp/**",         // MCP requires integration tests, not unit tests
        "src/evolution/**",   // Evolution is experimental
        "src/PluginSDK.ts",   // SDK interface definition
        "src/test_*.ts",      // Manual test scripts
        "src/core/index.ts",  // Barrel re-export file — 0% is acceptable
        // V8 coverage parse error: Rolldown cannot parse TS parameter types
        "src/channels/TelegramCommandHandler.ts",
      ],
      // KPI Thresholds — raised after Audit V2 improvements
      thresholds: {
        statements: 65,
        branches: 55,
        functions: 65,
        lines: 65,
      },
    },
  },
});
