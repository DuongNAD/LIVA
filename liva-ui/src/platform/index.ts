import { TauriAdapter } from "./TauriAdapter";
import { MockWebAdapter } from "./MockWebAdapter";
import type { IPlatformAdapter } from "./IPlatformAdapter";

export function detectPlatform(): IPlatformAdapter {
  // 1. Detect Tauri
  if (typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__) {
    console.log('[PlatformBridge] 🚀 Detected Tauri Environment');
    return new TauriAdapter();
  }

  // 2. Fallback (Chrome/Browser Dev Mode)
  console.log('[PlatformBridge] 🌐 Detected Browser Environment (Mock Mode)');
  return new MockWebAdapter();
}
