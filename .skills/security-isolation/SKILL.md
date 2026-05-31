---
name: security-isolation
version: 1.0.0
description: Coding standards and guidelines for enforcing strict database tenant isolation, feed ownership middlewares, compound indexes, GDPR right to erasure, and DSAR exports.
triggers: ["feed ownership", "tenant isolation", "compound index", "GDPR right to erasure", "DSAR"]
dependencies: ["sequelize", "express-middleware", "mcp:postgres"]
---

# Antigravity-Vibe Security Isolation & Privacy Standards

Antigravity-Vibe operates under a strict, multi-tenant database paradigm. Adherence to these isolation and security standards is mandatory to prevent Insecure Direct Object Reference (IDOR) exploits or GDPR compliance failures.

---

## 1. Feed Ownership Gating (`withFeedOwnership`)

All backend Express routes that modify, access, or fetch data relative to a specific Feed ID (`params.id` or `params.feedId` references) **MUST** utilize the `withFeedOwnership` middleware.
* **Mechanism**: The middleware loads the Feed. If the authenticated user is an `Admin`, it queries by Feed ID only. Otherwise, it scopes the query with `userId = req.user!.id`.
* **Prevention**: Blocks cross-tenant data sniffing (IDOR) where a malicious user changes URL parameters to access or delete another user's private feed.
* **Organization Feeds**: Routes managing promoted team feeds must additionally apply the `requireOrgAdmin` check to ensure the caller has administrative privileges inside the organization.

---

## 2. Multi-Tenant Database Isolation
Whenever querying user-owned records (e.g. `Articles`, `VaultItems`, `UserActivity`) directly in endpoints:
* **The Isolation Standard**: Always filter query configurations using a scoped compound `where` block matching the tenant's identity:
  `where: { id: recordId, userId: req.user!.id }`
* **Never assume implicit safety**: Do not load a record by its primary key ID alone without validating that the `userId` matches the caller's active session.

---

## 3. Mandatory Compound Database Indexes
To maintain performance under high isolation query frequencies, any columns that are regularly filtered together **MUST** be backed by compound PostgreSQL indexes:
* **Articles Index**: The `Articles` table must maintain a compound index on `("feedId", "publishedDate")` to optimize chronological article retrieval.
* **VaultItems Index**: The `VaultItems` table must possess a compound index on `("userId", "createdAt")`.
* **Verification**: Verify active database indexes via the Postgres MCP:
  `SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'Articles';`

---

## 4. GDPR Compliance & Privacy Architectures

We support strict European GDPR rules:

### A. Right to Erasure (Cascading Teardown)
When a user requests account deletion, the teardown sequence inside `UserService.deleteUser()` must be executed sequentially to avoid orphaned database records:
1. **Deduplicate Jobs**: Clear all pending background scraping or extraction tasks associated with the user via `QueueService.clearUserJobs(userId)`.
2. **Cascade Assets**: Clear all localized child models:
   * Delete `VaultItems` where `userId = user.id`.
   * Delete `OrganizationMemberships` where `userId = user.id`.
   * Delete `Feeds` where `userId = user.id` (cascading deletes `Articles` child rows).
3. **Local Database Cleanse**: Delete the primary `User` record from the PostgreSQL database.
4. **Clerk Auth Purge**: Call `clerkClient.users.deleteUser(clerkId)` to permanently delete the identity profile.

### B. GDPR Data Subject Access Request (DSAR)
* **Standard**: Users can download a full export of their platform activity.
* **Architecture**: The `POST /api/user/dsar` endpoint gathers all records matching `userId = user.id` across `User`, `Feeds`, `Articles`, `VaultItems`, and `UserActivity`, packaging them into a single, structured, timestamped JSON export.
