import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Test files pattern — mirrors src/ structure
    include: ["tests/**/*.test.ts"],
    // Timeout per test
    testTimeout: 15000,
    // Use threads for speed
    pool: "threads",
    // Reporter
    reporters: ["verbose"],
    // Coverage Configuration (KPI: 85% overall, 95% security/memory)
    coverage: {
      enabled: false, // Enable with `vitest run --coverage`
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
      ],
      // KPI Thresholds
      thresholds: {
        lines: 85,
        branches: 80,
        functions: 85,
        statements: 85,
      },
    },
  },
});
