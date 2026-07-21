import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import vueParser from "vue-eslint-parser";

import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
const __dirname = dirname(fileURLToPath(import.meta.url));

export default tseslint.config(
  {
    ignores: [
      "eslint.config.js",
      "**/check_db.mjs",
      "**/test-dreaming-e2e.ts",
      "**/test_skills_comprehensive.mjs",
      "**/test_skills_live.mjs",
      "liva-ui/tests/**/*",
      "liva-ui/uno.config.ts",
      "liva-ui/vite.config.ts",
      "liva-ui/vitest.config.ts",
      "liva-ui/dist/**/*",
      "liva-ui/public/assets/**/*",
      "**/tests/**/*",
      "**/coverage/**/*",
      "**/*.config.*",
      "mobile_client/dist/**/*",
      "liva-desktop/dist/**/*",
      "**/*.js",
      "**/*.cjs",
      "**/*.mjs",
      "teamwork_projects/**/*",
      ".gitnexus/**/*",
      // Thư mục nháp của công cụ agent (không được git theo dõi). Chứa file
      // `proposed_*.vue` không thuộc tsconfig nào, nên từ khi bật parser SFC
      // (22/07/2026) chúng làm `eslint .` chạy từ gốc repo báo lỗi parse — kể
      // cả khi mã nguồn thật hoàn toàn sạch.
      ".agents/**/*",
      "scripts/**/*"
    ]
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  // File .vue KHÔNG được lint trước 22/07/2026: config không có parser cho SFC,
  // nên eslint bỏ qua toàn bộ 22 component. Nghĩa là ba quy tắc chặn của dự án
  // (no-console, cấm fetch thuần, cấm fs*Sync) không có hiệu lực ở đúng nơi
  // chứa phần lớn mã giao diện — dù CLAUDE.md ghi là "enforced by ESLint".
  //
  // Ở đây CHỈ thêm parser, KHÔNG bật bộ quy tắc style của eslint-plugin-vue:
  // mục tiêu là đóng lỗ hổng chặn, không phải mở một đợt sửa style.
  {
    files: ["**/*.vue"],
    languageOptions: {
      parser: vueParser,
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: [".vue"],
        sourceType: "module",
      },
    },
    rules: {
      // TypeScript tự bắt biến chưa khai báo, và `no-undef` không biết các
      // global của trình duyệt (document, setTimeout, localStorage). Đây đúng
      // là điều typescript-eslint tự làm cho file .ts.
      "no-undef": "off",

      // Nợ có sẵn: 74 chỗ dùng `any` trong 22 file .vue, tích tụ suốt thời
      // gian các file này không hề được lint. Bật thành lỗi ngay sẽ chặn CI và
      // ép một đợt sửa 74 chỗ — mỗi chỗ đều có khả năng đổi hành vi lúc chạy.
      //
      // Đánh đổi có chủ ý: tắt riêng quy tắc này để BA QUY TẮC CHẶN của dự án
      // (no-console, cấm `fetch` thuần, cấm `fs*Sync`) có hiệu lực ở file .vue
      // NGAY BÂY GIỜ — chúng mới là thứ bảo vệ người dùng. Đo ngày 22/07/2026:
      // 0 vi phạm ba quy tắc đó, tức lâu nay vẫn được tuân thủ bằng tay.
      //
      // Gỡ dòng này sau khi dọn xong `any`.
      "@typescript-eslint/no-explicit-any": "off",
    },
  },
  {
    languageOptions: {
      parserOptions: {
        project: [
          "./liva-ui/tsconfig.app.json",
          "./liva-ui/tsconfig.node.json",
          "./packages/liva-common/tsconfig.json",
          "./mobile_client/tsconfig.json",
          "./liva-desktop/tsconfig.json"
        ],
        tsconfigRootDir: __dirname,
      },
    },
    rules: {
      "no-console": "error",
      "no-restricted-syntax": [
        "error",
        {
          "selector": "CallExpression[callee.name='fetch']",
          "message": "CRITICAL: BANNED. Native fetch swallows 500 errors. Use safeFetch() instead!"
        },
        {
          "selector": "CallExpression[callee.object.name='fs'][callee.property.name=/.*Sync$/]",
          "message": "CRITICAL: BANNED. Synchronous I/O blocks the Event Loop. Use fs.promises."
        }
      ]
    }
  }
);
