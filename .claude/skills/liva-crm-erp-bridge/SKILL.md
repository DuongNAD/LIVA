---
name: liva-crm-erp-bridge
description: Connect and synchronize customer accounts, quotations, and orders between local stores and enterprise CRM/ERP platforms (Salesforce, HubSpot, Odoo). Use when syncing customer profiles, reconciling sales orders, resolving multi-platform data conflicts, or streaming enterprise webhooks.
---

# LIVA CRM ERP Bridge

## Workflow

1. **Connector Authentication & Status Inspection**:
   - Verify connection status and API quotas for configured CRM/ERP endpoints (Salesforce REST, HubSpot API, Odoo JSON-RPC/REST).
   - Validate encryption of API tokens and OAuth refresh tokens stored in local SQLite vault using AES-256-GCM.

2. **Entity Schema Mapping & Transformation**:
   - Map bi-directional data models between local schemas and enterprise systems:
     - **Customer Accounts**: `Contact` / `Account` (Name, Tax ID, Email, Phone, Billing Address).
     - **Deals & Quotations**: `Opportunity` / `Quote` (Line items, Unit prices, Discounts, Currency, Stage).
     - **Sales Orders & Invoices**: `Order` / `Invoice` (Fulfillment status, VAT computation, Payment terms).
   - Apply normalization filters (e.g., phone number E.164 standardization, tax ID formatting).

3. **Idempotent Sync Execution & Delta Ingestion**:
   - Retrieve delta changes using timestamp watermarks (`updated_at >= :last_sync_timestamp`).
   - Assign deterministic idempotency keys (`idempotency_key = sha256(entity_id + payload_hash + sync_epoch)`) to every mutating API call to prevent duplicate creations.

4. **Domain-Authoritative Conflict Resolution**:
   - Detect simultaneous dual-write conflicts and resolve using the authoritative domain hierarchy:
     - **Financial Ledger & Invoices**: ERP system is authoritative.
     - **Customer Contacts & Deal Pipeline**: CRM system is authoritative.
     - **Local Overrides**: If manual resolution is configured, stage conflicting records in the Conflict Queue for operator review.

5. **Two-Phase Confirmation for Outbound Cloud Mutations**:
   - **Read-Only / Inbound Pull**: Fetching records and computing synchronization diffs executes automatically.
   - **Outbound Batch Mutations**: Generating new cloud invoices, deleting contacts, or overriding quotes requires explicit Two-Phase Confirmation previewing affected records.

6. **Audit Ledger & Dead Letter Queue (DLQ)**:
   - Record immutable sync event logs into SQLite WAL (`crm_erp_sync_events`).
   - Route unprocessable records (schema validation error, missing mandatory fields) to the Dead Letter Queue (DLQ) with exponential retry backoff.
   - Save execution summaries to `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Enterprise - <Sync_Title>.md` using `write_markdown`.

## Platform Constraints

- **Execution Mode**: Two-Phase Confirmation for batch cloud writes; Automatic for delta querying and read-only schema mappings.
- **Tool Dependencies**: Requires native HTTP/REST connector and `obsidian` MCP (`write_markdown`, `search_vault`).
- **Security & Compliance**: All cloud credentials encrypted at rest with AES-256-GCM. Customer PII handled in compliance with Vietnam Decree 13/2023/NĐ-CP and GDPR data minimization rules.

## Stop Conditions

Stop and report immediately when:
- Enterprise API returns authentication failure (`401 Unauthorized`) or expired OAuth refresh tokens.
- Dual-write conflict occurs on non-reconcilable financial fields without an authoritative resolution rule.
- API rate limits exceed safe thresholds (`429 Too Many Requests`) with risk of service lockout.
- Unsanitized plain-text credit card or banking PIN credentials appear in synchronization payloads.
