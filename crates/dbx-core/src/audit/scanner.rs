use crate::types::ColumnInfo;

use super::detector::detect_field;
use super::types::{AuditFinding, AuditLevelFilter, AuditMode, AuditSample};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSqlDialect {
    Mysql,
    Postgres,
    Mssql,
    Oracle,
}

pub fn audit_column_findings(
    database: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    mode: AuditMode,
    level: AuditLevelFilter,
) -> Vec<AuditFinding> {
    if !mode.includes_field_name() {
        return Vec::new();
    }

    columns
        .iter()
        .flat_map(|column| {
            detect_field(table, &column.name, level).into_iter().map(|kind| AuditFinding {
                database: database.to_string(),
                schema: schema.map(str::to_string),
                table: table.to_string(),
                column: column.name.clone(),
                data_type: Some(column.data_type.clone()),
                kind,
                level: kind.level(),
                mode: AuditMode::FieldName,
                basis: "field-name".to_string(),
                count: 0,
                samples: Vec::<AuditSample>::new(),
            })
        })
        .collect()
}

pub fn quote_ident(dialect: AuditSqlDialect, ident: &str) -> String {
    match dialect {
        AuditSqlDialect::Mysql => format!("`{}`", ident.replace('`', "``")),
        AuditSqlDialect::Postgres | AuditSqlDialect::Oracle => format!("\"{}\"", ident.replace('"', "\"\"")),
        AuditSqlDialect::Mssql => format!("[{}]", ident.replace(']', "]]")),
    }
}

pub fn build_sample_rows_sql(
    dialect: AuditSqlDialect,
    schema: Option<&str>,
    table: &str,
    columns: &[String],
    limit: usize,
) -> String {
    let select = if columns.is_empty() {
        "*".to_string()
    } else {
        columns.iter().map(|column| quote_ident(dialect, column)).collect::<Vec<_>>().join(", ")
    };
    let table_name = qualified_table_name(dialect, schema, table);
    match dialect {
        AuditSqlDialect::Mssql => format!("select top ({limit}) {select} from {table_name}"),
        _ => format!("select {select} from {table_name} limit {limit}"),
    }
}

pub fn build_content_match_sql(
    dialect: AuditSqlDialect,
    schema: Option<&str>,
    table: &str,
    column: &str,
    pattern: &str,
    limit: usize,
) -> String {
    let table_name = qualified_table_name(dialect, schema, table);
    let column_name = quote_ident(dialect, column);
    match dialect {
        AuditSqlDialect::Mysql => {
            format!("select {column_name} from {table_name} where {column_name} regexp {pattern:?} limit {limit}")
        }
        AuditSqlDialect::Postgres => {
            format!("select {column_name} from {table_name} where {column_name}::text ~ {pattern:?} limit {limit}")
        }
        AuditSqlDialect::Mssql => {
            let like = regex_to_like(pattern);
            format!("select top ({limit}) {column_name} from {table_name} where cast({column_name} as nvarchar(max)) like {like:?}")
        }
        AuditSqlDialect::Oracle => {
            format!("select {column_name} from {table_name} where regexp_like({column_name}, {pattern:?}) fetch first {limit} rows only")
        }
    }
}

fn qualified_table_name(dialect: AuditSqlDialect, schema: Option<&str>, table: &str) -> String {
    match schema.filter(|schema| !schema.trim().is_empty()) {
        Some(schema) => format!("{}.{}", quote_ident(dialect, schema), quote_ident(dialect, table)),
        None => quote_ident(dialect, table),
    }
}

fn regex_to_like(pattern: &str) -> String {
    if pattern.contains('@') {
        "%@%".to_string()
    } else {
        "%".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{audit_column_findings, build_content_match_sql, build_sample_rows_sql, AuditSqlDialect};
    use crate::audit::types::{AuditKind, AuditLevelFilter, AuditMode};
    use crate::types::ColumnInfo;

    fn column(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.to_string(),
            data_type: data_type.to_string(),
            is_nullable: true,
            column_default: None,
            is_primary_key: false,
            extra: None,
            comment: None,
            numeric_precision: None,
            numeric_scale: None,
            character_maximum_length: None,
        }
    }

    #[test]
    fn produces_field_name_findings_from_columns() {
        let findings = audit_column_findings(
            "app",
            Some("public"),
            "users",
            &[column("mobile", "varchar"), column("password_hash", "varchar")],
            AuditMode::FieldName,
            AuditLevelFilter::All,
        );
        assert!(findings.iter().any(|finding| finding.kind == AuditKind::Phone));
        assert!(findings.iter().any(|finding| finding.kind == AuditKind::PasswordSecret));
    }

    #[test]
    fn builds_database_specific_sample_sql() {
        assert_eq!(
            build_sample_rows_sql(AuditSqlDialect::Mysql, Some("app"), "users", &["id".into(), "email".into()], 5),
            "select `id`, `email` from `app`.`users` limit 5"
        );
        assert_eq!(
            build_sample_rows_sql(AuditSqlDialect::Mssql, Some("dbo"), "Users", &["email".into()], 5),
            "select top (5) [email] from [dbo].[Users]"
        );
    }

    #[test]
    fn builds_content_match_sql_for_core_databases() {
        assert!(build_content_match_sql(AuditSqlDialect::Postgres, Some("public"), "users", "email", "@", 3)
            .contains("::text ~"));
        assert!(build_content_match_sql(AuditSqlDialect::Oracle, None, "USERS", "EMAIL", "@", 3)
            .contains("regexp_like"));
    }
}
