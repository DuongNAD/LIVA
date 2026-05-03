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
    testTimeout: 15000,
    // Use forks instead of threads to prevent native addon segmentation faults
    pool: "forks",
    setupFiles: ["./tests/setup.ts"],
    // Reporter
    reporters: ["verbose"],
    // Coverage Configuration (KPI: 70% overall, 95% security/memory)
    coverage: {
      provider: "v8",
      reportsDirectory: "./coverage",
      reporter: ["text", "text-summary", "html", "lcov"],
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
        "src/evolution/**",   // R&D pipeline — separate tech debt ticket
        "src/mcp/**",         // MCP requires integration tests, not unit tests
        "src/core/index.ts",  // Barrel re-export file — 0% is acceptable
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
