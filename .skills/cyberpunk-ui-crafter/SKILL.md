---
name: cyberpunk-ui-crafter
version: 2.0.0
description: Core styling rules, component guidelines, Framer Motion animations, personal archiving WYSIWYG editor parameters, and visual verification protocols for the React frontend.
triggers: ["cyberpunk executive", "mui 7", "framer motion", "vault view", "TipTap", "AppInfoChip"]
dependencies: ["react 19", "@mui/material", "framer-motion", "tiptap-v3", "mcp:playwright"]
---

# UI & React Component Guidelines

The Antigravity-Vibe frontend (`client/`) is a high-fidelity visual experience built on React 19, Vite 7, Framer Motion, and MUI 7. It adheres to strict, premium "Cyberpunk-Executive" brand constraints.

---

## 1. Design System & Semantic Color Tokens

We enforce a rigorous theme color system. **NEVER** hardcode hex colors or RGB values in components:
* **The HSL Semantic Palette**:
  * **Personal Feeds**: Map to `primary.main` (Electric Lavender/Vivid Indigo).
  * **Curated Feeds**: Map to `curated.main` (Warm Cyber Purple).
  * **Organization Feeds**: Map to `org.main` (Electric Amber, `#FFC107`).
* **Icons**: Standardize strictly on `lucide-react`. Ensure size and weight transitions match surrounding text hierarchies.

---

## 2. Platform-Aware Navigation (`useArticleOpener`)
To prevent app hijacking or navigation lockups within native Android custom WebViews:
* **Rule**: **NEVER** use `window.open` targeting `_blank` or raw `<a>` tags inside custom components.
* **Usage**: Always consume the custom hook:
  ```typescript
  import { useArticleOpener } from '../hooks/useArticleOpener';
  const { openArticle } = useArticleOpener();
  // On trigger:
  openArticle(articleUrl);
  ```
  * *Behind the scenes*: On web/desktop, this safely falls back to standard `window.open(url, '_blank')`. On Android, it routes through the `@capacitor/browser` plugin using Chrome Custom Tabs or `_system` browser wrappers.

---

## 3. Dynamic App Telemetry (`AppInfoChip`)
Antigravity-Vibe utilizes an expandable telemetry pill (`AppInfoChip.tsx`) for user debugging:
* **10 Diagnostic Fields**: Displays Server Origin, User Role, User-Agent Browser, OS Platform, Install Mode (Native/PWA/Browser), Push permission status, Service Worker state, Android Device model, App native build info, and viewport screen dimensions.
* **Actions**: Supports standard Copy-to-Clipboard (`copyVariant='clipboard'`) and chat-input Push (`copyVariant='push'`).

---

## 4. Antigravity-Vibe Vault personal archiving & TipTap v3 (v2.8.17 Baseline)
Our personal archiving feature is highly polished, utilizing a prose-matched TipTap WYSIWYG editor:
* **Boilerplate Integration**: Lazily load `VaultEditor.tsx` inside `VaultView.tsx` to optimize bundles.
* **Prose Styles**: The editor's input field and `ReactMarkdown` reader preview use identical visual margins, code blocks, quote borders (`curated.main`), and link colors (refer to `MARKDOWN_PROSE_STYLES` in `VaultEditor.tsx`).
* **Dirty-State Gating**: Always hook up `onDirtyChange(isDirty)` to prevent users from accidentally closing the editor sheet or modal with unsaved modifications.
* **Revert Action**: If `originalMarkdown` is provided, expose the "Revert to Original" button, calling `POST /vault/:id/revert` and updating editor content dynamically.

---

## 5. Coding Standards: The "Vibe Coding" threshold
* **Context Limit**: Adhere strictly to the `File Size Zones` rule in `GEMINI.md`. Files entering the 475-499 line zone (RED) require an active decomposition plan. Files exceeding 500 lines are considered `God Components` and must be broken down before any new styling is added.

---

## 6. Visual Verification Protocols (Merged from `vibe-check-ui`)

To audit visual fidelity, layouts, and responsiveness across web/native platforms without local execution:
* **The Playwright MCP Check**: Proactively launch a headless Chromium context using the `playwright` MCP server.
* **Visual Audit Routine**:
  1. Navigate to the local dev URL: `http://localhost:5173`.
  2. Simulate the responsive screen sizes:
     * Desktop: `browser_resize(1280, 800)`
     * Mobile: `browser_resize(375, 812)` (simulating mobile viewports)
  3. Call `browser_take_screenshot` to capture the layout.
  4. Perform OCR or image inspection on the saved screenshot to verify HSL color contrast, typography margins, and modal positions.
* **Browser Verification & Client Auditing**:
  When reviewing a frontend component change manually or through local browser inspection:
  1. **F12 Console Audit**: Always inspect the browser console (F12) for uncaught exceptions, warning outputs, or routing failures after each significant rendering change.
  2. **Cache Erasure**: Issue a hard refresh (`Ctrl+Shift+R`) before concluding a visual test passes, to guarantee zero stale bundle caching interference.
  3. **Network Tab Auditing**: Verify the **Network tab** to ensure that components are issuing correct, debounced API calls and payloads without triggering infinite rendering loops.
