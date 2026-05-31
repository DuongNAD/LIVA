import { defineConfig } from 'vitest/config'
import viteConfig from './vite.config'

export default defineConfig({
  ...viteConfig,
  test: {
    environment: 'jsdom',
    setupFiles: ['./tests/setup.ts'],
    globals: true,
    // Coverage Configuration
    coverage: {
      provider: 'istanbul',
      reportsDirectory: './coverage',
      reporter: ['text-summary', 'lcov'],
      reportOnFailure: true,
      include: ['src/**/*.ts', 'src/**/*.vue'],
      exclude: [
        'src/**/*.d.ts',
        'src/vite-env.d.ts',
        'src/main.ts',          // App bootstrap
        'src/App.vue',          // Root component (tested via integration)
        'src/router/**',        // Router config (tested via integration)
        'src/assets/**',        // Static assets
      ],
      thresholds: {
        statements: 50,
        branches: 40,
        functions: 50,
        lines: 50,
      },
    },
  }
})
