# LIVA Spring Cleaning Report - 2026-05-31

## Executive Summary
This report analyzes the codebase for dead code, unused imports, orphaned files, and outdated debug scripts. The goal is to maximize code cleanliness, optimize the developer environment, and preserve core modular skills.

## 1. Dead Code & Unused Imports
* **TypeScript Compiler Check**: Checked via `npm run typecheck`, which compiled with zero errors after resolving the CSHSAnalyzer import bug.
* **Incubating Modules**:
  * **File**: `src/incubating/WriteValidationGate.ts`
  * **Description**: Originally placed in memory but moved to `src/incubating/` because it is awaiting a lightweight NLI classifier endpoint.
  * **Action**: **PRESERVE**. Do not delete, as it is documented under a clear re-activation plan in `src/incubating/README.md`.

## 2. Orphaned & Legacy Files
* **Candidates**:
  * `export_weights.py`
  * `fix_onnx.py`
  * `fix_onnx_output.py`
  * `print_onnx.py`
  * `print_onnx2.py`
  * `test_onnx.js`
* **Description**: Development scratch scripts and debug utilities for verifying ONNX layouts that are not used in production execution flows.
* **Action**: Move to a dedicated debug or scratch directory (e.g., `scripts/onnx_debug/`) to clean up the root repository directory.

## 3. Unused Dependencies (package.json)
* **Madge / Circular Dependency Analyzer**:
  * `madge` is listed in devDependencies. Can be run to inspect imports.
  * No immediate bloat detected. All core dependencies are used in active runtime features.

## 4. Verification & Testing Impact
* Deleting or moving the root python ONNX scratch files has **zero impact** on system execution or gateway tests, as verified by search queries showing zero imports.
