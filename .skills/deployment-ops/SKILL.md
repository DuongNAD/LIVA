---
name: deployment-ops
version: 2.0.0
description: Core guidelines and diagnostic protocols for PM2 deployments, Caddy proxies, database backup dumps, Better Stack heartbeats, and fast-fail health checks.
triggers: ["deploy test", "deploy production", "Caddyfile", "automated backup", "Better Stack"]
dependencies: ["pm2", "caddy", "pg_dump", "mcp:docker"]
---

# Deployment & CI/CD Operations

Antigravity-Vibe deploys automatically on push for the test server and uses a manual workflow dispatch gate with pre-deploy `pg_dump` backups for the production server.

---

## 1. Environment & Secrets Mapping

We manage committed, mode-specific environment variables for static VITE builds:
* **Development**: Port `5173`/`5174` locally, loads `.env.development`.
* **Test Server**: `test.antigravity.vibe` / `admin.test.antigravity.vibe`, loads `.env.test`.
* **Production**: `antigravity.vibe` / `admin.antigravity.vibe`, loads `.env.production`.
* *Environment-aware configs ensure that `VITE_API_URL` is never manually modified in the codebase.*

---

## 2. Server Deployment & Update Schemes (v2.8.17 Baseline)

The server updates execute using automated bash wrappers on Ubuntu 24.04 LTS:
* **Dual-Mode Database Updates**:
  * **Test Server (`Seed Present`)**: Destroys and cleans the entire PostgreSQL environment on every deploy, restoring a pristine development dataset from `backups/antigravity-vibe_seed.dump`.
  * **Production Server (`Seed Absent`)**: Keeps data intact. Applies *only* incremental schema migrations using `npx tsx server/scripts/migrate.ts`.
* **Playwright Dependencies**: The server update scripts automatically execute:
  `npx playwright install --with-deps chromium`
  to guarantee selector discovery libraries can launch Chromium on clean headless node setups.

---

## 3. Automated Tiered PostgreSQL Backups

`BackupService.ts` executes automated hourly backups using local `pg_dump` spawns:
* **Concurrency Protection**: Uses an in-memory lock `isRunning` to prevent overlapping dump processes if database latency spikes.
* **Tiered Retention Cleanup**: Automatically organizes files into strict limits:
  * **Hourly**: Max 24 dumps. Promotes oldest Hourly ➔ Daily if date is unique.
  * **Daily**: Max 7 dumps. Promotes oldest Daily ➔ Weekly if date falls on a Monday.
  * **Weekly**: Max 10 dumps. Permanently deletes older weekly backups.
  * *Safety Invariant: Only files beginning with `Hourly-`, `Daily-`, or `Weekly-` are audited. `antigravity-vibe_seed.dump` and manual admin snapshots are NEVER deleted.*
* **Better Stack Heartbeat**: Upon successful execution, the runner fires a non-blocking `fetch()` request to `BETTER_STACK_BACKUP_HEARTBEAT_URL` to log a successful heartbeat, guaranteeing immediate alerts if automated backups freeze.

---

## 4. Health Checks & Fast-Fail Redis Client
* **The Hanger Issue**: Standard Redis queues (`QueueService.ts`) use `maxRetriesPerRequest: null`. If Redis suffers an outage, database queries wait indefinitely, causing health checking routes `/api/health` to hang and timeout, which delays automated server recoveries.
* **The Resolution**: `QueueService.ts` exports a dedicated client `healthRedis`:
  ```typescript
  export const healthRedis = new Redis(process.env.REDIS_URL, {
      maxRetriesPerRequest: 1,
      connectTimeout: 2000,
      lazyConnect: true,
  });
  ```
  This guarantees immediate rejection and failure reporting during outages, allowing PM2 or Caddy to trigger fast-fail self-healing loops.

---

## 5. Reverse Proxy Management (Caddy)
* **Domains**: Caddy handles SSL certificates, HTTP/3 proxy, and SPA URL routing.
* **Asset Subdomain Routing**: The admin UI is hosted on a completely separate subdomain (`admin.antigravity.vibe` or `admin.test.antigravity.vibe`) to prevent Vite PWA Service Workers on the main domain from intercepting admin static assets and assets in `/api/admin/queues`.
* **Boilerplate**: Refer to `resources/Caddyfile-reverse-proxy` for standard server blocks.
