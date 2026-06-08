use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::types::{AuditFinding, AuditJobState, AuditLogEntry};
use crate::xlsx_export::{build_xlsx_workbook_multi, XlsxWorksheetData};

pub fn audit_job_to_json(job: &AuditJobState) -> Result<String, String> {
    serde_json::to_string_pretty(job).map_err(|err| err.to_string())
}

pub fn audit_job_to_xlsx(job: &AuditJobState) -> Result<Vec<u8>, String> {
    let mut sheets = Vec::new();
    sheets.push(summary_sheet(&job.findings));
    sheets.extend(table_detail_sheets(&job.findings));
    if job.findings.iter().any(is_redis_finding) {
        sheets.push(redis_sheet(&job.findings));
    }
    sheets.push(findings_sheet(&job.findings));
    if !job.logs.is_empty() {
        sheets.push(logs_sheet(&job.logs));
    }
    if !job.errors.is_empty() {
        sheets.push(errors_sheet(&job.errors));
    }
    build_xlsx_workbook_multi(&sheets)
}

pub fn audit_findings_to_xlsx(findings: &[AuditFinding]) -> Result<Vec<u8>, String> {
    build_xlsx_workbook_multi(&[
        summary_sheet(findings),
        findings_sheet(findings),
        redis_sheet(findings),
    ])
}

fn summary_sheet(findings: &[AuditFinding]) -> XlsxWorksheetData {
    let mut by_table = BTreeMap::<String, Vec<&AuditFinding>>::new();
    for finding in findings {
        by_table.entry(table_key(finding)).or_default().push(finding);
    }
    let rows = by_table
        .values()
        .map(|items| {
            let first = items[0];
            json_row(vec![
                first.connection_name.clone().or_else(|| first.connection_id.clone()).unwrap_or_default(),
                first.db_type.clone().unwrap_or_default(),
                first.database.clone(),
                first.schema.clone().unwrap_or_default(),
                first.table.clone(),
                items.iter().map(|item| item.column.clone()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", "),
                highest_level(items),
                items.iter().map(|item| item.count).sum::<u64>().to_string(),
            ])
        })
        .collect();
    XlsxWorksheetData {
        sheet_name: Some("敏感信息汇总".to_string()),
        columns: vec![
            "连接".into(),
            "类型".into(),
            "数据库".into(),
            "Schema".into(),
            "表/Key".into(),
            "敏感字段".into(),
            "最高风险".into(),
            "存在行数".into(),
        ],
        rows,
    }
}

fn table_detail_sheets(findings: &[AuditFinding]) -> Vec<XlsxWorksheetData> {
    let mut by_table = BTreeMap::<String, Vec<&AuditFinding>>::new();
    for finding in findings.iter().filter(|finding| !is_redis_finding(finding)) {
        by_table.entry(table_key(finding)).or_default().push(finding);
    }
    by_table
        .values()
        .map(|items| {
            let first = items[0];
            XlsxWorksheetData {
                sheet_name: Some(sheet_name(first)),
                columns: vec![
                    "字段".into(),
                    "疑似类型".into(),
                    "敏感级别".into(),
                    "判断依据".into(),
                    "存在行数".into(),
                    "样例".into(),
                ],
                rows: items.iter().map(|finding| finding_row(finding)).collect(),
            }
        })
        .collect()
}

fn redis_sheet(findings: &[AuditFinding]) -> XlsxWorksheetData {
    XlsxWorksheetData {
        sheet_name: Some("Redis Keys".to_string()),
        columns: vec![
            "连接".into(),
            "DB".into(),
            "Key".into(),
            "字段".into(),
            "命中类型".into(),
            "敏感级别".into(),
            "判断依据".into(),
            "样例".into(),
        ],
        rows: findings
            .iter()
            .filter(|finding| is_redis_finding(finding))
            .map(|finding| {
                json_row(vec![
                    finding.connection_name.clone().or_else(|| finding.connection_id.clone()).unwrap_or_default(),
                    finding.database.clone(),
                    finding.table.clone(),
                    finding.column.clone(),
                    kind_label(finding),
                    level_label(finding),
                    finding.basis.clone(),
                    sample_text(finding),
                ])
            })
            .collect(),
    }
}

fn findings_sheet(findings: &[AuditFinding]) -> XlsxWorksheetData {
    XlsxWorksheetData {
        sheet_name: Some("命中明细".to_string()),
        columns: vec![
            "连接".into(),
            "类型".into(),
            "Database".into(),
            "Schema".into(),
            "Table/Key".into(),
            "Column/Field".into(),
            "Kind".into(),
            "Level".into(),
            "Mode".into(),
            "Basis".into(),
            "Count".into(),
            "Samples".into(),
        ],
        rows: findings
            .iter()
            .map(|finding| {
                json_row(vec![
                    finding.connection_name.clone().or_else(|| finding.connection_id.clone()).unwrap_or_default(),
                    finding.db_type.clone().unwrap_or_default(),
                    finding.database.clone(),
                    finding.schema.clone().unwrap_or_default(),
                    finding.table.clone(),
                    finding.column.clone(),
                    kind_label(finding),
                    level_label(finding),
                    mode_label(finding),
                    finding.basis.clone(),
                    finding.count.to_string(),
                    sample_text(finding),
                ])
            })
            .collect(),
    }
}

fn logs_sheet(logs: &[AuditLogEntry]) -> XlsxWorksheetData {
    XlsxWorksheetData {
        sheet_name: Some("运行日志".to_string()),
        columns: vec!["时间".into(), "级别".into(), "内容".into()],
        rows: logs.iter().map(|entry| json_row(vec![entry.time.clone(), entry.level.clone(), entry.message.clone()])).collect(),
    }
}

fn errors_sheet(errors: &[String]) -> XlsxWorksheetData {
    XlsxWorksheetData {
        sheet_name: Some("错误".to_string()),
        columns: vec!["错误".into()],
        rows: errors.iter().map(|entry| vec![json!(entry)]).collect(),
    }
}

fn finding_row(finding: &AuditFinding) -> Vec<Value> {
    json_row(vec![
        finding.column.clone(),
        kind_label(finding),
        level_label(finding),
        finding.basis.clone(),
        finding.count.to_string(),
        sample_text(finding),
    ])
}

fn json_row(values: Vec<String>) -> Vec<Value> {
    values.into_iter().map(Value::String).collect()
}

fn table_key(finding: &AuditFinding) -> String {
    format!(
        "{}\u{0}{}\u{0}{}\u{0}{}",
        finding.connection_id.clone().unwrap_or_default(),
        finding.database,
        finding.schema.clone().unwrap_or_default(),
        finding.table
    )
}

fn sheet_name(finding: &AuditFinding) -> String {
    [finding.database.as_str(), finding.schema.as_deref().unwrap_or_default(), finding.table.as_str()]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

fn highest_level(findings: &[&AuditFinding]) -> String {
    if findings.iter().any(|finding| level_label(finding) == "high") {
        "high".to_string()
    } else if findings.iter().any(|finding| level_label(finding) == "medium") {
        "medium".to_string()
    } else {
        "low".to_string()
    }
}

fn is_redis_finding(finding: &AuditFinding) -> bool {
    finding.db_type.as_deref() == Some("redis") || finding.schema.as_deref() == Some("redis-key")
}

fn kind_label(finding: &AuditFinding) -> String {
    format!("{:?}", finding.kind).to_ascii_lowercase()
}

fn level_label(finding: &AuditFinding) -> String {
    format!("{:?}", finding.level).to_ascii_lowercase()
}

fn mode_label(finding: &AuditFinding) -> String {
    format!("{:?}", finding.mode).to_ascii_lowercase()
}

fn sample_text(finding: &AuditFinding) -> String {
    finding.samples.iter().map(|sample| sample.value.clone()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::{audit_findings_to_xlsx, audit_job_to_json, audit_job_to_xlsx};
    use crate::audit::types::{
        AuditFinding, AuditJobState, AuditJobStatus, AuditKind, AuditLevel, AuditLevelFilter, AuditLogEntry,
        AuditMode, AuditSample, AuditScanRequest,
    };

    fn finding() -> AuditFinding {
        AuditFinding {
            connection_id: Some("conn-1".to_string()),
            connection_name: Some("local".to_string()),
            db_type: Some("postgres".to_string()),
            database: "app".to_string(),
            schema: Some("public".to_string()),
            table: "users".to_string(),
            column: "email".to_string(),
            data_type: Some("varchar".to_string()),
            kind: AuditKind::Email,
            level: AuditLevel::Medium,
            mode: AuditMode::FieldName,
            basis: "field-name".to_string(),
            count: 1,
            samples: vec![AuditSample { column: "email".to_string(), value: "a@example.com".to_string() }],
        }
    }

    fn job() -> AuditJobState {
        AuditJobState {
            job_id: "job".to_string(),
            status: AuditJobStatus::Completed,
            progress: 100,
            request: AuditScanRequest {
                connection_id: "local".to_string(),
                database: Some("app".to_string()),
                schema: Some("public".to_string()),
                tables: vec!["users".to_string()],
                mode: AuditMode::FieldName,
                level: AuditLevelFilter::All,
                limit: 15,
                mask: false,
                include_system: false,
                workers: 1,
                timeout_secs: 15,
            },
            logs: vec![AuditLogEntry { time: "12:00:00".to_string(), level: "info".to_string(), message: "done".to_string() }],
            findings: vec![finding()],
            errors: vec!["sample warning".to_string()],
            started_at: "2026-06-08T00:00:00Z".to_string(),
            finished_at: Some("2026-06-08T00:00:01Z".to_string()),
        }
    }

    #[test]
    fn serializes_job_to_json() {
        let json = audit_job_to_json(&job()).expect("json");
        assert!(json.contains("\"jobId\""));
        assert!(json.contains("\"findings\""));
    }

    #[test]
    fn builds_xlsx_for_findings() {
        let bytes = audit_findings_to_xlsx(&[finding()]).expect("xlsx");
        assert_eq!(bytes[0], 0x50);
        assert_eq!(bytes[1], 0x4b);
    }

    #[test]
    fn builds_multi_sheet_xlsx_for_job() {
        let bytes = audit_job_to_xlsx(&job()).expect("xlsx");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("敏感信息汇总"));
        assert!(text.contains("命中明细"));
        assert!(text.contains("运行日志"));
        assert!(text.contains("sample warning"));
    }
}
