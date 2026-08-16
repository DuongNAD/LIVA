---
name: business-intelligence-analyst
description: Synthesize Text-to-SQL queries, discover database schemas, compute business KPIs, and generate interactive charts from structured enterprise data. Use when analyzing database metrics, writing SQL queries, diagnosing business trends, generating financial/operational dashboards, or creating chart specifications.
---

# Business Intelligence & Text-to-SQL Analyst

## Workflow

1. **Schema Discovery & Metadata Introspection**:
   - Inspect the active target database connection (PostgreSQL, SQLite, MySQL) via `db:introspect_schema`.
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
   - Run query cost analysis via `db:explain_query` before running against production tables.
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
   - Save the completed analysis into `vault/Knowledge/BI - <Analysis_Title>.md` using `write_markdown`.
   - Adhere strictly to the Obsidian Vault Dialect (`title`, `tags: [knowledge/analytics, bi/sql]`, `author: "user"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Hybrid. Schema discovery, query cost estimation, and chart generation execute automatically (`Auto`). Query execution runs under strict read-only transaction isolation.
- **Tool Dependencies**: Requires `db` native module / MCP (`db:introspect_schema`, `db:explain_query`, `db:execute_query`, `db:generate_chart`) and `obsidian` MCP (`write_markdown`, `search_vault`).
- **Query Safety Invariants**: Strictly read-only; mutations are rejected at AST parsing layer. Maximum execution time: 5,000ms; maximum returned rows: 1,000.
- **Data Privacy**: Queries must not extract raw unmasked personal customer credentials or plain-text passwords.

## Stop Conditions

Stop and report immediately when:
- The generated SQL query contains any mutating or DDL keyword (`DROP`, `DELETE`, `UPDATE`, `ALTER`, `TRUNCATE`, `INSERT`).
- The query execution plan indicates a full sequential scan on an unindexed table containing > 500,000 rows without partition pruning.
- Database connection fails, connection pool is exhausted, or credentials lack read authorization on the requested schema.
- Ambiguous column definitions or missing relational keys prevent deterministic join resolution without user clarification.

## BI Analytics & Chart Specification Example

```markdown
---
title: "BI - Subscription Churn & Retention Cohort Analysis"
tags:
  - knowledge/analytics
  - bi/sql
author: "user"
last_update: "2026-08-14T12:00:00+07:00"
---

# BI Analysis: Subscription Churn & Retention Cohort Analysis

## 1. Executive Summary
- **Period**: Q1 2026 – Q2 2026 (Rolling 6 Months)
- **Key Finding**: Net Revenue Retention (NRR) improved from **94.2%** to **103.8%**, driven by tier upgrades in the Enterprise segment.
- **Churn Concentration**: SMB monthly plan churn remains elevated at **4.8%** due to onboarding friction.

## 2. Validated SQL Query
```sql
WITH monthly_cohorts AS (
    SELECT 
        DATE_TRUNC('month', created_at) AS cohort_month,
        user_id,
        plan_tier,
        mrr_amount
    FROM subscriptions
    WHERE created_at >= '2026-01-01'
),
retention_summary AS (
    SELECT 
        cohort_month,
        COUNT(DISTINCT user_id) AS initial_users,
        SUM(mrr_amount) AS total_cohort_mrr,
        AVG(mrr_amount) AS avg_arpu
    FROM monthly_cohorts
    GROUP BY cohort_month
)
SELECT 
    cohort_month,
    initial_users,
    total_cohort_mrr,
    ROUND(avg_arpu, 2) AS arpu
FROM retention_summary
ORDER BY cohort_month ASC
LIMIT 100;
```

## 3. Vega-Lite Visualization Specification
```json
{
  "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
  "title": "Monthly Cohort MRR & User Trajectory",
  "data": {
    "values": [
      {"cohort_month": "2026-01-01", "total_cohort_mrr": 45200, "initial_users": 320},
      {"cohort_month": "2026-02-01", "total_cohort_mrr": 52400, "initial_users": 380},
      {"cohort_month": "2026-03-01", "total_cohort_mrr": 61800, "initial_users": 430},
      {"cohort_month": "2026-04-01", "total_cohort_mrr": 74500, "initial_users": 510}
    ]
  },
  "mark": {"type": "bar", "cornerRadiusTopLeft": 4, "cornerRadiusTopRight": 4},
  "encoding": {
    "x": {"field": "cohort_month", "type": "temporal", "title": "Cohort Month"},
    "y": {"field": "total_cohort_mrr", "type": "quantitative", "title": "Total MRR ($)"},
    "color": {"value": "#3b82f6"},
    "tooltip": [
      {"field": "cohort_month", "type": "temporal", "title": "Month"},
      {"field": "total_cohort_mrr", "type": "quantitative", "title": "MRR ($)"},
      {"field": "initial_users", "type": "quantitative", "title": "Active Users"}
    ]
  }
}
```
```
