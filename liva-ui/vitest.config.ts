import { defineConfig } from 'vitest/config'
import viteConfig from './vite.config'

export default defineConfig({
  ...viteConfig,
  test: {
    environment: 'jsdom',
    environmentOptions: {
      jsdom: {
        url: 'http://localhost/',
      },
    },
    setupFiles: ['./tests/setup.ts'],
    globals: true,
    // Coverage Configuration
    coverage: {
      provider: 'istanbul',
      reportsDirectory: './coverage',
      reporter: ['text-summary', 'lcov', 'text'],
      reportOnFailure: true,
      include: ['src/**/*.ts', 'src/**/*.vue'],
      exclude: [
        'src/**/*.d.ts',
        'src/vite-env.d.ts',
        'src/main.ts',          // App bootstrap
        'src/App.vue',          // Root component (tested via integration)
        'src/router/**',        // Router config (tested via integration)
        'src/assets/**',        // Static assets
        'src/components/VisionSensor.vue', // Empty file causing compilation error
      ],
      // Ngưỡng này TRƯỚC NAY VÔ HIỆU: script test là `vitest run` (không kèm
      // `--coverage`), nên coverage không bao giờ được đo và ngưỡng không bao
      // giờ được áp — một "cổng xanh giả" đúng nghĩa. Từ 22/07/2026 CI chạy
      // `test:coverage`, nên đây thành cổng THẬT.
      //
      // Giá trị đặt HƠI THẤP hơn coverage đo thật ngày 22/07/2026
      // (stmts 62,9% · branch 45,8% · func 48,6% · lines 64,6%) để thành chốt
      // chống-thụt-lùi có headroom, không vỡ vì thay đổi nhỏ. Con số cũ 50/40/
      // 50/50 là số ước lệ chưa từng được kiểm: func 50 thực ra KHÔNG đạt
      // (48,6%), nên bật cổng mà giữ nguyên là CI đỏ ngay.
      thresholds: {
        statements: 60,
        branches: 43,
        functions: 46,
        lines: 62,
      },
    },
  }
})
