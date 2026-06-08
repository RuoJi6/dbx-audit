use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum AuditLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuditLevelFilter {
    All,
    Low,
    Medium,
    High,
}

impl AuditLevelFilter {
    pub fn allows(self, level: AuditLevel) -> bool {
        match self {
            AuditLevelFilter::All => true,
            AuditLevelFilter::Low => level == AuditLevel::Low,
            AuditLevelFilter::Medium => level == AuditLevel::Medium,
            AuditLevelFilter::High => level == AuditLevel::High,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuditMode {
    FieldContent,
    FieldName,
    Content,
    All,
}

impl AuditMode {
    pub fn includes_field_name(self) -> bool {
        matches!(self, AuditMode::FieldContent | AuditMode::FieldName | AuditMode::All)
    }

    pub fn includes_content(self) -> bool {
        matches!(self, AuditMode::FieldContent | AuditMode::Content | AuditMode::All)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuditKind {
    Phone,
    Email,
    IdCard,
    BankCard,
    PasswordSecret,
    TokenSecret,
    Address,
    Username,
    Account,
}

impl AuditKind {
    pub fn level(self) -> AuditLevel {
        match self {
            AuditKind::IdCard | AuditKind::BankCard | AuditKind::PasswordSecret | AuditKind::TokenSecret => {
                AuditLevel::High
            }
            AuditKind::Phone | AuditKind::Email => AuditLevel::Medium,
            AuditKind::Address | AuditKind::Username | AuditKind::Account => AuditLevel::Low,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditScanRequest {
    pub connection_id: String,
    #[serde(default)]
    pub connection: Option<serde_json::Value>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default = "default_audit_mode")]
    pub mode: AuditMode,
    #[serde(default = "default_level_filter")]
    pub level: AuditLevelFilter,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub mask: bool,
    #[serde(default)]
    pub include_system: bool,
    #[serde(default = "default_workers")]
    pub workers: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuditTaskKind {
    Single,
    Fscan,
    Sql,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditTarget {
    pub db_type: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditTaskRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub kind: AuditTaskKind,
    pub scan: AuditScanRequest,
    #[serde(default)]
    pub sql: Option<String>,
    #[serde(default)]
    pub targets: Vec<AuditTarget>,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default)]
    pub include_system: bool,
    #[serde(default)]
    pub split_output: bool,
    #[serde(default = "default_text_encoding")]
    pub text_encoding: String,
    #[serde(default)]
    pub output_path: Option<String>,
}

fn default_text_encoding() -> String {
    "auto".to_string()
}

fn default_audit_mode() -> AuditMode {
    AuditMode::FieldContent
}

fn default_level_filter() -> AuditLevelFilter {
    AuditLevelFilter::All
}

fn default_limit() -> usize {
    15
}

fn default_workers() -> usize {
    1
}

fn default_timeout_secs() -> u64 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditSample {
    pub column: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditFinding {
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub connection_name: Option<String>,
    #[serde(default)]
    pub db_type: Option<String>,
    pub database: String,
    #[serde(default)]
    pub schema: Option<String>,
    pub table: String,
    pub column: String,
    #[serde(default)]
    pub data_type: Option<String>,
    pub kind: AuditKind,
    pub level: AuditLevel,
    pub mode: AuditMode,
    pub basis: String,
    pub count: u64,
    #[serde(default)]
    pub samples: Vec<AuditSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditTableResult {
    pub database: String,
    #[serde(default)]
    pub schema: Option<String>,
    pub table: String,
    #[serde(default)]
    pub sensitive_fields: Vec<String>,
    pub row_count: u64,
    pub level: AuditLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditFieldResult {
    pub database: String,
    #[serde(default)]
    pub schema: Option<String>,
    pub table: String,
    pub column: String,
    pub kind: AuditKind,
    pub level: AuditLevel,
    pub hit_count: u64,
    #[serde(default)]
    pub sample_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditSampleGroup {
    pub database: String,
    #[serde(default)]
    pub schema: Option<String>,
    pub table: String,
    #[serde(default)]
    pub fields: Vec<AuditFieldResult>,
    #[serde(default)]
    pub rows: Vec<Vec<AuditSample>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditSqlResult {
    pub sql: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub rows: Vec<Vec<String>>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub findings: Vec<AuditFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogEntry {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuditJobStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditJobState {
    pub job_id: String,
    pub status: AuditJobStatus,
    pub progress: u8,
    pub request: AuditScanRequest,
    #[serde(default)]
    pub logs: Vec<AuditLogEntry>,
    #[serde(default)]
    pub findings: Vec<AuditFinding>,
    #[serde(default)]
    pub errors: Vec<String>,
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedFscanTarget {
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub line: usize,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedFscanTargets {
    pub targets: Vec<ParsedFscanTarget>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuditExportFormat {
    Json,
    Xlsx,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportResult {
    pub path: String,
    pub format: AuditExportFormat,
    pub finding_count: usize,
}
