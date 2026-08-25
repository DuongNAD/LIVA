# Changelog

All notable changes to LIVA are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases use
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- OpenAI-compatible HTTP surface on the gateway: `/v1/models`,
  `/v1/chat/completions` (including SSE streaming) and `/v1/audio/speech`.
  Disabled by default — it opens no socket unless `LIVA_OPENAI_PORT` is set.
- Preflight screen in the Dashboard reporting model, disk and runtime readiness.

### Changed

- The Skills screen now lists the MCP tools the core actually exposes (7)
  instead of a single hard-coded entry, and `system_status` reads its count from
  the same source so the two screens can no longer disagree.
- Rust advisory gate in CI moved from `cargo audit` to
  `cargo deny check -W unmaintained -W unsound advisories licenses sources`,
  adding license and source compliance. Only vulnerabilities fail the build,
  same as before.

### Fixed

- Widget lip-sync: duplicated audio playback, and an analyser that tracked a
  chunk that had not been played yet.
- Avatar control tags are now recognised mid-sentence, not only at the start of
  a turn, with a matching allow-list on both the TypeScript and Rust sides.
- Three per-frame costs on the avatar path reduced.
- Developer tooling now runs outside Windows: the gateway end-to-end check
  resolves the core binary by platform instead of assuming a `.exe` suffix, and
  the artifact-trust test removes its symlink fixture with `remove_file` on unix
  rather than `remove_dir`, which is correct only for a Windows junction. The
  security assertion in that test — rejecting a symlink escape out of the trust
  root — was passing throughout; only the teardown was wrong.
- The UI coverage gate is reproducible from a clean install:
  `@vitest/coverage-istanbul` is now a declared devDependency instead of an
  undeclared optional peer. No threshold was lowered.

### Security

- npm advisories cleared twice on lockfile-only bumps: five findings on
  2026-08-04 and js-yaml CVE-2026-59870 on 2026-08-07. Neither involved a change
  to LIVA's own code; they are advisory-database drift against an unchanged
  lockfile.

## [1.0.0] - 2026-08-03

### Added

- Windows NSIS installer for the per-user LIVA desktop application.
- First-run model setup experience with an integrity-checked model manifest.
- Unified native Rust runtime for local STT, TTS, memory, tools, and desktop IPC.
- Per-device encryption key protected by Windows DPAPI, with recovery-key escrow.

### Security

- Fail-closed authorization for privileged WebSocket and Tauri commands.
- AES-256-GCM personal-data encryption with automatic migration away from the
  former public development key.
- Pinned model and runtime-artifact hashes checked before native loading.

### Known limitations

- Model downloads are separate from the installer and require substantial disk
  space; the complete optional model set is not bundled in the NSIS executable.
- The Windows installer is not yet code-signed, so Windows may display a
  publisher/reputation warning.

[Unreleased]: https://github.com/DuongNAD/LIVA/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/DuongNAD/LIVA/releases/tag/v1.0.0
