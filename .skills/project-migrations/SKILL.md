---
name: project-migrations
version: 2.0.0
description: Strict guidelines for creating, verifying, and running Sequelize PostgreSQL schema migrations in project-name.
triggers: ["database migration", "sequelize migration", "add column", "SequelizeMeta"]
dependencies: ["sequelize", "tsx", "mcp:postgres"]
---

# Database Migration & Model Rules

Antigravity-Vibe uses TypeScript Sequelize migrations to track and eantigravity-vibe database schemas. These migrations do NOT run automatically on server boot, but are executed via the centralized automated runner `server/scripts/migrate.ts` or during production deploys.

## 1. Migration Protocol
* **Location**: Store all incremental migration files in `server/migrations/`.
* **Naming Standard**: Use three-digit incremental prefixes: `NNN_short_description.ts` (e.g., `006_vault_is_edited.ts`, `010_content_threshold_setting.ts`).
* **Up / Down Methods**: Every migration file **MUST** export both an `up` and a `down` method, taking `QueryInterface` from `sequelize`.
  ```typescript
  import { QueryInterface, DataTypes } from 'sequelize';
  export async function up(queryInterface: QueryInterface): Promise<void> { ... }
  export async function down(queryInterface: QueryInterface): Promise<void> { ... }
  ```
* **Column Naming**: All columns **MUST** use camelCase (e.g., `subscriptionPlanId`), EXCEPT when mirroring Clerk webhook response fields (e.g., `user_in_clerk`, `subscription_status`) which mirror Clerk's naming conventions.

## 2. Automated Migration Runner (`server/scripts/migrate.ts`)
We use a custom, automated migration runner to apply changes in lexicographical order:
* **Check Status**: To view applied and pending migrations, run:
  `npx tsx server/scripts/migrate.ts --status`
* **Apply Migrations**: To apply all pending migrations:
  `npx tsx server/scripts/migrate.ts`
* **Idempotency Guard**: Once successfully run, the filename is recorded in the `"SequelizeMeta"` table in PostgreSQL. Pending migrations are filtered dynamically.

## 3. Step-by-Step Schema Evolution Guide
When tasked with database changes:
1. **Create the Migration File**: Place a new TS file in `server/migrations/` following the layout inside `resources/ts-migration-template.ts`.
2. **Update the Sequelize Model**: Ensure the corresponding TypeScript model file in `server/models/` is updated to reflect the new columns/relations.
3. **Local Testing**:
   * Run the migration: `npx tsx server/scripts/migrate.ts`
   * Inspect the `SequelizeMeta` table to ensure it is marked as applied.
   * Verify using the Postgres MCP:
     `SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'TableName';`
4. **Model Alignment Verification**: Run `npm run typecheck` to confirm type safety is preserved across imports.

## 4. Integrity and Deletions Gotchas
* **Soft-Deletes Preferred**: We never hard-delete user accounts or organizations. Use soft-delete flags like `user_in_clerk = false` or `org_active = false`.
* **Cascade Constraints**: Deleting a `Feed` **MUST** cascade-delete all child `Articles`. Ensure the migration and Sequelize model association specify `onDelete: 'CASCADE'`.
* **Index Audit**: If adding compound or filter-backed indexes, always verify their creation using the Postgres MCP:
  `SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'TableName';`
