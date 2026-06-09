use regex::Regex;

use super::types::{AuditKind, AuditLevelFilter};

const PHONE_PATTERN: &str = r"(?x)(?:\+?86[-\s]?)?1[3-9]\d{9}";
const EMAIL_PATTERN: &str = r"(?i)[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}";
const ID_CARD_PATTERN: &str = r"(?i)\b\d{17}[\dxX]\b";
const BANK_CARD_PATTERN: &str = r"\b(?:\d[ -]?){13,19}\b";
const TOKEN_PATTERN: &str = r"(?i)\b(?:ak|sk|token|secret|bearer|api[_-]?key)[a-z0-9_\-:=.]{8,}\b";
const IPV4_PATTERN: &str = r"\b(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b";
const HAE_JWT_PATTERN: &str = r"(?i)\beyJ[A-Za-z0-9_/+\-]{10,}\.[A-Za-z0-9._/+\-]{10,}(?:\.[A-Za-z0-9._/+\-]{8,})?\b";
const HAE_CLOUD_KEY_PATTERN: &str = r"(?i)\b(?:access[-_]?key[-_]?(?:id|secret)|LTAI[a-z0-9]{12,20})\b";
const HAE_AUTH_HEADER_PATTERN: &str = r"(?i)\b(?:basic|bearer)\s+[a-z0-9=:_+/.\-]{5,100}\b";
const HAE_PASSWORD_FIELD_PATTERN: &str = r#"(?i)(?:"?[\.\w]{0,32}(?:pass|pwd|passwd|password)[\.\w]{0,32}"?\s*(?::|={1,3}|!={1,2})\s*["'][^"']+["']|["'][^"']+["']\s*(?::|={1,3}|!={1,2})\s*"?[\.\w]{0,32}(?:pass|pwd|passwd|password)[\.\w]{0,32}"?)"#;
const HAE_SENSITIVE_FIELD_PATTERN: &str = r#"(?i)(?:"?[\.\w]{0,32}(?:key|secret|token|config|auth|access|admin|ticket)[\.\w]{0,32}"?\s*(?::|={1,3}|!={1,2})\s*["'][^"']+["']|["'][^"']+["']\s*(?::|={1,3}|!={1,2})\s*"?[\.\w]{0,32}(?:key|secret|token|config|auth|access|admin|ticket)[\.\w]{0,32}"?)"#;

pub fn detect_field(table: &str, column: &str, level: AuditLevelFilter) -> Vec<AuditKind> {
    let column_key = column.to_ascii_lowercase();
    let _table_key = table.to_ascii_lowercase();
    let mut kinds = Vec::new();
    push_if(
        &mut kinds,
        AuditKind::Phone,
        level,
        any_contains(&column_key, &["phone", "mobile", "tel", "手机号", "手机"]),
    );
    push_if(&mut kinds, AuditKind::Email, level, any_contains(&column_key, &["email", "mail", "邮箱"]));
    push_if(
        &mut kinds,
        AuditKind::IdCard,
        level,
        any_contains(&column_key, &["idcard", "id_card", "identity", "身份证", "证件"]),
    );
    push_if(&mut kinds, AuditKind::BankCard, level, any_contains(&column_key, &["bank", "card_no", "银行卡", "卡号"]));
    push_if(
        &mut kinds,
        AuditKind::PasswordSecret,
        level,
        any_contains(&column_key, &["password", "passwd", "pwd", "pass", "密码"]),
    );
    push_if(
        &mut kinds,
        AuditKind::TokenSecret,
        level,
        any_contains(&column_key, &["token", "secret", "apikey", "api_key", "access_key", "private_key", "密钥"]),
    );
    push_if(&mut kinds, AuditKind::Address, level, any_contains(&column_key, &["address", "addr", "地址"]));
    push_if(
        &mut kinds,
        AuditKind::Username,
        level,
        any_contains(
            &column_key,
            &["username", "user_name", "nickname", "realname", "actor", "operator", "姓名", "用户名"],
        ),
    );
    push_if(&mut kinds, AuditKind::Account, level, any_contains(&column_key, &["account", "acct", "账号", "账户"]));
    push_if(
        &mut kinds,
        AuditKind::IpAddress,
        level,
        matches_exactish(&column_key, &["ip", "client_ip", "remote_ip", "source_ip", "ip_addr", "ip_address"])
            || any_contains(&column_key, &["ip地址", "来源ip"]),
    );
    push_if(
        &mut kinds,
        AuditKind::BusinessIdentifier,
        level,
        matches_exactish(
            &column_key,
            &[
                "customer_id",
                "customerid",
                "cust_id",
                "entity_id",
                "entityid",
                "event_id",
                "eventid",
                "order_no",
                "orderno",
                "order_id",
                "orderid",
                "finding_id",
                "findingid",
                "asset_id",
                "assetid",
                "trace_id",
                "traceid",
                "request_id",
                "requestid",
                "session_id",
                "sessionid",
            ],
        ) || any_contains(&column_key, &["客户id", "订单号", "事件id", "实体id", "资产id"]),
    );
    push_if(
        &mut kinds,
        AuditKind::RiskEvidence,
        level,
        matches_exactish(
            &column_key,
            &[
                "evidence",
                "risk_score",
                "risk_decision",
                "severity",
                "detector",
                "message",
                "tags",
                "finding",
                "audit_log",
                "audit_logs",
            ],
        ) || any_contains(&column_key, &["风险", "证据", "审计", "日志"]),
    );
    kinds
}

pub fn detect_value(value: &str, level: AuditLevelFilter) -> Vec<AuditKind> {
    let mut kinds = Vec::new();
    push_if(&mut kinds, AuditKind::Phone, level, regex_match(PHONE_PATTERN, value));
    push_if(&mut kinds, AuditKind::Email, level, regex_match(EMAIL_PATTERN, value));
    push_if(&mut kinds, AuditKind::IdCard, level, regex_match(ID_CARD_PATTERN, value));
    push_if(&mut kinds, AuditKind::BankCard, level, regex_match(BANK_CARD_PATTERN, value) && luhn_like(value));
    push_if(&mut kinds, AuditKind::PasswordSecret, level, regex_match(HAE_PASSWORD_FIELD_PATTERN, value));
    push_if(
        &mut kinds,
        AuditKind::TokenSecret,
        level,
        regex_match(TOKEN_PATTERN, value)
            || regex_match(HAE_JWT_PATTERN, value)
            || regex_match(HAE_CLOUD_KEY_PATTERN, value)
            || regex_match(HAE_AUTH_HEADER_PATTERN, value)
            || regex_match(HAE_SENSITIVE_FIELD_PATTERN, value),
    );
    push_if(&mut kinds, AuditKind::IpAddress, level, regex_match(IPV4_PATTERN, value));
    kinds
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

fn push_if(kinds: &mut Vec<AuditKind>, kind: AuditKind, level: AuditLevelFilter, condition: bool) {
    if condition && level.allows(kind.level()) && !kinds.contains(&kind) {
        kinds.push(kind);
    }
}

fn any_contains(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn matches_exactish(haystack: &str, needles: &[&str]) -> bool {
    haystack.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).any(|part| {
        let part = part.trim_matches('_');
        needles.contains(&part)
    })
}

fn regex_match(pattern: &str, value: &str) -> bool {
    Regex::new(pattern).map(|re| re.is_match(value)).unwrap_or(false)
}

fn luhn_like(value: &str) -> bool {
    let digits: String = value.chars().filter(|ch| ch.is_ascii_digit()).collect();
    (13..=19).contains(&digits.len())
}

#[cfg(test)]
mod tests {
    use super::{detect_field, detect_value, mask_sensitive_value};
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
    fn masks_values() {
        assert_eq!(mask_sensitive_value("13800138000"), "13*******00");
    }
}
