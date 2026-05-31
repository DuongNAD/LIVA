---
name: admin-ui-patterns
version: 2.0.0
description: Coding guidelines, UI patterns, GDPR deletion gates, FCM push chat cards, and routing protocols for the separate platform administrator portal.
triggers: ["admin dashboard", "Bull Board", "FCM chat logging", "settings CRUD", "user deletion"]
dependencies: ["@bull-board/express", "pino", "mcp:redis"]
---

# Admin UI Development Patterns

The Admin UI (`admin-ui/`) is a dedicated platform administrator portal running separately from the client (port 5174 in dev). While sharing the Cyberpunk-Executive design tokens, it uses simplified Clerk authentication and has **no automatic org sync logic**.

---

## 1. Directory & Routing Architecture
* **Isolated Environment**: Developed under `admin-ui/` as a separate Vite project.
* **Axios API Scope**: Base API calls in `admin-ui/src/api.ts` are prefixed with `/api`.
  * **Rule**: **NEVER** prepend `/api/` directly in routing calls. E.g. use `api.get('/admin/dashboard/stats')`, NOT `api.get('/api/admin/dashboard/stats')`.
* **Clerk v6 `<Show>` Controls**: Restrict elements using `<Show when='signed-in'>` and `<Show when='signed-out'>`. Do not import legacy `SignedIn` or `SignedOut` wrappers.

---

## 2. Telemetry and Queue Monitoring (Bull Board)
* **Bull Board Path**: Access via `/api/admin/queues`.
* **Static Asset Proxy Bypass**: The "Monitor Queues" link **MUST** target the absolute backend host URL (`https://[host]/api/admin/queues`) rather than relative endpoints. This bypasses the Vite proxy and service worker, which would otherwise intercept assets.
* **Basic Auth**: Access is protected via `BULL_BOARD_USER` / `BULL_BOARD_PASS` environment variables, entirely independent of Clerk identity sessions.

---

## 3. GDPR Deletion Gating & Admin Protection (v2.8.17 Baseline)
To protect platform integrity and prevent complete administrator exclusion:
* **The `role === 'admin'` Gate**: Inside the user database management tables in the Admin UI:
  * **Rule**: **NEVER** allow an account with `role === 'admin'` to be deleted.
  * **UI Standard**: The delete button for any admin row **MUST** be disabled, paired with a persistent explanatory Tooltip (e.g. *"Admin accounts cannot be deleted. Demote their role to User first."*).

---

## 4. FCM Support Chat Logging (v2.8.17 Baseline)
Support staff can reply to users and audit automated push communications inside `ChatThread.tsx`:
* **Notification Audit Cards**: Push notification events are styled as centered violet cards decorated with a Lucide `Bell` icon. They render using `senderRole === 'notification'` with specific `notificationTitle` and `notificationBody` attributes.
* **The FCM Reply Gate**: When sending replies, the Admin UI check-box `notifyFcm` **MUST** be disabled or hidden unless the recipient's metadata indicates `hasFcmToken === true`. This prevents attempting pushes to unsubscribed desktop browsers.

---

## 5. Layout Grid & MUI 7 Standards
All administrative CRUD pages must utilize modern Material-UI grid boundaries:
* **MUI v7 Grid Standard**: Use the newer `<Grid>` layout format.
  * **Rule**: **ALWAYS** use the modern `size` object attribute instead of deprecated inline `xs`/`md`/`lg` props.
  * *Correct*: `<Grid size={{ xs: 12, md: 6 }}>`
  * *Incorrect*: `<Grid xs={12} md={6}>`
