---
name: liva-financial-advisor
description: Analyze financial statements, forecast cash flows, manage personal/corporate budgets, and audit financial health with encrypted ledger security. Use when parsing bank statements (CSV/OFX), projecting cash flow, reviewing balance sheets, categorizing expenses, or auditing financial ratios.
---

# LIVA Financial Advisor

## Workflow

1. **Financial Document & Ledger Ingestion**:
   - Ingest multi-format financial data: bank statement CSV/OFX exports, corporate trial balances, and invoice ledgers.
   - Parse transaction records with schema normalization (Date, Merchant/Payee, Description, Debit/Credit, Currency, Running Balance).

2. **Categorization & Machine Learning Classification**:
   - Assign standardized financial taxonomy categories (e.g., OPEX, COGS, CAPEX, Payroll, Tax, Discretionary Spend).
   - Detect recurring subscription anomalies, sudden price increases, and duplicate billing charges.

3. **Financial Statement Analysis & KPI Computation**:
   - **Corporate Financials**:
     - Income Statement: Gross Margin, EBITDA, Net Operating Profit After Tax (NOPAT).
     - Balance Sheet: Current Ratio, Quick Ratio, Debt-to-Equity (D/E).
     - Cash Flow: Operating Cash Flow (OCF), Free Cash Flow (FCF), Cash Burn Rate, Runway.
   - **Personal / SMB Wealth**:
     - Monthly Savings Rate, Emergency Fund Runway, Debt-to-Income (DTI).

4. **Rolling Cash Flow Forecasting & Variance Modeling**:
   - Generate rolling 30-day and 90-day cash flow projections based on historical transaction velocity and scheduled recurring commitments.
   - Model scenario stress tests (e.g., -20% revenue drop, delayed customer payments, inflation indexation).

5. **Security, Zeroization & Encrypted Local Storage**:
   - Encrypt raw financial ledger records and account balances in local SQLite using AES-256-GCM under user-managed master keys.
   - Mask sensitive account numbers, tax IDs, and banking details in all generated reports unless explicit unmasking is authorized.

6. **Dossier Generation & Obsidian Persistence**:
   - Save executive financial briefings to `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Finance - <Financial_Audit_Title>.md` using `write_markdown`.
   - Adhere strictly to the Obsidian frontmatter standard (`title`, `tags: [liva/knowledge, liva/finance, wealth/audit]`, `author: "codex"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Local Confidential. All computation, statement parsing, and ratio calculations run locally without telemetry or external API leaking.
- **Tool Dependencies**: Requires encrypted local SQLite ledger and `obsidian` MCP (`write_markdown`, `search_vault`).
- **Mathematical Invariants**: Double-entry balancing strictly enforced ($\text{Assets} = \text{Liabilities} + \text{Equity}$). Zero mathematical discrepancies permitted in balance sheets.
- **Data Compliance**: Compliant with Decree 13/2023/NĐ-CP and GDPR personal financial data protection standards.

## Stop Conditions

Stop and report immediately when:
- Balance sheet fails the fundamental double-entry equation ($\text{Assets} \ne \text{Liabilities} + \text{Equity}$).
- Unresolved bank statement debit/credit reconciliation errors exceed configured variance tolerance.
- Financial data storage is unencrypted or master key is missing.
- Ingestion data contains unmasked banking passwords, PINs, or CVV security codes.
