---
name: liva-compliance-sanitizer
description: Audit data streams and documents for Personally Identifiable Information (PII), sensitive secrets, and regulatory compliance violations before processing or external storage. Use when sanitizing prompt payloads, masking PII/credentials, verifying GDPR/Decree 13 compliance, or preparing data for secure export.
---

# LIVA Compliance Sanitizer

## Workflow

1. **Payload Inspection**: Scan text payloads, logs, configuration files, and database exports for sensitive entities.
2. **Entity Detection**:
   - **Credentials & Secrets**: API tokens, JWTs, private keys, passwords, connection strings.
   - **Personal Identifiers (PII)**: Vietnamese Citizen ID (CCCD), phone numbers, email addresses, tax IDs, passport numbers, home addresses.
   - **Financial Data**: Credit card numbers, bank account numbers, salary records.
3. **Redaction & Reversible Tokenization**:
   - Replace sensitive tokens with deterministic surrogate placeholders (e.g., `[REDACTED_CCCD_1]`, `[REDACTED_PHONE_1]`).
   - If reversible tokenization is required, store encryption keys locally in SQLite with AES-256-GCM under user-controlled keychains.
4. **Compliance Gate Verification**:
   - Check compliance against Vietnam Decree 13/2023/NĐ-CP (Personal Data Protection) and GDPR principles (data minimization, storage limitation).
5. **Audit Trail Logging**:
   - Record an immutable audit log detailing timestamp, entity category redacted, and sanitized destination hash without leaking original plaintext.

## Stop Conditions

Stop and report when:
- Unmasked root passwords or master encryption keys are exposed in public code/logs.
- Data export lacks explicit data subject consent metadata under strict compliance mode.
