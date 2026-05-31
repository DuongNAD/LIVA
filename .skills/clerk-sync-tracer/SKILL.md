---
name: clerk-sync-tracer
version: 2.0.0
description: Guidelines and verification procedures for Clerk Webhooks, Auth Syncing, Subscription states, and Native OAuth in the Antigravity-Vibe ecosystem.
triggers: ["clerk sync", "webhooks", "subscription", "native auth", "native-login"]
dependencies: ["@clerk/express", "google-auth-library", "mcp:postgres", "mcp:clerk"]
---

# Clerk Identity, Sync, & Native Auth Rules

Antigravity-Vibe maintains a local PostgreSQL mirror of Clerk identity data. The local database is the source of truth for the app, not the Clerk API.

## 1. Data Flow Architecture
1. **Real-time Webhooks:** Clerk Dashboard → Webhooks → Local DB (`User`, `Organization`, etc.) via `server/routes/webhooks.ts`.
2. **Login Fallback:** Browser Login → `/auth/sync` → Clerk Billing API synchronization.
3. **Database-First Queries:** **ALWAYS** query the local `User`, `OrganizationMembership`, and `SubscriptionPlan` tables over hitting the Clerk API directly.

## 2. API & Subscription Gotchas
* **Arrays in Clerk:** `subscriptionItems` contains canceled, active, and past-due items. **ALWAYS** use `subscriptionItems.find(i => i.status === 'active')`. NEVER just assume `[0]`.
* **Org Status Override:** Organization members **ALWAYS** receive the `business` plan, overriding their personal Clerk subscription.
* **Accessing Auth ID:** In Express, **ALWAYS** use `getAuth(req)` from `@clerk/express`. **NEVER** access `req.auth` as a property (it is a function in v2).

## 3. Key Management & Isolated Environments
Antigravity-Vibe runs on two totally isolated Clerk environments:
* **DEV / TEST:** Domains use `enhanced-zebra-...clerk.accounts.dev`. Keys start with `pk_test_` and `sk_test_`. 
* **PRODUCTION:** Domains use `clerk.antigravity.vibe`. Keys start with `pk_live_` and `sk_live_`.
* *Never mix test and live keys across the environments.*

## 4. Native OAuth & Authentication (v2.8.17 Baseline)
* **Native OAuth Route**: The unauthenticated `POST /api/auth/native-login` route handles native Google OAuth ID tokens.
  * **CRITICAL**: The `nativeAuth` router **MUST** be mounted **BEFORE** the `requireAuth` middleware inside `server/index.ts`.
* **Client Google Client ID**: Native Android OAuth requires `VITE_GOOGLE_WEB_CLIENT_ID` on the client (for `SocialLogin.initialize()`) and `GOOGLE_WEB_CLIENT_ID` (no `VITE_` prefix) in `server/.env` for backend `OAuth2Client` validation.
* **React 19 / Clerk v6 Bug**: In `@clerk/react` v6 + React 19, `useSignIn()` and `useSignUp()` return `isLoaded: undefined` initially. 
  * **Workaround**: Use `useClerk()` + `useAuth().isLoaded` instead. Use `clerk.client.signIn` and `clerk.client.signUp` directly. Refer to `client/src/hooks/useNativeAuth.ts`.

## 5. GDPR Right to Erasure & Deletions
* **Deletions Gating**: In `UserService.deleteUser()`, the teardown sequence must occur in this order to guarantee data integrity:
  1. Cascade-delete local database assets associated with the user.
  2. Permanently delete the user record in the local database.
  3. Call `clerkClient.users.deleteUser(clerkId)` to remove the user from Clerk's authentication database.

## 6. Verification & Troubleshooting
* Check the server logs for `[Auth] User <id>...` prefixes.
* Use the Postgres MCP server to verify if `user_in_clerk` is true (refer to `scripts/db-verify.sql`).
* **Caddy for Webhooks**: Local Clerk webhooks require a running Caddy tunnel (`caddy run`).
* **External API Debugging & Sync Audit**:
  When diagnosing auth, webhook, or subscription sync mismatches:
  1. **Direct API Lookups**: Leverage the Clerk MCP server or query the Clerk SDK/Dashboard directly to confirm the user's authentic third-party profile properties.
  2. **Database Alignment Compare**: Compare the external API payload structures side-by-side with your local PostgreSQL `Users` table fields to isolate sync delta or property mismatches.
  3. **Verification of Session Identity**: Confirm the frontend's active user session ID matches the token parameters processed by the Express backend (`req.user.id`).
