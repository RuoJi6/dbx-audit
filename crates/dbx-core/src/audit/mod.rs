pub mod detector;
pub mod document;
pub mod fscan;
pub mod report;
pub mod rules;
pub mod scanner;
pub mod types;

pub use detector::{detect_field, detect_field_matches, detect_value, detect_value_matches, mask_sensitive_value};
pub use document::{audit_document_findings, audit_document_findings_with_engine};
pub use fscan::parse_fscan_text;
pub use report::{audit_findings_to_xlsx, audit_job_to_json, audit_job_to_xlsx};
pub use scanner::{
    audit_column_findings, audit_column_findings_with_engine, audit_content_findings_with_engine,
    build_content_match_sql, build_non_empty_count_sql, build_paged_rows_sql, build_sample_rows_sql,
    build_table_count_sql, is_textual_audit_type, quote_ident,
};
pub use types::*;
