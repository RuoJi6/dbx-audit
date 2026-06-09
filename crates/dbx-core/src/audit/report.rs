use std::collections::BTreeMap;

use serde_json::Value;

use super::types::{
    AuditFinding, AuditJobState, AuditKind, AuditLevel, AuditLogEntry, AuditMode, AuditTableEvidence, AuditTableField,
    AuditTargetSummary,
};
use crate::xlsx_export::{build_xlsx_workbook_multi, XlsxCellData, XlsxWorksheetData};

const STYLE_HIGH: usize = 2;
const STYLE_MEDIUM: usize = 3;
const STYLE_LOW: usize = 4;

pub fn audit_job_to_json(job: &AuditJobState) -> Result<String, String> {
    serde_json::to_string_pretty(job).map_err(|err| err.to_string())
}

pub fn audit_job_to_xlsx(job: &AuditJobState) -> Result<Vec<u8>, String> {
    let sql_tables = sql_tables(job);
    let redis_findings = job.findings.iter().filter(|finding| is_redis_finding(finding)).cloned().collect::<Vec<_>>();
    let mut sheets = Vec::new();

    if !job.target_summaries.is_empty() {
        sheets.push(target_overview_sheet(&job.target_summaries));
    }
    if sql_tables.is_empty() && redis_findings.is_empty() {
        sheets.push(no_findings_sheet());
    } else {
        if !sql_tables.is_empty() {
            sheets.push(summary_sheet(&sql_tables));
            for table in &sql_tables {
                sheets.push(table_detail_sheet(table, findings_for_table(&job.findings, table)));
            }
        }
        if !redis_findings.is_empty() {
            sheets.push(redis_summary_sheet(&redis_findings));
            sheets.push(redis_keys_sheet(&redis_findings));
        }
    }
    if !job.logs.is_empty() {
        sheets.push(logs_sheet(&job.logs));
    }
    if !job.errors.is_empty() {
        sheets.push(errors_sheet(&job.errors));
    }
    build_xlsx_workbook_multi(&sheets)
}

pub fn audit_findings_to_xlsx(findings: &[AuditFinding]) -> Result<Vec<u8>, String> {
    let job = AuditJobState {
        job_id: "findings".to_string(),
        status: super::types::AuditJobStatus::Completed,
        progress: 100,
        request: super::types::AuditScanRequest {
            connection_id: String::new(),
            connection: None,
            database: None,
            schema: None,
            tables: Vec::new(),
            mode: super::types::AuditMode::FieldContent,
            level: super::types::AuditLevelFilter::All,
            limit: 15,
            mask: false,
            include_system: false,
            workers: 1,
            table_workers: None,
            field_workers: None,
            timeout_secs: 15,
        },
        logs: Vec::new(),
        findings: findings.to_vec(),
        table_results: Vec::new(),
        target_summaries: Vec::new(),
        errors: Vec::new(),
        started_at: String::new(),
        finished_at: None,
    };
    audit_job_to_xlsx(&job)
}

fn sql_tables(job: &AuditJobState) -> Vec<AuditTableEvidence> {
    let mut tables = job
        .table_results
        .iter()
        .filter(|table| table.schema.as_deref() != Some("redis-key") && table.db_type.as_deref() != Some("redis"))
        .cloned()
        .collect::<Vec<_>>();
    if tables.is_empty() {
        tables = fallback_tables_from_findings(&job.findings);
    }
    tables.sort_by(|a, b| table_sort_key(a).cmp(&table_sort_key(b)));
    tables
}

fn fallback_tables_from_findings(findings: &[AuditFinding]) -> Vec<AuditTableEvidence> {
    let mut by_table = BTreeMap::<String, Vec<&AuditFinding>>::new();
    for finding in findings.iter().filter(|finding| !is_redis_finding(finding)) {
        by_table
            .entry(table_key_parts(
                &finding.connection_id,
                &finding.database,
                finding.schema.as_deref(),
                &finding.table,
            ))
            .or_default()
            .push(finding);
    }
    by_table
        .values()
        .map(|items| {
            let first = items[0];
            AuditTableEvidence {
                connection_id: first.connection_id.clone(),
                connection_name: first.connection_name.clone(),
                db_type: first.db_type.clone(),
                connection_host: first.connection_host.clone(),
                connection_port: first.connection_port,
                connection_user: first.connection_user.clone(),
                database: first.database.clone(),
                schema: first.schema.clone(),
                table: first.table.clone(),
                row_count: items.iter().map(|finding| finding.count).max().unwrap_or(0),
                columns: vec![
                    "字段名".to_string(),
                    "命中模式".to_string(),
                    "疑似类型".to_string(),
                    "样例值".to_string(),
                ],
                fields: table_fields_from_findings(items),
                rows: Vec::new(),
            }
        })
        .collect()
}

fn table_fields_from_findings(findings: &[&AuditFinding]) -> Vec<AuditTableField> {
    let mut fields = BTreeMap::<String, AuditTableField>::new();
    for finding in findings {
        let field = fields.entry(finding.column.clone()).or_insert_with(|| AuditTableField {
            name: finding.column.clone(),
            kinds: Vec::new(),
            level: finding.level,
            mode: finding.mode,
            total: finding.count,
        });
        if !field.kinds.contains(&finding.kind) {
            field.kinds.push(finding.kind);
        }
        if finding.level > field.level {
            field.level = finding.level;
        }
        field.total = field.total.max(finding.count);
    }
    fields.into_values().collect()
}

fn no_findings_sheet() -> XlsxWorksheetData {
    sheet("No Findings", vec![row(["结果"]), row(["未发现敏感信息命中"])])
}

fn target_overview_sheet(targets: &[AuditTargetSummary]) -> XlsxWorksheetData {
    let mut rows = vec![row(["连接结果概览"]), Vec::new()];
    rows.push(row(["数据库类型", "连接名称", "目标", "用户", "数据库", "状态", "命中数", "命中表数", "错误"]));
    for target in targets {
        let status = target_status_label(&target.status, target.finding_count, target.error.as_deref());
        rows.push(vec![
            cell(database_type_label(target.db_type.as_deref()), None),
            cell(target.connection_name.clone().or_else(|| target.connection_id.clone()).unwrap_or_default(), None),
            cell(target_summary_address(target), None),
            cell(target.connection_user.clone().unwrap_or_default(), None),
            cell(target.database.clone().unwrap_or_default(), None),
            cell(status, target_status_style(target)),
            cell(target.finding_count.to_string(), None),
            cell(target.table_count.to_string(), None),
            cell(target.error.clone().unwrap_or_default(), None),
        ]);
    }
    sheet("连接结果概览", rows)
}

fn summary_sheet(tables: &[AuditTableEvidence]) -> XlsxWorksheetData {
    let mut rows = vec![row(["敏感信息汇总"]), Vec::new()];
    for (index, table) in tables.iter().enumerate() {
        if index > 0 {
            rows.push(Vec::new());
        }
        rows.extend(audit_target_rows(table));
        rows.push(row(["[数据库]", table.database.as_str()]));
        rows.push(row(["[表]", &format!("{}【实际数据行数：{}】", table_label(table), table.row_count)]));
        for field in &table.fields {
            rows.push(vec![
                cell(format!("{}（{}）：", kind_label_many(&field.kinds), level_label(field.level)), None),
                cell(field.name.clone(), Some(style_for_level(field.level))),
                cell(format!("（存在行数：{}）", field.total), None),
            ]);
        }
    }
    sheet("敏感信息汇总", rows)
}

fn table_detail_sheet(table: &AuditTableEvidence, findings: Vec<&AuditFinding>) -> XlsxWorksheetData {
    let mut rows = audit_target_rows(table)
        .into_iter()
        .map(|mut row| {
            if let Some(first) = row.first_mut() {
                if let Value::String(value) = &mut first.value {
                    *value = value.trim_matches(['[', ']']).to_string();
                }
            }
            row
        })
        .collect::<Vec<_>>();
    rows.extend([
        row(["数据库", table.database.as_str()]),
        row(["Schema", table.schema.as_deref().unwrap_or("")]),
        row(["表名", table.table.as_str()]),
        row(["实际数据行数", &table.row_count.to_string()]),
        Vec::new(),
        row(["敏感字段清单"]),
        row(["字段名", "疑似类型", "敏感级别", "判断依据", "字段非空行数"]),
    ]);
    for field in &table.fields {
        rows.push(vec![
            cell(field.name.clone(), Some(style_for_level(field.level))),
            cell(kind_label_many(&field.kinds), None),
            cell(level_label(field.level), None),
            cell(mode_label(field.mode), None),
            cell(field.total.to_string(), None),
        ]);
    }
    rows.push(Vec::new());
    rows.push(row(["真实样例数据"]));
    if table.rows.is_empty() {
        rows.push(row(["字段名", "命中模式", "疑似类型", "样例值"]));
        for finding in findings {
            if finding.samples.is_empty() {
                rows.push(row([finding.column.as_str(), mode_label(finding.mode), kind_label(finding.kind), ""]));
            } else {
                for sample in &finding.samples {
                    rows.push(row([
                        finding.column.as_str(),
                        mode_label(finding.mode),
                        kind_label(finding.kind),
                        sample.value.as_str(),
                    ]));
                }
            }
        }
    } else {
        let field_styles = table_field_styles(table);
        rows.push(table.columns.iter().map(|column| cell(column.clone(), field_styles.get(column).copied())).collect());
        for sample in &table.rows {
            rows.push(
                table
                    .columns
                    .iter()
                    .map(|column| {
                        cell(sample.get(column).cloned().unwrap_or_default(), field_styles.get(column).copied())
                    })
                    .collect(),
            );
        }
    }
    sheet(&table_sheet_name(table), rows)
}

fn redis_summary_sheet(findings: &[AuditFinding]) -> XlsxWorksheetData {
    let mut rows = vec![row(["Redis 敏感 Key 汇总"]), Vec::new(), redis_header(false)];
    for finding in findings {
        rows.push(redis_row(finding, false));
    }
    sheet("Redis 汇总", rows)
}

fn redis_keys_sheet(findings: &[AuditFinding]) -> XlsxWorksheetData {
    let mut rows = vec![row(["Redis 敏感 Key 明细"]), Vec::new(), redis_header(true)];
    for finding in findings {
        rows.push(redis_row(finding, true));
    }
    sheet("Redis Keys", rows)
}

fn logs_sheet(logs: &[AuditLogEntry]) -> XlsxWorksheetData {
    let mut rows = vec![row(["时间", "级别", "内容"])];
    rows.extend(logs.iter().map(|entry| row([entry.time.as_str(), entry.level.as_str(), entry.message.as_str()])));
    sheet("运行日志", rows)
}

fn errors_sheet(errors: &[String]) -> XlsxWorksheetData {
    let mut rows = vec![row(["错误"])];
    rows.extend(errors.iter().map(|entry| row([entry.as_str()])));
    sheet("错误", rows)
}

fn redis_header(with_value: bool) -> Vec<XlsxCellData> {
    if with_value {
        row([
            "数据库类型",
            "Target",
            "DB",
            "Key",
            "Redis类型",
            "TTL",
            "Path/Field",
            "Value",
            "命中类型",
            "敏感级别",
            "判断依据",
        ])
    } else {
        row(["数据库类型", "Target", "DB", "Key", "Redis类型", "TTL", "Path/Field", "命中类型", "敏感级别", "判断依据"])
    }
}

fn redis_row(finding: &AuditFinding, with_value: bool) -> Vec<XlsxCellData> {
    let mut values = vec![
        cell(database_type_label(finding.db_type.as_deref()), None),
        cell(finding.connection_name.clone().or_else(|| finding.connection_id.clone()).unwrap_or_default(), None),
        cell(finding.database.trim_start_matches("redis-db").to_string(), None),
        cell(finding.table.clone(), None),
        cell(finding.data_type.clone().unwrap_or_else(|| "redis".to_string()), None),
        cell("", None),
        cell(finding.column.clone(), None),
    ];
    if with_value {
        values.push(cell(sample_text(finding), Some(style_for_level(finding.level))));
    }
    values.extend([
        cell(kind_label(finding.kind), Some(style_for_level(finding.level))),
        cell(level_label(finding.level), None),
        cell(finding.basis.clone(), None),
    ]);
    values
}

fn sheet(name: &str, cells: Vec<Vec<XlsxCellData>>) -> XlsxWorksheetData {
    XlsxWorksheetData { sheet_name: Some(name.to_string()), cells, columns: Vec::new(), rows: Vec::new() }
}

fn row<const N: usize>(values: [&str; N]) -> Vec<XlsxCellData> {
    values.into_iter().map(|value| cell(value, None)).collect()
}

fn cell(value: impl Into<String>, style: Option<usize>) -> XlsxCellData {
    XlsxCellData { value: Value::String(value.into()), style }
}

fn table_sort_key(table: &AuditTableEvidence) -> String {
    format!(
        "{}\u{0}{}",
        table.db_type.clone().unwrap_or_default(),
        table_key_parts(&table.connection_id, &table.database, table.schema.as_deref(), &table.table)
    )
}

fn table_key_parts(connection_id: &Option<String>, database: &str, schema: Option<&str>, table: &str) -> String {
    format!(
        "{}\u{0}{database}\u{0}{}\u{0}{table}",
        connection_id.clone().unwrap_or_default(),
        schema.unwrap_or_default()
    )
}

fn findings_for_table<'a>(findings: &'a [AuditFinding], table: &AuditTableEvidence) -> Vec<&'a AuditFinding> {
    findings
        .iter()
        .filter(|finding| {
            finding.connection_id == table.connection_id
                && finding.database == table.database
                && finding.schema == table.schema
                && finding.table == table.table
        })
        .collect()
}

fn table_label(table: &AuditTableEvidence) -> String {
    match table.schema.as_deref().filter(|schema| !schema.trim().is_empty()) {
        Some(schema) => format!("{schema}.{}", table.table),
        None => table.table.clone(),
    }
}

fn table_sheet_name(table: &AuditTableEvidence) -> String {
    let source = table
        .connection_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| table.db_type.as_deref())
        .unwrap_or("db");
    let table_name = table.table.as_str();
    let schema = table.schema.as_deref().filter(|value| !value.trim().is_empty());
    let name = match schema {
        Some("document") | Some("index") | Some("redis-key") | None => format!("{source}.{table_name}"),
        Some(schema) => format!("{source}.{schema}.{table_name}"),
    };
    compact_sheet_name(&name)
}

fn compact_sheet_name(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| match ch {
            '[' | ']' | ':' | '*' | '?' | '/' | '\\' => '_',
            _ => ch,
        })
        .collect::<String>();
    if cleaned.chars().count() <= 31 {
        return cleaned;
    }
    let parts = cleaned.split('.').filter(|part| !part.is_empty()).collect::<Vec<_>>();
    if parts.len() >= 2 {
        let head = parts[0];
        let tail = parts[parts.len() - 1];
        let tail_len = tail.chars().count();
        let sep_len = 1;
        let head_limit = 31usize.saturating_sub(tail_len + sep_len).max(8);
        let head_short = head.chars().take(head_limit).collect::<String>();
        return format!("{head_short}.{tail}").chars().take(31).collect();
    }
    cleaned.chars().take(31).collect()
}

fn audit_target_rows(table: &AuditTableEvidence) -> Vec<Vec<XlsxCellData>> {
    vec![
        row(["[数据库类型]", database_type_label(table.db_type.as_deref()).as_str()]),
        row(["[连接来源]", table.connection_name.as_deref().unwrap_or("")]),
        row(["[目标]", target_address(table).as_str()]),
        row(["[用户]", table.connection_user.as_deref().unwrap_or("")]),
    ]
}

fn target_address(table: &AuditTableEvidence) -> String {
    match (table.connection_host.as_deref(), table.connection_port) {
        (Some(host), Some(port)) if !host.trim().is_empty() => format!("{host}:{port}"),
        (Some(host), _) => host.to_string(),
        _ => String::new(),
    }
}

fn target_summary_address(target: &AuditTargetSummary) -> String {
    match (target.connection_host.as_deref(), target.connection_port) {
        (Some(host), Some(port)) if !host.trim().is_empty() => format!("{host}:{port}"),
        (Some(host), _) => host.to_string(),
        _ => String::new(),
    }
}

fn database_type_label(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_ascii_lowercase()
}

fn target_status_label(status: &str, finding_count: usize, error: Option<&str>) -> String {
    if error.is_some_and(|value| !value.trim().is_empty()) {
        return "失败".to_string();
    }
    match status.trim().to_ascii_lowercase().as_str() {
        "failed" | "error" => "失败".to_string(),
        "no-hit" | "no_hits" | "no-hits" => "无命中".to_string(),
        "hit" | "hits" => "有命中".to_string(),
        "completed" | "success" | "ok" if finding_count > 0 => "有命中".to_string(),
        "completed" | "success" | "ok" => "无命中".to_string(),
        other if other.is_empty() && finding_count > 0 => "有命中".to_string(),
        other if other.is_empty() => "无命中".to_string(),
        _ if finding_count > 0 => "有命中".to_string(),
        _ => status.to_string(),
    }
}

fn target_status_style(target: &AuditTargetSummary) -> Option<usize> {
    if target.error.as_deref().is_some_and(|value| !value.trim().is_empty()) {
        return Some(STYLE_HIGH);
    }
    match target.status.trim().to_ascii_lowercase().as_str() {
        "failed" | "error" => Some(STYLE_HIGH),
        "hit" | "hits" => Some(STYLE_MEDIUM),
        "completed" | "success" | "ok" if target.finding_count > 0 => Some(STYLE_MEDIUM),
        _ => None,
    }
}

fn table_field_styles(table: &AuditTableEvidence) -> BTreeMap<String, usize> {
    table.fields.iter().map(|field| (field.name.clone(), style_for_level(field.level))).collect()
}

fn is_redis_finding(finding: &AuditFinding) -> bool {
    finding.db_type.as_deref() == Some("redis") || finding.schema.as_deref() == Some("redis-key")
}

fn kind_label_many(kinds: &[AuditKind]) -> String {
    let mut seen = Vec::new();
    for kind in kinds {
        if !seen.contains(kind) {
            seen.push(*kind);
        }
    }
    seen.into_iter().map(kind_label).collect::<Vec<_>>().join("、")
}

fn kind_label(kind: AuditKind) -> &'static str {
    match kind {
        AuditKind::Phone => "手机号",
        AuditKind::Email => "邮箱",
        AuditKind::IdCard => "身份证",
        AuditKind::BankCard => "银行卡",
        AuditKind::PasswordSecret => "密码/密钥",
        AuditKind::TokenSecret => "令牌/Token",
        AuditKind::Address => "地址",
        AuditKind::Username => "用户名",
        AuditKind::Account => "账号",
        AuditKind::IpAddress => "IP 地址",
        AuditKind::BusinessIdentifier => "业务标识",
        AuditKind::RiskEvidence => "风险/证据",
    }
}

fn level_label(level: AuditLevel) -> &'static str {
    match level {
        AuditLevel::High => "高敏",
        AuditLevel::Medium => "中敏",
        AuditLevel::Low => "低敏",
    }
}

fn mode_label(mode: AuditMode) -> &'static str {
    match mode {
        AuditMode::FieldContent => "字段名+内容",
        AuditMode::FieldName => "字段名",
        AuditMode::Content => "内容",
        AuditMode::All => "全部",
    }
}

fn style_for_level(level: AuditLevel) -> usize {
    match level {
        AuditLevel::High => STYLE_HIGH,
        AuditLevel::Medium => STYLE_MEDIUM,
        AuditLevel::Low => STYLE_LOW,
    }
}

fn sample_text(finding: &AuditFinding) -> String {
    finding.samples.iter().map(|sample| sample.value.clone()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::{audit_findings_to_xlsx, audit_job_to_json, audit_job_to_xlsx};
    use crate::audit::types::{
        AuditFinding, AuditJobState, AuditJobStatus, AuditKind, AuditLevel, AuditLevelFilter, AuditLogEntry, AuditMode,
        AuditSample, AuditScanRequest, AuditTableEvidence, AuditTableField, AuditTargetSummary,
    };
    use std::collections::BTreeMap;

    fn finding() -> AuditFinding {
        AuditFinding {
            connection_id: Some("conn-1".to_string()),
            connection_name: Some("local".to_string()),
            db_type: Some("postgres".to_string()),
            connection_host: Some("127.0.0.1".to_string()),
            connection_port: Some(5432),
            connection_user: Some("audit".to_string()),
            database: "audit_demo".to_string(),
            schema: Some("public".to_string()),
            table: "users".to_string(),
            column: "email".to_string(),
            data_type: Some("text".to_string()),
            kind: AuditKind::Email,
            level: AuditLevel::Medium,
            mode: AuditMode::FieldContent,
            basis: "field-name".to_string(),
            count: 2,
            samples: vec![AuditSample { column: "email".to_string(), value: "a@example.com".to_string() }],
        }
    }

    fn redis_finding() -> AuditFinding {
        AuditFinding {
            connection_id: Some("redis-1".to_string()),
            connection_name: Some("Redis_q3u0".to_string()),
            db_type: Some("redis".to_string()),
            connection_host: Some("127.0.0.1".to_string()),
            connection_port: Some(6379),
            connection_user: Some("default".to_string()),
            database: "redis-db0".to_string(),
            schema: Some("redis-key".to_string()),
            table: "session:token".to_string(),
            column: "value".to_string(),
            data_type: Some("string".to_string()),
            kind: AuditKind::TokenSecret,
            level: AuditLevel::High,
            mode: AuditMode::Content,
            basis: "key+value".to_string(),
            count: 1,
            samples: vec![AuditSample { column: "value".to_string(), value: "sk_live_redis".to_string() }],
        }
    }

    fn table_result() -> AuditTableEvidence {
        let mut row = BTreeMap::new();
        row.insert("id".to_string(), "1".to_string());
        row.insert("email".to_string(), "a@example.com".to_string());
        AuditTableEvidence {
            connection_id: Some("conn-1".to_string()),
            connection_name: Some("local".to_string()),
            db_type: Some("postgres".to_string()),
            connection_host: Some("127.0.0.1".to_string()),
            connection_port: Some(5432),
            connection_user: Some("audit".to_string()),
            database: "audit_demo".to_string(),
            schema: Some("public".to_string()),
            table: "users".to_string(),
            row_count: 2,
            columns: vec!["id".to_string(), "email".to_string()],
            fields: vec![AuditTableField {
                name: "email".to_string(),
                kinds: vec![AuditKind::Email],
                level: AuditLevel::Medium,
                mode: AuditMode::FieldContent,
                total: 2,
            }],
            rows: vec![row],
        }
    }

    fn job() -> AuditJobState {
        AuditJobState {
            job_id: "job".to_string(),
            status: AuditJobStatus::Completed,
            progress: 100,
            request: AuditScanRequest {
                connection_id: "local".to_string(),
                connection: None,
                database: Some("audit_demo".to_string()),
                schema: Some("public".to_string()),
                tables: vec!["users".to_string()],
                mode: AuditMode::FieldContent,
                level: AuditLevelFilter::All,
                limit: 15,
                mask: false,
                include_system: false,
                workers: 1,
                timeout_secs: 15,
            },
            logs: vec![AuditLogEntry {
                time: "12:00:00".to_string(),
                level: "info".to_string(),
                message: "done".to_string(),
            }],
            findings: vec![finding(), redis_finding()],
            table_results: vec![table_result()],
            target_summaries: vec![
                AuditTargetSummary {
                    connection_id: Some("conn-1".to_string()),
                    connection_name: Some("local".to_string()),
                    db_type: Some("postgres".to_string()),
                    connection_host: Some("127.0.0.1".to_string()),
                    connection_port: Some(5432),
                    connection_user: Some("audit".to_string()),
                    database: Some("audit_demo".to_string()),
                    status: "hit".to_string(),
                    finding_count: 2,
                    table_count: 1,
                    error: None,
                },
                AuditTargetSummary {
                    connection_id: Some("conn-2".to_string()),
                    connection_name: Some("empty-clickhouse".to_string()),
                    db_type: Some("clickhouse".to_string()),
                    connection_host: Some("127.0.0.1".to_string()),
                    connection_port: Some(8123),
                    connection_user: Some("audit".to_string()),
                    database: Some("audit_demo".to_string()),
                    status: "no-hit".to_string(),
                    finding_count: 0,
                    table_count: 0,
                    error: None,
                },
            ],
            errors: vec!["sample warning".to_string()],
            started_at: "2026-06-08T00:00:00Z".to_string(),
            finished_at: Some("2026-06-08T00:00:01Z".to_string()),
        }
    }

    #[test]
    fn serializes_job_to_json() {
        let json = audit_job_to_json(&job()).expect("json");
        assert!(json.contains("\"jobId\""));
        assert!(json.contains("\"tableResults\""));
    }

    #[test]
    fn builds_xlsx_for_findings() {
        let bytes = audit_findings_to_xlsx(&[finding()]).expect("xlsx");
        assert_eq!(bytes[0], 0x50);
        assert_eq!(bytes[1], 0x4b);
    }

    #[test]
    fn builds_legacy_style_xlsx_for_job() {
        let bytes = audit_job_to_xlsx(&job()).expect("xlsx");
        let text = String::from_utf8_lossy(&bytes);
        for expected in [
            "敏感信息汇总",
            "连接结果概览",
            "empty-clickhouse",
            "无命中",
            "local.public.users",
            "Redis 汇总",
            "Redis Keys",
            "数据库类型",
            "postgres",
            "127.0.0.1:5432",
            "[数据库]",
            "[表]",
            "实际数据行数",
            "存在行数",
            "敏感字段清单",
            "真实样例数据",
            "Redis 敏感 Key 明细",
        ] {
            assert!(text.contains(expected), "missing {expected}");
        }
        assert!(text.contains("s=\"3\""), "medium-risk cell should be styled");
        assert!(text.contains("sample warning"));
    }
}
