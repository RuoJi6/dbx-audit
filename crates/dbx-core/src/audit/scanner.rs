use crate::types::ColumnInfo;

use super::detector::mask_sensitive_value;
use super::rules::AuditRuleEngine;
use super::types::{AuditFinding, AuditLevelFilter, AuditMode, AuditSample};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSqlDialect {
    Mysql,
    Postgres,
    Mssql,
    Oracle,
    Sqlite,
    ClickHouse,
    Ansi,
}

pub fn audit_column_findings(
    database: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    mode: AuditMode,
    level: AuditLevelFilter,
) -> Vec<AuditFinding> {
    audit_column_findings_with_engine(AuditRuleEngine::builtin(), database, schema, table, columns, mode, level)
}

pub fn audit_column_findings_with_engine(
    engine: &AuditRuleEngine,
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
            let text = field_scan_text(column);
            engine.scan_field(&text, level).into_iter().map(|matched| AuditFinding {
                connection_id: None,
                connection_name: None,
                db_type: None,
                connection_host: None,
                connection_port: None,
                connection_user: None,
                database: database.to_string(),
                schema: schema.map(str::to_string),
                table: table.to_string(),
                column: column.name.clone(),
                data_type: Some(column.data_type.clone()),
                kind: matched.kind,
                level: matched.level,
                mode: AuditMode::FieldName,
                basis: "field-name".to_string(),
                count: 0,
                samples: Vec::<AuditSample>::new(),
                rule_id: Some(matched.rule_id),
                rule_name: Some(matched.rule_name),
                rule_severity: Some(matched.rule_severity),
                rule_tags: matched.rule_tags,
                target_key: Some(audit_target_key(database, schema, table, &column.name)),
                confidence: Some("suspected".to_string()),
            })
        })
        .collect()
}

pub fn audit_content_findings_with_engine(
    engine: &AuditRuleEngine,
    database: &str,
    schema: Option<&str>,
    table: &str,
    column: &str,
    data_type: Option<&str>,
    value: &str,
    level: AuditLevelFilter,
    mask: bool,
) -> Vec<AuditFinding> {
    engine
        .scan_content(value, level)
        .into_iter()
        .map(|matched| AuditFinding {
            connection_id: None,
            connection_name: None,
            db_type: None,
            connection_host: None,
            connection_port: None,
            connection_user: None,
            database: database.to_string(),
            schema: schema.map(str::to_string),
            table: table.to_string(),
            column: column.to_string(),
            data_type: data_type.map(str::to_string),
            kind: matched.kind,
            level: matched.level,
            mode: AuditMode::Content,
            basis: "content".to_string(),
            count: 1,
            samples: vec![AuditSample {
                column: column.to_string(),
                value: if mask { mask_sensitive_value(&matched.value) } else { matched.value },
            }],
            rule_id: Some(matched.rule_id),
            rule_name: Some(matched.rule_name),
            rule_severity: Some(matched.rule_severity),
            rule_tags: matched.rule_tags,
            target_key: Some(audit_target_key(database, schema, table, column)),
            confidence: Some("confirmed".to_string()),
        })
        .collect()
}

fn field_scan_text(column: &ColumnInfo) -> String {
    let comment = column.comment.as_deref().unwrap_or_default();
    format!("column={} comment={} data_type={}", column.name, comment, column.data_type)
}

pub fn quote_ident(dialect: AuditSqlDialect, ident: &str) -> String {
    match dialect {
        AuditSqlDialect::Mysql | AuditSqlDialect::ClickHouse => format!("`{}`", ident.replace('`', "``")),
        AuditSqlDialect::Postgres | AuditSqlDialect::Oracle | AuditSqlDialect::Sqlite | AuditSqlDialect::Ansi => {
            format!("\"{}\"", ident.replace('"', "\"\""))
        }
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
        AuditSqlDialect::Oracle => format!("select {select} from {table_name} fetch first {limit} rows only"),
        AuditSqlDialect::Mysql
        | AuditSqlDialect::Postgres
        | AuditSqlDialect::Sqlite
        | AuditSqlDialect::ClickHouse
        | AuditSqlDialect::Ansi => {
            format!("select {select} from {table_name} limit {limit}")
        }
    }
}

pub fn build_paged_rows_sql(
    dialect: AuditSqlDialect,
    schema: Option<&str>,
    table: &str,
    columns: &[String],
    limit: usize,
    offset: usize,
) -> String {
    let select = if columns.is_empty() {
        "*".to_string()
    } else {
        columns.iter().map(|column| quote_ident(dialect, column)).collect::<Vec<_>>().join(", ")
    };
    let table_name = qualified_table_name(dialect, schema, table);
    match dialect {
        AuditSqlDialect::Mssql => {
            format!("select {select} from {table_name} order by (select null) offset {offset} rows fetch next {limit} rows only")
        }
        AuditSqlDialect::Oracle => {
            format!("select {select} from {table_name} offset {offset} rows fetch next {limit} rows only")
        }
        AuditSqlDialect::Mysql
        | AuditSqlDialect::Postgres
        | AuditSqlDialect::Sqlite
        | AuditSqlDialect::ClickHouse
        | AuditSqlDialect::Ansi => {
            format!("select {select} from {table_name} limit {limit} offset {offset}")
        }
    }
}

pub fn build_table_count_sql(dialect: AuditSqlDialect, schema: Option<&str>, table: &str) -> String {
    format!("select count(*) from {}", qualified_table_name(dialect, schema, table))
}

pub fn build_non_empty_count_sql(dialect: AuditSqlDialect, schema: Option<&str>, table: &str, column: &str) -> String {
    let table_name = qualified_table_name(dialect, schema, table);
    let column_name = quote_ident(dialect, column);
    match dialect {
        AuditSqlDialect::Mysql => {
            format!("select count(*) from {table_name} where {column_name} is not null and cast({column_name} as char) <> ''")
        }
        AuditSqlDialect::Postgres => {
            format!("select count(*) from {table_name} where {column_name} is not null and {column_name}::text <> ''")
        }
        AuditSqlDialect::Mssql => {
            format!("select count(*) from {table_name} where {column_name} is not null and cast({column_name} as nvarchar(max)) <> ''")
        }
        AuditSqlDialect::Oracle => {
            format!(
                "select count(*) from {table_name} where {column_name} is not null and to_char({column_name}) <> ''"
            )
        }
        AuditSqlDialect::Sqlite => {
            format!(
                "select count(*) from {table_name} where {column_name} is not null and cast({column_name} as text) <> ''"
            )
        }
        AuditSqlDialect::ClickHouse => {
            format!(
                "select count() from {table_name} where {column_name} is not null and notEmpty(toString({column_name})) settings max_threads = 1"
            )
        }
        AuditSqlDialect::Ansi => {
            format!(
                "select count(*) from {table_name} where {column_name} is not null and cast({column_name} as varchar) <> ''"
            )
        }
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
        AuditSqlDialect::Sqlite => {
            let like = regex_to_like(pattern);
            format!(
                "select {column_name} from {table_name} where cast({column_name} as text) like {like:?} limit {limit}"
            )
        }
        AuditSqlDialect::ClickHouse => {
            format!(
                "select {column_name} from {table_name} where match(toString({column_name}), {pattern:?}) limit {limit}"
            )
        }
        AuditSqlDialect::Ansi => {
            let like = regex_to_like(pattern);
            format!("select {column_name} from {table_name} where cast({column_name} as varchar) like {like:?} limit {limit}")
        }
    }
}

fn qualified_table_name(dialect: AuditSqlDialect, schema: Option<&str>, table: &str) -> String {
    match schema.filter(|schema| !schema.trim().is_empty()) {
        Some(schema) => format!("{}.{}", quote_ident(dialect, schema), quote_ident(dialect, table)),
        None => quote_ident(dialect, table),
    }
}

pub fn audit_target_key(database: &str, schema: Option<&str>, table: &str, column: &str) -> String {
    match schema.filter(|schema| !schema.trim().is_empty()) {
        Some(schema) => format!("{database}/{schema}/{table}/{column}"),
        None => format!("{database}/{table}/{column}"),
    }
}

pub fn is_textual_audit_type(data_type: &str) -> bool {
    let data_type = data_type.to_ascii_lowercase();
    let blocked = [
        "blob",
        "binary",
        "varbinary",
        "bytea",
        "image",
        "raw",
        "geometry",
        "geography",
        "point",
        "polygon",
        "linestring",
        "raster",
        "audio",
        "video",
    ];
    if blocked.iter().any(|item| data_type.contains(item)) {
        return false;
    }
    let allowed = [
        "char", "text", "string", "json", "xml", "clob", "uuid", "varchar", "nvarchar", "nchar", "enum", "set", "inet",
        "cidr",
    ];
    allowed.iter().any(|item| data_type.contains(item))
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
    use super::{
        audit_column_findings, build_content_match_sql, build_paged_rows_sql, build_sample_rows_sql,
        is_textual_audit_type, AuditSqlDialect,
    };
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
        assert_eq!(
            build_sample_rows_sql(AuditSqlDialect::Oracle, Some("HR"), "USERS", &["EMAIL".into()], 5),
            "select \"EMAIL\" from \"HR\".\"USERS\" fetch first 5 rows only"
        );
    }

    #[test]
    fn builds_count_sql_for_postgres_schema() {
        assert_eq!(
            super::build_table_count_sql(AuditSqlDialect::Postgres, Some("public"), "users"),
            "select count(*) from \"public\".\"users\""
        );
        assert!(super::build_non_empty_count_sql(AuditSqlDialect::Postgres, Some("public"), "users", "email")
            .contains("\"email\"::text <> ''"));
    }

    #[test]
    fn builds_count_sql_for_ansi_schema() {
        assert_eq!(
            super::build_table_count_sql(AuditSqlDialect::Ansi, Some("PUBLIC"), "USERS"),
            "select count(*) from \"PUBLIC\".\"USERS\""
        );
        assert!(super::build_non_empty_count_sql(AuditSqlDialect::Ansi, Some("PUBLIC"), "USERS", "EMAIL")
            .contains("cast(\"EMAIL\" as varchar) <> ''"));
    }

    #[test]
    fn builds_content_match_sql_for_core_databases() {
        assert!(build_content_match_sql(AuditSqlDialect::Postgres, Some("public"), "users", "email", "@", 3)
            .contains("::text ~"));
        assert!(
            build_content_match_sql(AuditSqlDialect::Oracle, None, "USERS", "EMAIL", "@", 3).contains("regexp_like")
        );
    }

    #[test]
    fn builds_paged_rows_sql_for_content_scan() {
        assert_eq!(
            build_paged_rows_sql(AuditSqlDialect::Mysql, Some("app"), "users", &["email".into()], 100, 200),
            "select `email` from `app`.`users` limit 100 offset 200"
        );
        assert!(build_paged_rows_sql(AuditSqlDialect::Mssql, Some("dbo"), "Users", &["email".into()], 100, 200)
            .contains("offset 200 rows fetch next 100 rows only"));
    }

    #[test]
    fn detects_textual_audit_types() {
        assert!(is_textual_audit_type("varchar(255)"));
        assert!(is_textual_audit_type("jsonb"));
        assert!(!is_textual_audit_type("bytea"));
    }
}
