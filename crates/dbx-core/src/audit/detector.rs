use super::rules::{AuditRuleEngine, AuditRuleMatch};
use super::types::{AuditKind, AuditLevelFilter};

pub fn detect_field(table: &str, column: &str, level: AuditLevelFilter) -> Vec<AuditKind> {
    detect_field_matches(table, column, level).into_iter().map(|item| item.kind).fold(Vec::new(), |mut kinds, kind| {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        kinds
    })
}

pub fn detect_field_matches(_table: &str, column: &str, level: AuditLevelFilter) -> Vec<AuditRuleMatch> {
    AuditRuleEngine::builtin().scan_field(&format!("column={column}"), level)
}

pub fn detect_value(value: &str, level: AuditLevelFilter) -> Vec<AuditKind> {
    detect_value_matches(value, level).into_iter().map(|item| item.kind).fold(Vec::new(), |mut kinds, kind| {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        kinds
    })
}

pub fn detect_value_matches(value: &str, level: AuditLevelFilter) -> Vec<AuditRuleMatch> {
    AuditRuleEngine::builtin().scan_content(value, level)
}

pub fn mask_sensitive_value(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 4 {
        return "*".repeat(chars.len());
    }
    let head: String = chars.iter().take(2).collect();
    let tail: String = chars.iter().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{}{}{}", head, "*".repeat(chars.len().saturating_sub(4).min(12)), tail)
}

#[cfg(test)]
mod tests {
    use super::{detect_field, detect_value, detect_value_matches, mask_sensitive_value};
    use crate::audit::types::{AuditKind, AuditLevelFilter};

    #[test]
    fn detects_sensitive_field_names() {
        let kinds = detect_field("users", "id_card_no", AuditLevelFilter::All);
        assert!(kinds.contains(&AuditKind::IdCard));

        let kinds = detect_field("users", "password_hash", AuditLevelFilter::High);
        assert!(kinds.contains(&AuditKind::PasswordSecret));

        let kinds = detect_field("users", "mobile", AuditLevelFilter::Medium);
        assert!(kinds.contains(&AuditKind::Phone));

        let kinds = detect_field("events", "customer_id", AuditLevelFilter::All);
        assert!(kinds.contains(&AuditKind::BusinessIdentifier));

        let kinds = detect_field("payments", "ip", AuditLevelFilter::All);
        assert!(kinds.contains(&AuditKind::IpAddress));

        let kinds = detect_field("security_findings", "evidence", AuditLevelFilter::All);
        assert!(kinds.contains(&AuditKind::RiskEvidence));

        let kinds = detect_field("app_secrets", "id", AuditLevelFilter::All);
        assert!(kinds.is_empty(), "plain id should not inherit secret from table name");

        let kinds = detect_field("audit_logs", "_id", AuditLevelFilter::All);
        assert!(kinds.is_empty(), "plain document id should not inherit risk from table name");

        let kinds = detect_field("app_secrets", "secret_key", AuditLevelFilter::All);
        assert!(kinds.contains(&AuditKind::TokenSecret));
    }

    #[test]
    fn filters_by_level() {
        let kinds = detect_field("users", "mobile", AuditLevelFilter::High);
        assert!(!kinds.contains(&AuditKind::Phone));
    }

    #[test]
    fn detects_sensitive_values() {
        assert!(detect_value("13800138000", AuditLevelFilter::All).contains(&AuditKind::Phone));
        assert!(detect_value("alice@example.com", AuditLevelFilter::All).contains(&AuditKind::Email));
        assert!(detect_value("11010519491231002X", AuditLevelFilter::All).contains(&AuditKind::IdCard));
        assert!(detect_value("api_key=abcdef1234567890", AuditLevelFilter::All).contains(&AuditKind::TokenSecret));
        assert!(detect_value("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature", AuditLevelFilter::All)
            .contains(&AuditKind::TokenSecret));
        assert!(detect_value("Authorization: Bearer abcdefghijklmnopqrstuvwxyz123456", AuditLevelFilter::All)
            .contains(&AuditKind::TokenSecret));
        assert!(
            detect_value(r#""password":"auditpass123""#, AuditLevelFilter::All).contains(&AuditKind::PasswordSecret)
        );
        assert!(
            detect_value(r#""secret_key":"abcdef1234567890""#, AuditLevelFilter::All).contains(&AuditKind::TokenSecret)
        );
        assert!(detect_value("10.211.55.16", AuditLevelFilter::All).contains(&AuditKind::IpAddress));
    }

    #[test]
    fn returns_rule_metadata() {
        let matches = detect_value_matches("BEGIN RSA PRIVATE KEY", AuditLevelFilter::All);
        assert!(matches.iter().any(|item| item.rule_id == "private-key" && item.kind == AuditKind::PrivateKey));
    }

    #[test]
    fn masks_values() {
        assert_eq!(mask_sensitive_value("13800138000"), "13*******00");
    }
}
