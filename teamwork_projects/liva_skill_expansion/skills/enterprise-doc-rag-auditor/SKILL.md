---
name: enterprise-doc-rag-auditor
description: Ingest, index, and audit complex enterprise documents, multi-page PDFs, OCR scans, and legal contracts using hybrid vector-FTS retrieval and risk redlining. Use when analyzing enterprise contracts, cross-referencing legal clauses, extracting structured invoice/financial tables, auditing regulatory non-compliance risks, or generating redline diff reports.
---

# Enterprise Document RAG & Contract Auditor

## Workflow

1. **Document Intake & Structural Layout Parsing**:
   - Inspect the target document (PDF, DOCX, TIFF, PNG/OCR) to determine format, page count, and textual fidelity.
   - If scanned or image-based, trigger OCR layout recognition with bounding-box coordinate tracking for tables and columnar sections.
   - Segment document into logical structural hierarchies: Title, Recitals, Sections, Numbered Clauses, Schedules, Annexures, and Signature Blocks.

2. **Hierarchical Chunking & Hybrid Dual-Index Ingestion**:
   - Apply parent-child sliding chunking:
     - **Parent Chunks**: Full clause sections (512–1024 tokens) preserving holistic legal context and recital relationships.
     - **Child Chunks**: Atomic sub-clauses (128–256 tokens) optimized for dense semantic retrieval.
   - Index synchronously across dual engines in `liva-native-core`:
     - **Dense Vector Store (`sqlite-vec`)**: Embedding vectors generated via `multilingual-e5-small` or `bge-large`.
     - **Sparse FTS Index (`sqlite FTS5`)**: Full-text keyword index with Porter stemming and unicode tokenization.

3. **Hybrid RRF Querying & Cross-Clause Provenance Retrieval**:
   - Execute Reciprocal Rank Fusion (RRF) search:
     $$\text{RRF\_Score}(d) = \sum_{m \in \{\text{vector}, \text{fts}\}} \frac{1}{60 + r_m(d)}$$
   - Assemble context windows with precise page-number and clause-anchor metadata (e.g., `Doc #4, Section 14.2, Page 18`).
   - Cross-reference related obligations across distinct contract sections (e.g., matching "Indemnification" against "Limitation of Liability" and "Insurance Requirements").

4. **Risk Redlining & Regulatory Deviation Analysis**:
   - Evaluate clauses against standard statutory baselines and enterprise playbooks:
     - **Indemnity & Liability**: Check for uncapped liability, indirect/consequential damages exclusions, and mutual vs. unilateral indemnification.
     - **Termination & Default**: Review termination for convenience notice windows (e.g., 30 vs. 90 days), immediate cure triggers, and post-termination transition obligations.
     - **Intellectual Property & IP Assignment**: Verify work-for-hire boundaries, background IP reservations, and broad derivative ownership claims.
     - **Governing Law & Dispute Resolution**: Validate jurisdiction, mandatory arbitration seats, and fee-shifting provisions.
   - Flag deviations and categorize severity: `LOW`, `MEDIUM`, `HIGH`, `CRITICAL/UNACCEPTABLE`.

5. **Redline Diff Synthesis & Negotiation Strategy Formulation**:
   - Generate standard Markdown redline diffs with proposed replacement wording:
     - `[-] Strikethrough for deleted aggressive clauses`
     - `[+] Bold/Underline for proposed protective language`
   - Provide concrete legal rationale and fallback compromise positions for contract negotiations.

6. **Audit Dossier Compilation & Vault Knowledge Persistence**:
   - Synthesize a comprehensive Contract Risk Audit Report.
   - Persist into Obsidian vault under `vault/Knowledge/Contract Audit - <Document_Name>.md` using `write_markdown`.
   - Adhere strictly to the Obsidian Vault Dialect (`title`, `tags: [knowledge/contract-audit, compliance/legal]`, `author: "user"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Hybrid. Document layout parsing, vector search, and clause extraction execute automatically (`Auto`). Ingestion commits to shared repositories and vault report writes operate under proposal gating (`ProposeOnly`).
- **Tool Dependencies**: Requires `rag` native module / MCP server (`rag:ingest_doc`, `rag:query_hybrid`, `rag:extract_tables`, `rag:redline_diff`) and `obsidian` MCP server (`read_markdown`, `write_markdown`, `search_vault`).
- **Filesystem Boundaries**: Input documents must reside within authorized enterprise data directories or local vault paths. Path validation enforces strict canonical ancestor containment.
- **Resource Limits**: Ingestion pipeline throttles OCR parsing to 25 pages/batch with 512-token chunk boundaries to ensure predictable memory consumption.

## Stop Conditions

Stop and report immediately when:
- The target document is corrupted, password-protected, or unreadable by both text extraction and OCR parsers.
- An absolute path or path traversal pattern (`..`, UNC shares, junction links escaping vault) is supplied.
- The contract lacks a governing law clause or essential signature metadata required for statutory compliance benchmarking.
- The vector embedding engine encounters dimension mismatches or database lock contention during batch ingestion.

## Contract Audit Report Example

```markdown
---
title: "Contract Audit - Master Services Agreement (Vendor Inc)"
tags:
  - knowledge/contract-audit
  - compliance/legal
author: "user"
last_update: "2026-08-14T12:00:00+07:00"
---

# Contract Risk Audit: Master Services Agreement (Vendor Inc)

## 1. Executive Summary
- **Document**: `MSA_VendorInc_v2.1_Final.pdf` (34 pages)
- **Governing Law**: State of Delaware / Vietnam International Arbitration Centre (VIAC)
- **Overall Risk Rating**: **HIGH** (2 Uncapped Liability clauses, 1 Unilateral IP Assignment)

## 2. Key Clause Risk Matrix

| Section | Clause Title | Risk Level | Identified Hazard | Recommended Action |
| :--- | :--- | :--- | :--- | :--- |
| **Sec 8.1** | Limitation of Liability | **CRITICAL** | Customer liability uncapped; Vendor capped at 1x monthly fee. | Require mutual cap of 12 months aggregate fees. |
| **Sec 11.3**| IP Assignment | **HIGH** | Broad assignment of Customer background tooling. | Carve out preexisting IP and open-source components. |
| **Sec 14.2**| Termination Notice | **MEDIUM** | 10-day notice for convenience with immediate termination fees. | Extend to 30 days; eliminate unearned termination fees. |
| **Sec 19.4**| Data Protection | **HIGH** | Missing Vietnam Decree 13 and GDPR compliance schedules. | Add standard Data Processing Addendum (DPA). |

## 3. Redline Recommendations

### Section 8.1: Limitation of Liability
```diff
- 8.1 IN NO EVENT SHALL VENDOR'S TOTAL AGGREGATE LIABILITY EXCEED THE FEES PAID IN THE ONE (1) MONTH PRIOR TO THE CLAIM, WHILE CUSTOMER SHALL REMAIN FULLY LIABLE FOR ALL DIRECT AND CONSEQUENTIAL DAMAGES.
+ 8.1 EXCEPT FOR BREACHES OF CONFIDENTIALITY OR INDEMNIFICATION OBLIGATIONS, NEITHER PARTY'S TOTAL AGGREGATE LIABILITY ARISING OUT OF OR RELATED TO THIS AGREEMENT SHALL EXCEED THE TOTAL FEES PAID OR PAYABLE BY CUSTOMER IN THE TWELVE (12) MONTHS PRECEDING THE EVENT GIVING RISE TO LIABILITY.
```
*Rationale*: Restores commercial parity and protects enterprise against disproportionate downside risk.
```
