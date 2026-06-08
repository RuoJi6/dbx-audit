# Database Scan GUI parity checklist

This document tracks the legacy `database_scan/gui` behavior that the DBX audit rewrite must preserve or intentionally replace with a DBX-native equivalent.

## Product shell

- No separate login or vault unlock screen in DBX. Use DBX's existing app startup, connection storage, settings storage, and local file dialogs.
- Audit must not block DBX navigation. Opening query tabs, driver manager, transfer, SQL file execution, schema diff, data compare, settings, or connection dialogs must leave the audit workspace.
- Audit forms should persist non-sensitive draft settings through DBX local settings. Do not persist raw fscan text by default because it may contain passwords.

## Legacy GUI task model

- Task list with search and status overview.
- Task kinds:
  - Single database audit.
  - fscan batch audit.
  - Custom SQL execution task.
- Task create/edit wizard with task name, description, target settings, scan settings, and output settings.
- Start, stop, delete, refresh task.
- Poll running tasks and preserve completed task result state.
- Detail tabs:
  - Hits by table.
  - Sensitive fields.
  - fscan targets.
  - SQL result.
  - Row samples.
  - Logs.

## Target and connection options

- Database type selection with default ports.
- Supported types from legacy core:
  - MySQL family: mysql, mariadb, tidb, oceanbase, polardb-mysql, doris, starrocks, gbase-mysql.
  - MSSQL.
  - PostgreSQL family: postgres, opengauss, gaussdb, kingbase, highgo, polardb-postgres.
  - Oracle.
  - Redis.
- Host, port, user, password.
- Optional database filter. Multiple databases are supported by the CLI.
- Optional table filter. Supports `table`, `schema.table`, and comma-separated values.
- Proxy URL.
- Connection test with version, resolved address, server time, and Redis info when applicable.

## Scan options

- Scan modes:
  - `field-content`
  - `field-name`
  - `content`
  - `all`
- Risk level filters:
  - `all`
  - `high`
  - `medium`
  - `low`
- Sample limit.
- Worker count for table-level parallel scanning.
- Timeout.
- Include system databases toggle.
- Mask sensitive values toggle.
- Text encoding repair options:
  - auto
  - utf8
  - gbk / cp936
  - gb18030
  - big5
  - shift-jis
  - euc-kr
  - latin1 / iso-8859-1
  - windows-1252

## fscan support

- Choose fscan result file.
- Paste or manually compose targets.
- Manual target list with add/remove/clear.
- Parse preview with total targets and per-target type, host, port, user, source line, raw text, and password presence.
- Test individual manual target connection.
- Batch scan parsed targets.
- Split output: write one independent Excel report per target, plus summary output.

## Results and evidence

- Table-level hit summary with database, schema, table, hit count, sensitive fields, and sample row count.
- Sensitive field list with kind, risk level, mode, and hit count.
- Evidence filters:
  - Search fields.
  - Filter by risk level.
  - Sort fields by hit count ascending/descending.
  - Search sample values.
  - Search sample metadata such as field name, level, kind, and mode.
- Row samples should show whole rows, not only the matching cell, when the DBX driver can fetch them safely.
- Sensitive fields and values should be visually marked by risk level.
- Errors must be grouped and visible in task detail.
- Logs must keep recent entries and show progress messages.

## Redis audit

- Scan Redis DB selection from the database option.
- Enumerate keys with SCAN.
- Read supported value types:
  - string
  - hash
  - list
  - set
  - zset
- Result fields:
  - target
  - DB
  - key
  - type
  - TTL
  - path / field
  - value
  - hit kind
  - risk level
  - basis
- Export Redis summary and Redis key details.

## Export and backup

- Choose output path.
- XLSX report output.
- JSON should remain the DBX rewrite's stable intermediate format.
- Open output folder.
- Legacy GUI backup import/export maps to DBX-native audit task export/import, not a separate login vault.
- Optional encrypted backup can be revisited later, but it should not force a startup login screen.

## Current MVP gaps

- Current DBX rewrite has a basic audit workspace, Tauri commands, field-name findings, fscan parsing, JSON/XLSX export, and i18n shell.
- Missing or incomplete:
  - DBX-native persistent audit task list.
  - Full field-content/content row sampling.
  - Redis key/value audit path.
  - fscan batch scan workflow.
  - Connection test UI.
  - Text encoding repair.
  - Risk-colored result table and row evidence views.
  - Split output.
  - Import/export of audit task history.
