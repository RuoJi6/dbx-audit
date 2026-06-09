use std::collections::BTreeMap;

use serde_json::Value;

use super::detector::{detect_field, detect_value, mask_sensitive_value};
use super::types::{
    AuditFinding, AuditKind, AuditLevel, AuditLevelFilter, AuditMode, AuditSample, AuditTableEvidence, AuditTableField,
};

#[derive(Debug, Clone)]
struct DocumentFieldValue {
    path: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FindingKey {
    path: String,
    kind: AuditKind,
    mode: AuditMode,
    basis: String,
}

#[allow(clippy::too_many_arguments)]
pub fn audit_document_findings(
    database: &str,
    schema: Option<&str>,
    collection: &str,
    documents: &[Value],
    mode: AuditMode,
    level: AuditLevelFilter,
    limit: usize,
    mask: bool,
) -> (Vec<AuditFinding>, Option<AuditTableEvidence>) {
    let mut rows = documents.iter().map(flatten_document_row).collect::<Vec<_>>();
    let mut columns = rows.iter().flat_map(|row| row.keys().cloned()).collect::<Vec<_>>();
    columns.sort();
    columns.dedup();

    let mut findings_by_key = BTreeMap::<FindingKey, AuditFinding>::new();
    for document in documents {
        let values = flatten_document_values(document);
        let mut counted = BTreeMap::<FindingKey, bool>::new();
        for field in values {
            if field.path.is_empty() || field.value.trim().is_empty() {
                continue;
            }
            if mode.includes_field_name() {
                for kind in detect_field(collection, &field.path, level) {
                    let key = FindingKey {
                        path: field.path.clone(),
                        kind,
                        mode: AuditMode::FieldName,
                        basis: "field-name".to_string(),
                    };
                    upsert_document_finding(
                        &mut findings_by_key,
                        &mut counted,
                        key,
                        database,
                        schema,
                        collection,
                        &field.value,
                        limit,
                        mask,
                    );
                }
            }
            if mode.includes_content() {
                for kind in detect_value(&field.value, level) {
                    let key = FindingKey {
                        path: field.path.clone(),
                        kind,
                        mode: AuditMode::Content,
                        basis: "content".to_string(),
                    };
                    upsert_document_finding(
                        &mut findings_by_key,
                        &mut counted,
                        key,
                        database,
                        schema,
                        collection,
                        &field.value,
                        limit,
                        mask,
                    );
                }
            }
        }
    }

    let findings = findings_by_key.into_values().collect::<Vec<_>>();
    if mask {
        let sensitive_paths = findings.iter().map(|finding| finding.column.clone()).collect::<Vec<_>>();
        for row in &mut rows {
            for path in &sensitive_paths {
                if let Some(value) = row.get_mut(path) {
                    *value = mask_sensitive_value(value);
                }
            }
        }
    }
    let evidence = (!findings.is_empty()).then(|| AuditTableEvidence {
        connection_id: None,
        connection_name: None,
        db_type: None,
        connection_host: None,
        connection_port: None,
        connection_user: None,
        database: database.to_string(),
        schema: schema.map(str::to_string),
        table: collection.to_string(),
        row_count: documents.len() as u64,
        columns,
        fields: table_fields(&findings),
        rows,
    });
    (findings, evidence)
}

#[allow(clippy::too_many_arguments)]
fn upsert_document_finding(
    findings: &mut BTreeMap<FindingKey, AuditFinding>,
    counted: &mut BTreeMap<FindingKey, bool>,
    key: FindingKey,
    database: &str,
    schema: Option<&str>,
    collection: &str,
    value: &str,
    limit: usize,
    mask: bool,
) {
    let finding = findings.entry(key.clone()).or_insert_with(|| AuditFinding {
        connection_id: None,
        connection_name: None,
        db_type: None,
        connection_host: None,
        connection_port: None,
        connection_user: None,
        database: database.to_string(),
        schema: schema.map(str::to_string),
        table: collection.to_string(),
        column: key.path.clone(),
        data_type: Some("document".to_string()),
        kind: key.kind,
        level: key.kind.level(),
        mode: key.mode,
        basis: key.basis.clone(),
        count: 0,
        samples: Vec::new(),
    });
    if let std::collections::btree_map::Entry::Vacant(entry) = counted.entry(key) {
        finding.count += 1;
        entry.insert(true);
    }
    if finding.samples.len() < limit.max(1) {
        finding.samples.push(AuditSample {
            column: finding.column.clone(),
            value: if mask { mask_sensitive_value(value) } else { value.to_string() },
        });
    }
}

fn flatten_document_row(value: &Value) -> BTreeMap<String, String> {
    flatten_document_values(value).into_iter().map(|field| (field.path, field.value)).collect()
}

fn flatten_document_values(value: &Value) -> Vec<DocumentFieldValue> {
    let mut values = Vec::new();
    flatten_document_value("", value, &mut values);
    values
}

fn flatten_document_value(path: &str, value: &Value, values: &mut Vec<DocumentFieldValue>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                flatten_document_value(&child_path, child, values);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let child_path = format!("{path}[{index}]");
                flatten_document_value(&child_path, child, values);
            }
        }
        Value::Null => {}
        Value::String(value) => values.push(DocumentFieldValue { path: path.to_string(), value: value.clone() }),
        Value::Bool(value) => values.push(DocumentFieldValue { path: path.to_string(), value: value.to_string() }),
        Value::Number(value) => values.push(DocumentFieldValue { path: path.to_string(), value: value.to_string() }),
    }
}

fn table_fields(findings: &[AuditFinding]) -> Vec<AuditTableField> {
    let mut fields = BTreeMap::<String, AuditTableField>::new();
    for finding in findings {
        let field = fields.entry(finding.column.clone()).or_insert_with(|| AuditTableField {
            name: finding.column.clone(),
            kinds: Vec::new(),
            level: AuditLevel::Low,
            mode: finding.mode,
            total: 0,
        });
        if !field.kinds.contains(&finding.kind) {
            field.kinds.push(finding.kind);
        }
        if finding.level > field.level {
            field.level = finding.level;
        }
        field.total = field.total.max(finding.count);
        if finding.mode == AuditMode::Content {
            field.mode = AuditMode::Content;
        }
    }
    fields.into_values().collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::audit_document_findings;
    use crate::audit::types::{AuditKind, AuditLevelFilter, AuditMode};

    #[test]
    fn audits_nested_document_fields_and_values() {
        let docs = vec![json!({
            "user": {
                "mobile_phone": "13800138000",
                "profile": { "email": "alice@example.com" }
            },
            "token": "api_key=abcdef1234567890"
        })];

        let (findings, evidence) = audit_document_findings(
            "shop",
            Some("document"),
            "users",
            &docs,
            AuditMode::FieldContent,
            AuditLevelFilter::All,
            5,
            true,
        );

        assert!(findings.iter().any(|finding| finding.column == "user.mobile_phone"));
        assert!(findings.iter().any(|finding| finding.kind == AuditKind::TokenSecret));
        let evidence = evidence.expect("table evidence");
        assert!(evidence.columns.iter().any(|column| column == "user.profile.email"));
        assert_eq!(evidence.row_count, 1);
    }
}
