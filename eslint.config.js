import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
const __dirname = dirname(fileURLToPath(import.meta.url));

export default tseslint.config(
  {
    ignores: ["eslint.config.js"]
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    languageOptions: {
      parserOptions: {
        project: true,
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
