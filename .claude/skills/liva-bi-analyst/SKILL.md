---
name: liva-bi-analyst
description: Synthesize Text-to-SQL queries, discover database schemas, compute business KPIs, and generate interactive charts from structured enterprise data. Use when analyzing database metrics, writing SQL queries, diagnosing business trends, generating financial/operational dashboards, or creating chart specifications.
---

# LIVA BI Analyst

## Workflow

1. **Schema Discovery & Metadata Introspection**:
   - Inspect the active target database connection (PostgreSQL, SQLite, MySQL).
   - Retrieve table definitions, column types, primary/foreign key relationships, nullability constraints, and index coverage.
   - Sample distinct categorical values and range distributions on target metrics to prevent hallucinated column joins or invalid enum filters.

2. **Semantic Metric Formulation & Business Logic Translation**:
   - Deconstruct user business questions into precise statistical and financial formulas:
     - **Revenue Metrics**: Monthly Recurring Revenue (MRR), Average Revenue Per User (ARPU), Customer Lifetime Value (LTV).
     - **Growth & Churn**: Month-over-Month (MoM) Growth, Churn Rate, Retention Cohort matrices.
     - **Operational KPIs**: Fulfillment Cycle Time, Order Velocity, SLA Breach ratios.
   - Define exact temporal aggregation windows (e.g., trailing 30 days, rolling 12 months, quarterly cohorts).

3. **Safe Text-to-SQL Query Synthesis & AST Validation**:
   - Synthesize standard SQL conforming to the database dialect (PostgreSQL / SQLite).
   - Pass synthesized query through a **Fail-Closed AST Validator**:
     - Allow ONLY `SELECT` and `WITH` (CTE) expressions.
     - Reject all mutating statements (`INSERT`, `UPDATE`, `DELETE`, `DROP`, `ALTER`, `TRUNCATE`, `GRANT`, `EXEC`, `CREATE`).
     - Block multiple statement chaining (semicolon injections) and inline comment evasion patterns.

4. **Query Plan Inspection & Performance Guarding (`EXPLAIN`)**:
   - Run query cost analysis before running against production tables.
   - Verify index utilization and detect sequential table scans (`Seq Scan`) on large volume tables.
   - Enforce query timeout limits (e.g., maximum 5,000ms) and limit result sets with mandatory `LIMIT` clauses (maximum 1,000 rows).

5. **Query Execution & Result Aggregation**:
   - Execute query in a strictly read-only transaction pool (`SET TRANSACTION READ ONLY`).
   - Parse result sets into structured tabular formats, computing aggregate summary statistics (Mean, Median, P95, Standard Deviation).

6. **Chart Specification & Executive Dashboard Synthesis**:
   - Generate declarative visualization specifications (Vega-Lite / Mermaid):
     - **Time Series / Trends**: Line charts, area charts with rolling moving averages.
     - **Categorical Comparisons**: Bar charts, stacked columns with percentage distributions.
     - **Cohort / Distribution**: Heatmaps, box plots, and scatter plots.
   - Synthesize an executive summary highlighting actionable drivers, anomalies, and strategic insights.

7. **Vault Persistence & Knowledge Graph Curation**:
   - Save the completed analysis into `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/BI - <Analysis_Title>.md` using `write_markdown`.
   - Adhere strictly to the Obsidian Vault Dialect (`title`, `tags: [liva/knowledge, liva/analytics, bi/sql]`, `author: "codex"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Hybrid. Schema discovery, query cost estimation, and chart generation execute automatically (`Auto`). Query execution runs under strict read-only transaction isolation.
- **Tool Dependencies**: Requires database connection tools and `obsidian` MCP (`write_markdown`, `search_vault`).
- **Query Safety Invariants**: Strictly read-only; mutations are rejected at AST parsing layer. Maximum execution time: 5,000ms; maximum returned rows: 1,000.
- **Data Privacy**: Queries must not extract raw unmasked personal customer credentials or plain-text passwords.

## Stop Conditions

Stop and report immediately when:
- The generated SQL query contains any mutating or DDL keyword (`DROP`, `DELETE`, `UPDATE`, `ALTER`, `TRUNCATE`, `INSERT`).
- The query execution plan indicates a full sequential scan on an unindexed table containing > 500,000 rows without partition pruning.
- Database connection fails, connection pool is exhausted, or credentials lack read authorization on the requested schema.
- Ambiguous column definitions or missing relational keys prevent deterministic join resolution without user clarification.
