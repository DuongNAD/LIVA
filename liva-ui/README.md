# LIVA UI (Vue 3)

This is the Vue 3 + TypeScript + Vite frontend for LIVA.
It serves as the Webview content for the Tauri desktop application.

## Key Features
- **Ghost Mode UI**: Transparent background for floating widget.
- **WebSocket Gateway Client**: Real-time duplex communication with `liva-gateway`.
- **WASM Wake-Word Detection**: Runs local `hey_liva.onnx` inference.
- **WebRTC AEC**: Always-on microphone with hardware echo cancellation.

## Development
Run `npm run dev` from the `liva-ui` folder or use the root `start.ps1` script.
