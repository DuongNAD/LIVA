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
        // A31-06: tổng số đẹp từng che ba đường UI rủi ro cao. Giữ chốt
        // per-file để reconnect/command transport/vision không âm thầm tụt lại
        // dưới mức đã nghiệm thu ngày 31/07/2026.
        //
        // ⚠️ RE-BASE 07/08/2026, đọc trước khi coi đây là tiền lệ hạ ngưỡng.
        //
        // Bánh cóc per-file **phạt đúng cuộc tái cấu trúc mà A31-04 cần**. Khi
        // bóc một khối ra composable, phần code chuyển đi thường được phủ TỐT
        // HƠN trung bình của file, nên phần ở lại — vốn phủ kém — chiếm tỷ
        // trọng lớn hơn và tỷ lệ tụt, dù không dòng nào mất test.
        //
        // Đo ở lát 3 (`useWidgetWindow.ts`): khối bóc ra phủ **93,58 %**,
        // `WidgetApp.vue` tụt 83,71 → 81,72, còn **tổng gần như đứng yên**
        // (80,88 → 80,84). Không có hồi quy nào, chỉ có phép đo bị lệch.
        //
        // Cách xử: cho bánh cóc **đi theo code**. Hạ chốt của file bị bóc về
        // mức thật, ĐỒNG THỜI đặt chốt mới cho file nhận, để tổng mức bảo vệ
        // không giảm mà chỉ dời chỗ. Lát sau lặp lại đúng công thức này.
        //
        // Cái KHÔNG được làm: hạ chốt mà không thêm chốt bù. Đó mới là dập cổng.
        'src/WidgetApp.vue': {
          lines: 81,
        },
        // Chốt bù cho lát 3 — khối hình học cửa sổ vừa rời WidgetApp.vue.
        'src/composables/useWidgetWindow.ts': {
          lines: 90,
        },
        'src/composables/useGateway.ts': {
          lines: 50,
        },
        'src/components/dashboard/VisionView.vue': {
          lines: 50,
        },
        'src/components/dashboard/MemoryViewer.vue': { functions: 50 },
        'src/components/dashboard/SettingsView.vue': { functions: 50 },
        'src/components/dashboard/TaskManager.vue': { functions: 50 },
        'src/composables/useSpeakerPlayback.ts': { functions: 50 },
      },
    },
  }
})
