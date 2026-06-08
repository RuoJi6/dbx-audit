use serde_json::{json, Value};

use super::types::{AuditFinding, AuditJobState};
use crate::xlsx_export::{build_xlsx_workbook, XlsxWorksheetData};

pub fn audit_job_to_json(job: &AuditJobState) -> Result<String, String> {
    serde_json::to_string_pretty(job).map_err(|err| err.to_string())
}

pub fn audit_findings_to_xlsx(findings: &[AuditFinding]) -> Result<Vec<u8>, String> {
    build_xlsx_workbook(&XlsxWorksheetData {
        sheet_name: Some("Audit Findings".to_string()),
        columns: vec![
            "Database".to_string(),
            "Schema".to_string(),
            "Table".to_string(),
            "Column/Key".to_string(),
            "Kind".to_string(),
            "Level".to_string(),
            "Mode".to_string(),
            "Basis".to_string(),
            "Count".to_string(),
            "Samples".to_string(),
        ],
        rows: findings.iter().map(finding_row).collect(),
    })
}

fn finding_row(finding: &AuditFinding) -> Vec<Value> {
    vec![
        json!(finding.database),
        json!(finding.schema.clone().unwrap_or_default()),
        json!(finding.table),
        json!(finding.column),
        json!(format!("{:?}", finding.kind)),
        json!(format!("{:?}", finding.level)),
        json!(format!("{:?}", finding.mode)),
        json!(finding.basis),
        json!(finding.count),
        json!(finding.samples.iter().map(|sample| sample.value.clone()).collect::<Vec<_>>().join("\n")),
    ]
}

#[cfg(test)]
mod tests {
    use super::{audit_findings_to_xlsx, audit_job_to_json};
    use crate::audit::types::{
        AuditFinding, AuditJobState, AuditJobStatus, AuditKind, AuditLevel, AuditLevelFilter, AuditMode,
        AuditScanRequest,
    };

    fn finding() -> AuditFinding {
        AuditFinding {
            database: "app".to_string(),
            schema: Some("public".to_string()),
            table: "users".to_string(),
            column: "email".to_string(),
            data_type: Some("varchar".to_string()),
            kind: AuditKind::Email,
            level: AuditLevel::Medium,
            mode: AuditMode::FieldName,
            basis: "field-name".to_string(),
            count: 0,
            samples: Vec::new(),
        }
    }

    #[test]
    fn serializes_job_to_json() {
        let job = AuditJobState {
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
                workers: 1,
                timeout_secs: 15,
            },
            logs: Vec::new(),
            findings: vec![finding()],
            errors: Vec::new(),
            started_at: "2026-06-08T00:00:00Z".to_string(),
            finished_at: Some("2026-06-08T00:00:01Z".to_string()),
        };
        let json = audit_job_to_json(&job).expect("json");
        assert!(json.contains("\"jobId\""));
        assert!(json.contains("\"findings\""));
    }

    #[test]
    fn builds_xlsx_for_findings() {
        let bytes = audit_findings_to_xlsx(&[finding()]).expect("xlsx");
        assert_eq!(bytes[0], 0x50);
        assert_eq!(bytes[1], 0x4b);
    }
}
