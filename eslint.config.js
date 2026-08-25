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
      "scripts/**/*",
      // Workspace cục bộ không nằm trong git (xem .gitignore): engine Python/
      // llama.cpp thế hệ cũ đã bị thay bởi liva-native-core, và bản mô phỏng
      // MVC bỏ dở. Bên trong là venv, clone llama.cpp và build output — lint
      // chúng chỉ sinh lỗi giả cho ai chạy `eslint .` từ gốc repo.
      "liva-ai-engine/**/*",
      "mvc-simulation/**/*",
      "target/**/*"
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

      // Đã dọn 22/07/2026: 74 chỗ `any` tích tụ trong 13 file .vue suốt thời
      // gian chúng không hề được lint. Nay còn đúng 2 chỗ, mỗi chỗ có
      // `eslint-disable-next-line` kèm lý do — siết kiểu ở đó sẽ buộc phải sửa
      // logic, mà đợt dọn này cố ý chỉ đụng annotation.
      //
      // Vì sao dám dọn hàng loạt: kiểu TypeScript bị XOÁ lúc biên dịch, nên
      // đổi annotation không thể đổi hành vi lúc chạy. Đã chứng minh bằng cách
      // dựng `vite build` cả trước lẫn sau rồi so JS xuất ra — **19/19 file
      // giống hệt từng byte** sau khi chuẩn hoá hash chunk và scope-id (hai
      // thứ dẫn xuất máy móc từ nội dung nguồn).
      "@typescript-eslint/no-explicit-any": "error",
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
