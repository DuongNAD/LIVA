import tseslint from 'typescript-eslint';

export default tseslint.config(
  ...tseslint.configs.recommended,
  {
    ignores: [
      "dist/",
      "node_modules/",
      "tests/",
      "src/evolution/",
      "coverage/",
      "vitest.config.ts",
      "src/test_*.ts",
      "src/utils/auth_google_script.ts",
      "src/utils/generate_skeleton.ts"
    ]
  },
  {
    rules: {
      "no-console": "error",
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unused-vars": "off"
    }
  }
);
