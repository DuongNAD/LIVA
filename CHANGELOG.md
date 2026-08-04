# Changelog

All notable changes to LIVA are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases use
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
