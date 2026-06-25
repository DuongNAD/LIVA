import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
const __dirname = dirname(fileURLToPath(import.meta.url));

export default tseslint.config(
  {
    ignores: [
      "eslint.config.js",
      "liva-gateway/check_db.mjs",
      "liva-gateway/test-dreaming-e2e.ts",
      "liva-gateway/test_skills_comprehensive.mjs",
      "liva-gateway/test_skills_live.mjs",
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
      "**/*.config.*"
    ]
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    languageOptions: {
      parserOptions: {
        project: [
          "./liva-ui/tsconfig.app.json",
          "./liva-ui/tsconfig.node.json",
          "./packages/liva-common/tsconfig.json"
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
