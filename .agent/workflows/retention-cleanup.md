---
description: Apply retention policy to generated reports
---

# Retention Cleanup Workflow

Apply the documentation retention policy to keep report folders manageable.

## Retention Rules

| Report Type | Location | Keep |
|:------------|:---------|:-----|
| Code Review | `docs/reports/code-review/` | Last **5** reports |
| Spring Cleaning | `docs/reports/spring-cleaning/` | Last **3** reports |
| Architecture Review | `docs/reports/architecture-review/` | Last **3** reports |
| Archive | `docs/reports/archive/` | Delete after **90 days** |

## Execution Steps

1. **Code Review Reports**:
   - List all files in `docs/reports/code-review/`
   - Sort by date (newest first)
   - Keep the 5 most recent files
   - Move older files to `docs/reports/archive/`

2. **Spring Cleaning Reports**:
   - List all files in `docs/reports/spring-cleaning/`
   - Sort by date (newest first)
   - Keep the 3 most recent files
   - Move older files to `docs/reports/archive/`

3. **Architecture Review Reports**:
   - List all files in `docs/reports/architecture-review/`
   - Sort by date (newest first)
   - Keep the 3 most recent files
   - Move older files to `docs/reports/archive/`

4. **Archive Cleanup**:
   - List all files in `docs/reports/archive/`
   - Identify files older than 90 days (by filename date or file modified date)
   - Delete files older than 90 days

4. **Report Summary**:
   Report to user:
   - Files archived from code-review
   - Files archived from spring-cleaning
   - Files deleted from archive
   - Current counts in each folder
