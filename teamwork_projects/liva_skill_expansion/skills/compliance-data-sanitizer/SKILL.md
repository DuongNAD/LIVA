---
name: compliance-data-sanitizer
description: Audit, mask, and tokenize multi-lingual Personally Identifiable Information (PII), Protected Health Information (PHI), and credentials with AES-256-GCM encryption and GDPR/Decree 13 compliance. Use when sanitizing prompt payloads, masking sensitive database exports, verifying privacy compliance, auditing data leakages, or managing encrypted surrogate tokens.
---

# Compliance Data Sanitizer & Privacy Vault

## Workflow

1. **Payload & Data Stream Ingestion**:
   - Ingest raw input streams (LLM prompt payloads, chat transcripts, database query results, CSV/JSON data dumps, and log files).
   - Normalize character encodings (UTF-8) and handle multi-lingual text segmentation.

2. **Multi-Lingual Entity Detection & Pattern Classification**:
   - Scan for multi-tier sensitive entities using regex heuristics and named entity recognition:
     - **Vietnamese Identifiers**: 12-digit Citizen ID (CCCD), 9-digit Identity Card (CMND), Personal Tax Code (MST), Social Health Insurance Number (BHXH), and Vietnamese phone formats (`+84`, `09x`, `08x`, `07x`, `03x`).
     - **Global PII & Financial Identifiers**: Credit card numbers (validated via Luhn algorithm), IBAN / bank accounts, SWIFT/BIC codes, passport numbers, email addresses, and home/postal addresses.
     - **Protected Health Information (PHI)**: Diagnostic codes (ICD-10), prescription data, clinical notes, and biometric records.
     - **Secrets & Infrastructure Credentials**: RSA/EC private keys (`BEGIN PRIVATE KEY`), API tokens (OpenAI, AWS, GitHub), JWTs, and database connection URIs.

3. **Deterministic Redaction & AES-256-GCM Privacy Vault**:
   - Substitute sensitive tokens with consistent surrogate placeholders:
     - `Nguyen Van A` $\rightarrow$ `[REDACTED_NAME_1]`
     - `001095012345` $\rightarrow$ `[REDACTED_CCCD_1]`
     - `0912345678` $\rightarrow$ `[REDACTED_PHONE_1]`
   - If reversible de-anonymization is requested under verified user consent, store the surrogate-to-plaintext mapping in the local SQLite Privacy Vault, encrypted with AES-256-GCM using hardware-backed or passphrase-derived keys.

4. **Regulatory Baseline Compliance Verification**:
   - Audit payload handling against privacy frameworks:
     - **Vietnam Decree 13/2023/NĐ-CP**: Article 9 (Sensitive personal data classification), Article 11 (Valid consent mechanisms), and Article 13 (Data processing notifications).
     - **EU GDPR**: Article 5 (Data minimization & purpose limitation), Article 6 (Lawful basis), and Article 9 (Special categories of data).
   - Generate compliance risk ratings (`COMPLIANT`, `MINOR_WARNING`, `CRITICAL_VIOLATION`).

5. **Immutable Audit Ledger & Hash Attestation**:
   - Record an immutable audit entry in the local database:
     - Hash of original payload (SHA-256)
     - Hash of sanitized payload (SHA-256)
     - Categorical entity counts (e.g., 2 CCCD, 1 Phone, 1 API Key)
     - Redaction timestamp and operator context (zero plaintext retained in logs).

6. **Vault Reporting & Knowledge Archival**:
   - Save the compliance audit report into `vault/Knowledge/Privacy Audit - <Dataset_Title>.md` using `write_markdown`.
   - Adhere strictly to the Obsidian Vault Dialect (`title`, `tags: [knowledge/compliance, security/privacy]`, `author: "user"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Hybrid. PII scanning, masking, and compliance validation run automatically (`Auto`). Reversible de-anonymization and vault writes require explicit policy authorization (`ProposeOnly`).
- **Tool Dependencies**: Requires `compliance` native module / MCP (`compliance:scan_pii`, `compliance:mask_payload`, `compliance:tokenize_reversible`, `compliance:audit_report`) and `obsidian` MCP (`write_markdown`, `search_vault`).
- **Cryptographic Invariants**: AES-256-GCM with unique 96-bit nonces (IV) per record; keys zeroized in memory upon process completion.
- **Fail-Closed Privacy Gate**: Sensor, screen, and log exports must check `consent::load().is_capture_allowed()`.

## Stop Conditions

Stop and report immediately when:
- Unmasked root passwords, cloud provider master keys, or private signing keys are detected in exported logs or shared repositories.
- Reversible de-anonymization is requested without valid user consent or without an authorized master decryption key.
- A critical violation of Vietnam Decree 13 or GDPR is detected (e.g., unconsented transmission of sensitive biometric or health data to external cloud LLMs).
- Encryption hardware token or keychain access fails during reversible token generation.

## Privacy Audit Report Example

```markdown
---
title: "Privacy Audit - Customer Support Chat Logs Sanitization"
tags:
  - knowledge/compliance
  - security/privacy
author: "user"
last_update: "2026-08-14T12:00:00+07:00"
---

# Privacy Audit: Customer Support Chat Logs Sanitization

## 1. Executive Summary
- **Dataset**: `support_tickets_export_20260814.csv` (1,240 records)
- **Sanitization Status**: **COMPLIANT** (100% PII Masked)
- **Regulatory Frameworks**: Vietnam Decree 13/2023/NĐ-CP & EU GDPR

## 2. Redacted Entities Breakdown

| Category | Entity Type | Detected Count | Redaction Pattern | Encryption Mode |
| :--- | :--- | :--- | :--- | :--- |
| **Personal ID** | Vietnamese CCCD | 48 | `[REDACTED_CCCD_n]` | AES-256-GCM Vault |
| **Contact** | VN Phone Numbers | 142 | `[REDACTED_PHONE_n]` | AES-256-GCM Vault |
| **Contact** | Email Addresses | 115 | `[REDACTED_EMAIL_n]` | AES-256-GCM Vault |
| **Financial** | Bank Account (VND) | 22 | `[REDACTED_BANK_n]` | AES-256-GCM Vault |
| **Secrets** | Bearer Tokens | 4 | `[REDACTED_TOKEN_n]` | Irreversible Scrub |

## 3. Cryptographic Verification Receipt
- **Original Payload SHA-256**: `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- **Sanitized Payload SHA-256**: `a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e`
- **Vault Token Batch ID**: `vault_tok_20260814_982f1b`
- **Attestation Signature**: `ECDSA-P256-SHA256:VERIFIED`
```
