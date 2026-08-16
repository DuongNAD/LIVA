---
name: liva-doc-rag-auditor
description: Ingest, index, and audit enterprise documents (PDFs, contracts, invoices, specifications) using LIVA's hybrid vector-FTS retrieval and risk clause verification. Use when analyzing enterprise documents, cross-referencing clauses, extracting structured invoice data, or auditing compliance risks.
---

# LIVA Doc RAG Auditor

## Workflow

1. **Document Ingestion & Chunking**:
   - Parse source files (PDF, DOCX, Markdown, CSV, Scanned text).
   - Divide documents into semantically coherent chunks with citation metadata (page number, section header, document hash).
2. **Hybrid Indexing**:
   - Register text chunks into LIVA's localized hybrid retrieval store (dense vector embeddings + SQLite FTS5 lexical index).
3. **Auditing & Clause Extraction**:
   - For contracts: Scan for high-risk clauses (unlimited liability, ambiguous termination terms, penalty multipliers, non-compete overreach).
   - For invoices & financials: Extract structured entities (VAT/Tax IDs, payment terms, line item calculations, bank credentials) and cross-validate totals.
4. **Evidence Synthesis & Citation**:
   - Formulate structured audit reports where every finding points directly to the verbatim excerpt and document page reference.
   - Categorize risks into `CRITICAL`, `HIGH`, `MEDIUM`, and `LOW`.
5. **Persistence**:
   - Archive audit summaries and extracted structured tables to the designated project reports or Obsidian Vault under `Knowledge/Enterprise/`.

## Stop Conditions

Stop and report when:
- Source document is unreadable, corrupted, or password-protected.
- Mathematical discrepancies are detected in financial line items without automatic resolution.
- Key contractual terms are missing or ambiguous, requiring legal counsel clarification.
