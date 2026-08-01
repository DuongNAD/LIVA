# LIVA UI (Vue 3)

This is the Vue 3 + TypeScript + Vite frontend for LIVA.
It serves as the Webview content for the Tauri desktop application.

## Key Features
- **Ghost Mode UI**: Transparent background for floating widget.
- **Native Gateway Client**: Real-time duplex communication with `liva-native-core`.
- **Wake Candidate Capture**: Browser worker cuts utterances; Rust verifies them with
  the ONNX classifier and/or local STT.
- **WebRTC AEC**: Always-on microphone with hardware echo cancellation.

## Development
Run `npm run dev` from the `liva-ui` folder or use `npm run dev` at the repository root.
