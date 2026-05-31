---
name: capacitor-ops
version: 2.0.0
description: Core guidelines and diagnostic protocols for managing the Capacitor 8 native Android application, versionCode calculation, intent share targets, and native OAuth.
triggers: ["build android", "android emulator", "versionCode", "share target", "Capacitor"]
dependencies: ["capacitor-cli", "adb", "plugin-share-target"]
---

# Capacitor & Android Development Rules

When developing or debugging the native Android shell (`com.antigravity.vibe`), adhere to these strict Capacitor and Android platform rules to prevent build regressions or runtime crashes.

---

## 1. Native Platform & Lifecycle Detection
* **Platform Detection**: **ALWAYS** use the custom `isNative()` wrapper from `client/src/utils/platform.ts` instead of calling `Capacitor.isNativePlatform()` directly in components.
* **Service Workers**: Service worker registration **MUST** be disabled when `isNative()` is true. This prevents local client files in the native APK asset envelope from being overwritten by caching conflicts with the native Android WebView.

---

## 2. Platform-Aware Browser Routing
* **Chrome Custom Tabs**: External links **MUST** be launched using the `@capacitor/browser` plugin (Chrome Custom Tabs). Refer to the `useArticleOpener` hook for usage.
* **Navigation Isolation**: **NEVER** use `window.open` or raw `<a>` tags targeting `_blank` in a native context. It will open inside the app's standard WebView container, hijacking navigation and causing back-button failures.

---

## 3. Versioning & versionCode Calculations (MANDATORY)
To prevent build collisions inside the Google Play Console:
* **Calculation Scheme**: The native Android `versionCode` **MUST** be computed mathematically from the semantic version (semver):
  `versionCode = major × 10000 + minor × 100 + patch`
  * E.g., `v2.8.15` ➔ `versionCode 20815`.
* **Sequential Banned**: **NEVER** use simple sequential integers (e.g. `293` or `294`) — they will collide with future semver calculations.
* **Automation script**: Always trigger version bumps via `node scripts/update-version.js X.Y.Z [versionCode]`. Do not edit `build.gradle` directly.

---

## 4. Intent Share Target WebView Crash Gotcha (v2.8.17 Baseline)

The `@capgo/capacitor-share-target` plugin buffers native Android ACTION_SEND share intents. This introduces a critical process gotcha:
* **The `removeAllListeners()` Ban**: **NEVER** call `CapacitorShareTarget.removeAllListeners()` or `removeAllListeners()` on the plugin.
  * *Why*: The plugin marks events with `retainUntilConsumed = true`. Calling `removeAllListeners()` removes the JS listener but leaves events in the native plugin buffer. The next time the listener registers, it is immediately overwhelmed with a rapid-fire sequence of stale, replayed events, triggering a race condition and WebView thread crash.
* **The Resolution**: Store the `PluginListenerHandle` returned by `addListener()` and call **`listenerHandle.remove()`** on cleanup (unmounting) to surgically remove the active JS handler without touching the native buffer.
* **Deduplication Window**: To suppress the rapid-fire replay of accumulated retained events from previous sessions, employ URL + timestamp deduplication with a **5-second threshold** (matching `DEDUP_WINDOW_MS` in `useShareTarget.ts`).

---

## 5. Native Google OAuth Integration (v2.8.17 Baseline)
* **Plugin**: Handled via the `@capgo/capacitor-social-login` package.
* **Google Client ID Setup**: Native Google Sign-In requires the Google Web Client ID to be configured in **both** files:
  1. `VITE_GOOGLE_WEB_CLIENT_ID` in the client's `.env.android` file (used to initialize `SocialLogin.initialize()`).
  2. `GOOGLE_WEB_CLIENT_ID` (without `VITE_` prefix) in the server's `.env` file (used by backend `OAuth2Client` to verify the native `idToken` payload).
* **Native OAuth Router Mounting**: The server route `POST /api/auth/native-login` must be mounted **BEFORE** the `requireAuth` middleware inside `server/index.ts`.
* **React 19 / Clerk v6 Bug**: In React 19, `useSignIn()` and `useSignUp()` return `isLoaded: undefined` initially. Always fall back to `useClerk()` + `useAuth().isLoaded` and call the core clients `clerk.client.signIn` / `clerk.client.signUp` directly.

---

## 6. Emulator Port Tunneling (`adb reverse`)
The Android emulator cannot resolve `localhost` on the host PC by default. To debug live-reload and routing:
* **Required Mappings**: You must set up local adb tunnels:
  * `adb reverse tcp:5173 tcp:5173` (tunnels Vite dev port)
  * `adb reverse tcp:3001 tcp:3001` (tunnels backend API port)
* **Automation**: These commands run automatically when launching dev environments via `.\android-dev.ps1` (refer to `scripts/adb-reverse.ps1`).

---

## 7. Diagnostics (Blank Screen Gotcha)
If the native Android APK shows a blank dark-blue or black screen:
1. Confirm the Clerk Production Publishable Key is packaged in the build environment.
2. Verify that `https://localhost` and `capacitor://localhost` have been added as **Trusted Origins** in the Clerk Dashboard.
