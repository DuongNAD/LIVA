import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["eslint.config.js"]
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    // NOTE: type-aware linting intentionally DISABLED here. The active rules below are
    // all syntactic; enabling the TS project service only added huge per-file memory
    // cost (OOM under lint-staged concurrency) and broke linting of files outside the
    // root tsconfig (e.g. liva-ui). Type checking is covered by `tsc --noEmit` in the
    // lint-staged hook instead. `project`/`projectService: false` forces syntax-only
    // parsing so no tsconfig resolution is needed.
    languageOptions: {
      parserOptions: {
        project: false,
        projectService: false,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // The team's intended hard rules (see AI_CONTEXT.md §12) are no-console,
      // banned imports, and no-fetch/no-sync below. `no-explicit-any` is inherited
      // transitively from tseslint recommended but is NOT in §12, and the codebase
      // uses `any` pervasively — under lint-staged `--max-warnings 0` it would block
      // every commit that touches an existing file. Disable to match documented policy.
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unused-vars": "off",
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
