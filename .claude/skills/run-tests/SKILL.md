---
name: run-tests
description: Run LIVA's test suites or linters with the correct, crash-avoiding flags. Use when asked to run tests, lint, typecheck, verify a change, or check the build for liva-gateway, liva-ui, or the Python liva-ai-engine. Encodes the vitest worker limits and memory flags that prevent the CI/local deadlocks documented in commit f337aa3.
---

# Run LIVA tests

This repo's test commands have non-obvious flags that, if omitted, cause worker-pool
deadlocks and out-of-memory crashes (see commit f337aa3). Always use the commands below
rather than a bare `vitest` / `pytest`.

`$ARGUMENTS` may name a scope: `gateway`, `ui`, `engine`, or empty for all. It may also
include the word `lint` — when present, run the linters in the **Lint** section instead of
the test suites (optionally combined with a scope, e.g. `lint gateway`). Run only the
requested scope. All commands run from the repo root unless noted.

## liva-gateway (vitest, TypeScript)
Forks pool + single worker + raised heap are mandatory — threads or >1 worker crashes:
```bash
NODE_OPTIONS="--max-old-space-size=4096" npx vitest run --pool=forks --max-workers=1 -w liva-gateway
```
- Typecheck only: `npm run typecheck` (→ `tsc --noEmit` in liva-gateway)
- Strict (typecheck + tests): `npm run test:strict -w liva-gateway`

## liva-ui (vitest, jsdom)
```bash
npm run test -w liva-ui
```
- Typecheck: `npm run typecheck -w liva-ui` (→ `vue-tsc --noEmit`)

## liva-ai-engine (pytest, Python)
Use the project venv; `pytest.ini` already sets `testpaths=tests` and ignores the
vendored `llama_cpp_src`. On macOS the venv is created from `requirements_mac.txt`.
```bash
cd liva-ai-engine && ./venv/bin/python -m pytest
```
If the venv is missing, create it first:
```bash
cd liva-ai-engine && python3 -m venv venv && ./venv/bin/pip install -r requirements_mac.txt
```

## Lint (`lint` in $ARGUMENTS)
There are no `lint` npm scripts — eslint runs off the flat configs (`eslint.config.js` at
root, `liva-gateway/eslint.config.mjs` for the gateway), and typecheck is per-workspace.
`--max-warnings 0` matches what the husky/lint-staged pre-commit gate enforces.

- **gateway** — eslint (its own config excludes tests/evolution/dist) + typecheck:
  ```bash
  (cd liva-gateway && npx eslint . --max-warnings 0) && npm run typecheck -w liva-gateway
  ```
- **ui** — eslint over source + vue-tsc:
  ```bash
  npx eslint "liva-ui/src/**/*.{ts,vue}" --max-warnings 0 && npm run typecheck -w liva-ui
  ```
- **engine** — ruff lint (config in `liva-ai-engine/ruff.toml`):
  ```bash
  liva-ai-engine/venv/bin/ruff check liva-ai-engine
  ```
  Add `--fix` to auto-apply safe fixes. Note: existing engine code has a known backlog of
  findings — when linting after an edit, focus on findings in the files you changed rather
  than treating the whole-repo count as a regression.

## After running
Report pass/fail counts per suite (or finding counts per linter). If vitest reports a worker timeout or heap crash,
confirm the `--pool=forks --max-workers=1` flags were used before investigating further.
Do not "fix" a crash by raising worker count — that reintroduces the deadlock.
