use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::WebState;
use dbx_core::audit::{
    audit_column_findings, audit_document_findings, audit_job_to_json, audit_job_to_xlsx, build_non_empty_count_sql,
    build_sample_rows_sql, build_table_count_sql, detect_field, detect_value, mask_sensitive_value, parse_fscan_text,
    AuditExportFormat, AuditExportResult, AuditFinding, AuditJobState, AuditJobStatus, AuditLogEntry, AuditSample,
    AuditScanRequest, AuditTableEvidence, AuditTableField, ParsedFscanTargets,
};
use dbx_core::models::connection::{ConnectionConfig, DatabaseType};
use dbx_core::query::QueryExecutionOptions;
use dbx_core::types::ColumnInfo;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartScanRequest {
    pub request: AuditScanRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelScanRequest {
    pub job_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReportRequest {
    pub job_id: String,
    pub format: AuditExportFormat,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReportSnapshotRequest {
    pub job: AuditJobState,
    pub format: AuditExportFormat,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseFscanRequest {
    pub text_or_file: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTaskStoreRequest {
    pub store: serde_json::Value,
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn log_entry(level: &str, message: impl Into<String>) -> AuditLogEntry {
    AuditLogEntry { time: Utc::now().format("%H:%M:%S").to_string(), level: level.to_string(), message: message.into() }
}

async fn update_job(state: &WebState, job_id: &str, update: impl FnOnce(&mut AuditJobState)) {
    let mut jobs = state.audit_jobs.write().await;
    if let Some(job) = jobs.get_mut(job_id) {
        update(job);
    }
}

pub async fn start_scan(
    State(state): State<Arc<WebState>>,
    Json(body): Json<StartScanRequest>,
) -> Result<Json<String>, AppError> {
    let request = body.request;
    if request.connection_id.trim().is_empty() {
        return Err(AppError("请选择连接".to_string()));
    }

    let job_id = Uuid::new_v4().to_string();
    let job = AuditJobState {
        job_id: job_id.clone(),
        status: AuditJobStatus::Running,
        progress: 1,
        request: request.clone(),
        logs: vec![log_entry("info", "审计扫描已加入任务队列")],
        findings: Vec::new(),
        table_results: Vec::new(),
        errors: Vec::new(),
        started_at: now_rfc3339(),
        finished_at: None,
    };

    state.audit_jobs.write().await.insert(job_id.clone(), job);

    let state_clone = state.clone();
    let spawned_job_id = job_id.clone();
    tokio::spawn(async move {
        let result = run_field_name_scan(state_clone.clone(), request, &spawned_job_id).await;
        update_job(&state_clone, &spawned_job_id, |job| {
            match result {
                Ok(()) if job.status != AuditJobStatus::Cancelled => {
                    job.status = AuditJobStatus::Completed;
                    job.progress = 100;
                    job.logs.push(log_entry("info", "审计扫描已完成"));
                }
                Ok(()) => {}
                Err(err) => {
                    job.status = AuditJobStatus::Failed;
                    job.errors.push(err.clone());
                    job.logs.push(log_entry("error", err));
                }
            }
            job.finished_at = Some(now_rfc3339());
        })
        .await;
    });

    Ok(Json(job_id))
}

pub async fn cancel_scan(
    State(state): State<Arc<WebState>>,
    Json(body): Json<CancelScanRequest>,
) -> Result<Json<bool>, AppError> {
    let mut found = false;
    update_job(&state, &body.job_id, |job| {
        found = true;
        if job.status == AuditJobStatus::Running {
            job.status = AuditJobStatus::Cancelled;
            job.progress = 100;
            job.finished_at = Some(now_rfc3339());
            job.logs.push(log_entry("info", "审计扫描已取消"));
        }
    })
    .await;
    Ok(Json(found))
}

pub async fn get_job(
    State(state): State<Arc<WebState>>,
    Path(job_id): Path<String>,
) -> Result<Json<Option<AuditJobState>>, AppError> {
    Ok(Json(state.audit_jobs.read().await.get(&job_id).cloned()))
}

pub async fn export_report(
    State(state): State<Arc<WebState>>,
    Json(body): Json<ExportReportRequest>,
) -> Result<Json<AuditExportResult>, AppError> {
    let job = state
        .audit_jobs
        .read()
        .await
        .get(&body.job_id)
        .cloned()
        .ok_or_else(|| AppError(format!("未找到审计任务：{}", body.job_id)))?;

    let path_buf = PathBuf::from(body.path.trim());
    if path_buf.as_os_str().is_empty() {
        return Err(AppError("请填写导出路径".to_string()));
    }

    match body.format {
        AuditExportFormat::Json => {
            let json = audit_job_to_json(&job).map_err(AppError)?;
            std::fs::write(&path_buf, json).map_err(|err| AppError(err.to_string()))?;
        }
        AuditExportFormat::Xlsx => {
            let workbook = audit_job_to_xlsx(&job).map_err(AppError)?;
            std::fs::write(&path_buf, workbook).map_err(|err| AppError(err.to_string()))?;
        }
    }

    Ok(Json(AuditExportResult {
        path: path_buf.to_string_lossy().to_string(),
        format: body.format,
        finding_count: job.findings.len(),
    }))
}

pub async fn export_report_snapshot(
    Json(body): Json<ExportReportSnapshotRequest>,
) -> Result<Json<AuditExportResult>, AppError> {
    let path_buf = PathBuf::from(body.path.trim());
    if path_buf.as_os_str().is_empty() {
        return Err(AppError("请填写导出路径".to_string()));
    }
    match body.format {
        AuditExportFormat::Json => {
            let json = audit_job_to_json(&body.job).map_err(AppError)?;
            std::fs::write(&path_buf, json).map_err(|err| AppError(err.to_string()))?;
        }
        AuditExportFormat::Xlsx => {
            let workbook = audit_job_to_xlsx(&body.job).map_err(AppError)?;
            std::fs::write(&path_buf, workbook).map_err(|err| AppError(err.to_string()))?;
        }
    }
    Ok(Json(AuditExportResult {
        path: path_buf.to_string_lossy().to_string(),
        format: body.format,
        finding_count: body.job.findings.len(),
    }))
}

pub async fn parse_fscan(Json(body): Json<ParseFscanRequest>) -> Result<Json<ParsedFscanTargets>, AppError> {
    let input = if std::path::Path::new(&body.text_or_file).is_file() {
        std::fs::read_to_string(&body.text_or_file).map_err(|err| AppError(err.to_string()))?
    } else {
        body.text_or_file
    };
    Ok(Json(parse_fscan_text(&input)))
}

pub async fn load_task_store(State(state): State<Arc<WebState>>) -> Result<Json<Option<serde_json::Value>>, AppError> {
    Ok(Json(state.app.storage.load_audit_task_store().await.map_err(AppError)?))
}

pub async fn save_task_store(
    State(state): State<Arc<WebState>>,
    Json(body): Json<SaveTaskStoreRequest>,
) -> Result<Json<bool>, AppError> {
    state.app.storage.save_audit_task_store(&body.store).await.map_err(AppError)?;
    Ok(Json(true))
}

async fn run_field_name_scan(state: Arc<WebState>, request: AuditScanRequest, job_id: &str) -> Result<(), String> {
    prepare_audit_connection(&state, &request).await?;
    match connection_db_type(&state, &request.connection_id).await {
        Some(DatabaseType::Redis) => return run_redis_scan(state, request, job_id).await,
        Some(DatabaseType::MongoDb | DatabaseType::Elasticsearch) => {
            return run_document_scan(state, request, job_id).await;
        }
        _ => {}
    }
    let databases = audit_target_databases(&state, &request, job_id).await?;
    let total_databases = databases.len().max(1);

    for (database_index, database) in databases.iter().enumerate() {
        if is_cancelled(&state, job_id).await {
            return Ok(());
        }

        update_job(&state, job_id, |job| {
            job.progress = (5 + ((database_index * 10) / total_databases)).min(15) as u8;
            job.logs.push(log_entry("info", format!("正在读取数据库元数据：{database}")));
        })
        .await;

        let schemas = audit_target_schemas(&state, &request, database).await?;
        let total_schemas = schemas.len().max(1);

        for (schema_index, schema) in schemas.iter().enumerate() {
            let tables = if request.tables.is_empty() {
                dbx_core::schema::list_tables_core(&state.app, &request.connection_id, database, schema, None, None)
                    .await?
                    .into_iter()
                    .map(|table| table.name)
                    .collect::<Vec<_>>()
            } else {
                requested_tables_for_schema(&request.tables, schema)
            };

            let total_tables = tables.len().max(1);
            for (table_index, table) in tables.iter().enumerate() {
                if is_cancelled(&state, job_id).await {
                    return Ok(());
                }

                let schema_opt = schema_option(schema);
                let columns =
                    dbx_core::schema::get_columns_core(&state.app, &request.connection_id, database, schema, table)
                        .await?;
                let mut findings =
                    audit_column_findings(database, schema_opt, table, &columns, request.mode, request.level);
                apply_connection_meta(&state, &request.connection_id, &mut findings).await;

                for finding in findings.iter_mut() {
                    match count_non_empty_rows(&state, &request, database, schema_opt, table, &finding.column).await {
                        Ok(count) => finding.count = count,
                        Err(err) => {
                            update_job(&state, job_id, |job| {
                                job.logs.push(log_entry(
                                    "warn",
                                    format!(
                                        "字段存在行数统计失败：{}.{table}.{}，{err}",
                                        schema_label(schema),
                                        finding.column
                                    ),
                                ));
                            })
                            .await;
                        }
                    }
                }

                if request.mode.includes_content() && !findings.is_empty() {
                    match attach_sample_rows(&state, &request, database, schema_opt, table, &mut findings).await {
                        Ok(()) => {}
                        Err(err) => {
                            update_job(&state, job_id, |job| {
                                job.logs.push(log_entry("warn", format!("样例采集失败：{database}.{table}，{err}")));
                            })
                            .await;
                        }
                    }
                }

                let table_result =
                    collect_table_evidence(&state, &request, database, schema_opt, table, &columns, &findings).await;

                update_job(&state, job_id, |job| {
                    job.findings.extend(findings);
                    match table_result {
                        Ok(table_result) if !table_result.fields.is_empty() => job.table_results.push(table_result),
                        Ok(_) => {}
                        Err(err) => {
                            job.logs.push(log_entry("warn", format!("表样例记录失败：{database}.{table}，{err}")));
                        }
                    }
                    let database_progress = (database_index * 85) / total_databases;
                    let schema_progress = (schema_index * 85) / (total_databases * total_schemas);
                    let table_progress = ((table_index + 1) * 85) / (total_databases * total_schemas * total_tables);
                    job.progress = (10 + database_progress + schema_progress + table_progress).min(95) as u8;
                    let qualified = if schema.is_empty() { table.to_string() } else { format!("{schema}.{table}") };
                    job.logs.push(log_entry("info", format!("已扫描表：{database}.{qualified}")));
                })
                .await;
            }
        }
    }

    Ok(())
}

async fn prepare_audit_connection(state: &WebState, request: &AuditScanRequest) -> Result<(), String> {
    if let Some(snapshot) = &request.connection {
        let mut config: ConnectionConfig = serde_json::from_value(snapshot.clone()).map_err(|err| err.to_string())?;
        config.id = request.connection_id.clone();
        let config = config.canonicalized();
        state.app.configs.write().await.insert(request.connection_id.clone(), config);
    }

    if state.app.configs.read().await.get(&request.connection_id).is_none() {
        return Err("Connection config not found".to_string());
    }

    state.app.get_or_create_pool(&request.connection_id, None).await?;
    Ok(())
}

async fn audit_target_schemas(
    state: &WebState,
    request: &AuditScanRequest,
    database: &str,
) -> Result<Vec<String>, String> {
    if let Some(schema) = request.schema.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
        return Ok(vec![schema.to_string()]);
    }
    let db_type = connection_db_type(state, &request.connection_id).await;
    if !db_type.is_some_and(audit_should_enumerate_schemas) {
        return Ok(vec![String::new()]);
    }
    let schemas = dbx_core::schema::list_schemas_core(&state.app, &request.connection_id, database)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|schema| request.include_system || !is_system_schema(db_type, schema))
        .collect::<Vec<_>>();
    if schemas.is_empty() {
        Ok(default_audit_schemas(state, &request.connection_id, db_type).await)
    } else {
        Ok(schemas)
    }
}

fn requested_tables_for_schema(tables: &[String], schema: &str) -> Vec<String> {
    tables
        .iter()
        .filter_map(|table| {
            let trimmed = table.trim();
            if trimmed.is_empty() {
                return None;
            }
            let Some((table_schema, table_name)) = trimmed.split_once('.') else {
                return Some(trimmed.to_string());
            };
            if table_schema.trim_matches('"').eq_ignore_ascii_case(schema) {
                Some(table_name.trim_matches('"').to_string())
            } else {
                None
            }
        })
        .collect()
}

fn schema_option(schema: &str) -> Option<&str> {
    if schema.trim().is_empty() {
        None
    } else {
        Some(schema)
    }
}

fn schema_label(schema: &str) -> String {
    if schema.trim().is_empty() {
        "-".to_string()
    } else {
        schema.to_string()
    }
}

async fn attach_sample_rows(
    state: &WebState,
    request: &AuditScanRequest,
    database: &str,
    schema: Option<&str>,
    table: &str,
    findings: &mut [AuditFinding],
) -> Result<(), String> {
    let dialect = audit_sql_dialect(state, &request.connection_id).await.ok_or("当前数据库暂不支持样例采集")?;
    let columns =
        findings.iter().map(|finding| finding.column.clone()).fold(Vec::<String>::new(), |mut columns, column| {
            if !columns.contains(&column) {
                columns.push(column);
            }
            columns
        });
    if columns.is_empty() {
        return Ok(());
    }
    let sql = build_sample_rows_sql(dialect, schema, table, &columns, request.limit.max(1));
    let result = dbx_core::query::execute_sql_statement_with_options(
        &state.app,
        &request.connection_id,
        database,
        &sql,
        schema,
        None,
        QueryExecutionOptions {
            max_rows: Some(request.limit.max(1)),
            timeout_secs: Some(request.timeout_secs),
            ..Default::default()
        },
    )
    .await?;
    for finding in findings.iter_mut() {
        let Some(column_index) = result.columns.iter().position(|column| column.eq_ignore_ascii_case(&finding.column))
        else {
            continue;
        };
        finding.samples = result
            .rows
            .iter()
            .filter_map(|row| row.get(column_index))
            .filter_map(sample_value)
            .map(|value| AuditSample {
                column: finding.column.clone(),
                value: if request.mask { mask_sensitive_value(&value) } else { value },
            })
            .collect();
    }
    Ok(())
}

async fn audit_sql_dialect(state: &WebState, connection_id: &str) -> Option<dbx_core::audit::scanner::AuditSqlDialect> {
    let configs = state.app.configs.read().await;
    let db_type = &configs.get(connection_id)?.db_type;
    match db_type {
        DatabaseType::Mysql
        | DatabaseType::Doris
        | DatabaseType::StarRocks
        | DatabaseType::Databend
        | DatabaseType::Goldendb
        | DatabaseType::Gbase => Some(dbx_core::audit::scanner::AuditSqlDialect::Mysql),
        DatabaseType::Postgres
        | DatabaseType::OpenGauss
        | DatabaseType::Redshift
        | DatabaseType::Kingbase
        | DatabaseType::Highgo
        | DatabaseType::Vastbase
        | DatabaseType::Gaussdb
        | DatabaseType::Kwdb => Some(dbx_core::audit::scanner::AuditSqlDialect::Postgres),
        DatabaseType::SqlServer => Some(dbx_core::audit::scanner::AuditSqlDialect::Mssql),
        DatabaseType::Oracle | DatabaseType::Dameng | DatabaseType::OceanbaseOracle | DatabaseType::Yashandb => {
            Some(dbx_core::audit::scanner::AuditSqlDialect::Oracle)
        }
        DatabaseType::Sqlite | DatabaseType::Rqlite => Some(dbx_core::audit::scanner::AuditSqlDialect::Sqlite),
        DatabaseType::ClickHouse => Some(dbx_core::audit::scanner::AuditSqlDialect::ClickHouse),
        DatabaseType::DuckDb
        | DatabaseType::H2
        | DatabaseType::Snowflake
        | DatabaseType::Trino
        | DatabaseType::Hive
        | DatabaseType::Databricks
        | DatabaseType::SapHana
        | DatabaseType::Teradata
        | DatabaseType::Vertica
        | DatabaseType::Db2
        | DatabaseType::Informix
        | DatabaseType::Kylin
        | DatabaseType::Sundb
        | DatabaseType::Xugu
        | DatabaseType::Iris
        | DatabaseType::Jdbc => Some(dbx_core::audit::scanner::AuditSqlDialect::Ansi),
        _ => None,
    }
}

async fn count_non_empty_rows(
    state: &WebState,
    request: &AuditScanRequest,
    database: &str,
    schema: Option<&str>,
    table: &str,
    column: &str,
) -> Result<u64, String> {
    let dialect = audit_sql_dialect(state, &request.connection_id).await.ok_or("当前数据库暂不支持行数统计")?;
    let sql = build_non_empty_count_sql(dialect, schema, table, column);
    let result = dbx_core::query::execute_sql_statement_with_options(
        &state.app,
        &request.connection_id,
        database,
        &sql,
        schema,
        None,
        QueryExecutionOptions { max_rows: Some(1), timeout_secs: Some(request.timeout_secs), ..Default::default() },
    )
    .await?;
    Ok(result.rows.first().and_then(|row| row.first()).and_then(value_as_u64).unwrap_or(0))
}

async fn collect_table_evidence(
    state: &WebState,
    request: &AuditScanRequest,
    database: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    findings: &[AuditFinding],
) -> Result<AuditTableEvidence, String> {
    let row_count = count_table_rows(state, request, database, schema, table).await.unwrap_or(0);
    let column_names = columns.iter().map(|column| column.name.clone()).collect::<Vec<_>>();
    let rows = if request.mode.includes_content() && !findings.is_empty() {
        collect_sample_rows(state, request, database, schema, table, &column_names, findings).await?
    } else {
        Vec::new()
    };
    let mut evidence = AuditTableEvidence {
        connection_id: None,
        connection_name: None,
        db_type: None,
        connection_host: None,
        connection_port: None,
        connection_user: None,
        database: database.to_string(),
        schema: schema.map(str::to_string),
        table: table.to_string(),
        row_count,
        columns: column_names,
        fields: table_fields(findings),
        rows,
    };
    apply_table_connection_meta(state, &request.connection_id, &mut evidence).await;
    Ok(evidence)
}

async fn count_table_rows(
    state: &WebState,
    request: &AuditScanRequest,
    database: &str,
    schema: Option<&str>,
    table: &str,
) -> Result<u64, String> {
    let dialect = audit_sql_dialect(state, &request.connection_id).await.ok_or("当前数据库暂不支持行数统计")?;
    let sql = build_table_count_sql(dialect, schema, table);
    let result = dbx_core::query::execute_sql_statement_with_options(
        &state.app,
        &request.connection_id,
        database,
        &sql,
        schema,
        None,
        QueryExecutionOptions { max_rows: Some(1), timeout_secs: Some(request.timeout_secs), ..Default::default() },
    )
    .await?;
    Ok(result.rows.first().and_then(|row| row.first()).and_then(value_as_u64).unwrap_or(0))
}

async fn collect_sample_rows(
    state: &WebState,
    request: &AuditScanRequest,
    database: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[String],
    findings: &[AuditFinding],
) -> Result<Vec<BTreeMap<String, String>>, String> {
    let dialect = audit_sql_dialect(state, &request.connection_id).await.ok_or("当前数据库暂不支持样例采集")?;
    let sql = build_sample_rows_sql(dialect, schema, table, columns, request.limit.max(1));
    let result = dbx_core::query::execute_sql_statement_with_options(
        &state.app,
        &request.connection_id,
        database,
        &sql,
        schema,
        None,
        QueryExecutionOptions {
            max_rows: Some(request.limit.max(1)),
            timeout_secs: Some(request.timeout_secs),
            ..Default::default()
        },
    )
    .await?;
    let sensitive = findings.iter().fold(BTreeMap::<String, dbx_core::audit::AuditKind>::new(), |mut map, finding| {
        map.entry(finding.column.clone()).or_insert(finding.kind);
        map
    });
    Ok(result
        .rows
        .iter()
        .map(|row| {
            let mut values = BTreeMap::new();
            for (index, column) in result.columns.iter().enumerate() {
                let mut value = row.get(index).and_then(sample_value).unwrap_or_default();
                if request.mask && sensitive.contains_key(column) {
                    value = mask_sensitive_value(&value);
                }
                values.insert(column.clone(), value);
            }
            values
        })
        .collect())
}

fn sample_value(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) if value.trim().is_empty() => None,
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        other => Some(other.to_string()),
    }
}

fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(value) => value.as_u64().or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok())),
        Value::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

fn table_fields(findings: &[AuditFinding]) -> Vec<AuditTableField> {
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

async fn audit_target_databases(
    state: &WebState,
    request: &AuditScanRequest,
    job_id: &str,
) -> Result<Vec<String>, String> {
    if let Some(database) = request.database.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
        return Ok(vec![database.to_string()]);
    }

    update_job(state, job_id, |job| {
        job.progress = 3;
        job.logs.push(log_entry("info", "数据库为空，正在枚举可访问数据库"));
    })
    .await;

    let db_type = connection_db_type(state, &request.connection_id).await;
    let databases = dbx_core::schema::list_databases_core(&state.app, &request.connection_id)
        .await?
        .into_iter()
        .map(|database| database.name)
        .filter(|name| request.include_system || !is_system_database(db_type, name))
        .collect::<Vec<_>>();

    if databases.is_empty() {
        return Err("未发现可扫描数据库，请手工填写数据库名".to_string());
    }

    Ok(databases)
}

async fn run_document_scan(state: Arc<WebState>, request: AuditScanRequest, job_id: &str) -> Result<(), String> {
    update_job(&state, job_id, |job| {
        job.progress = 5;
        job.logs.push(log_entry("info", "正在扫描文档集合/索引"));
    })
    .await;

    let db_type = connection_db_type(&state, &request.connection_id).await;
    let databases =
        if let Some(database) = request.database.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
            vec![database.to_string()]
        } else {
            dbx_core::mongo_ops::mongo_list_databases_core(&state.app, &request.connection_id)
                .await?
                .into_iter()
                .filter(|database| request.include_system || !is_system_database(db_type, database))
                .collect::<Vec<_>>()
        };
    if databases.is_empty() {
        return Err("未发现可扫描数据库，请手工填写数据库名".to_string());
    }

    let total_databases = databases.len().max(1);
    let schema = document_schema(&state, &request.connection_id).await;
    for (database_index, database) in databases.iter().enumerate() {
        if is_cancelled(&state, job_id).await {
            return Ok(());
        }
        let collections = if request.tables.is_empty() {
            dbx_core::mongo_ops::mongo_list_collections_core(&state.app, &request.connection_id, database)
                .await?
                .into_iter()
                .filter(|collection| request.include_system || !is_system_document_collection(db_type, collection))
                .collect::<Vec<_>>()
        } else {
            request
                .tables
                .iter()
                .filter_map(|table| table.trim().rsplit('.').next().map(str::to_string))
                .filter(|table| !table.is_empty())
                .collect::<Vec<_>>()
        };
        let total_collections = collections.len().max(1);
        for (collection_index, collection) in collections.iter().enumerate() {
            if is_cancelled(&state, job_id).await {
                return Ok(());
            }
            let result = match dbx_core::mongo_ops::mongo_find_documents_core(
                &state.app,
                &request.connection_id,
                database,
                collection,
                0,
                request.limit.max(1) as i64,
                None,
                None,
            )
            .await
            {
                Ok(result) => result,
                Err(err) => {
                    update_job(&state, job_id, |job| {
                        job.logs.push(log_entry("warn", format!("文档集合扫描跳过：{database}.{collection}，{err}")));
                        job.errors.push(format!("{database}.{collection}: {err}"));
                    })
                    .await;
                    continue;
                }
            };
            let (mut findings, table_result) = audit_document_findings(
                database,
                Some(&schema),
                collection,
                &result.documents,
                request.mode,
                request.level,
                request.limit,
                request.mask,
            );
            apply_connection_meta(&state, &request.connection_id, &mut findings).await;
            let mut table_result = table_result;
            if let Some(table) = table_result.as_mut() {
                apply_table_connection_meta(&state, &request.connection_id, table).await;
                table.row_count = result.total;
            }
            update_job(&state, job_id, |job| {
                job.findings.extend(findings);
                if let Some(table_result) = table_result {
                    job.table_results.push(table_result);
                }
                let database_progress = (database_index * 90) / total_databases;
                let collection_progress = ((collection_index + 1) * 90) / (total_databases * total_collections);
                job.progress = (5 + database_progress + collection_progress).min(95) as u8;
                job.logs.push(log_entry("info", format!("已扫描文档目标：{database}.{collection}")));
            })
            .await;
        }
    }
    Ok(())
}

async fn run_redis_scan(state: Arc<WebState>, request: AuditScanRequest, job_id: &str) -> Result<(), String> {
    update_job(&state, job_id, |job| {
        job.progress = 5;
        job.logs.push(log_entry("info", "正在扫描 Redis keys"));
    })
    .await;
    let databases =
        if let Some(database) = request.database.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
            vec![database.parse::<u32>().map_err(|_| "Redis 数据库必须是数字".to_string())?]
        } else {
            let mut dbs = dbx_core::redis_ops::redis_list_databases_core(&state.app, &request.connection_id)
                .await?
                .into_iter()
                .filter(|db| db.keys > 0)
                .map(|db| db.db)
                .collect::<Vec<_>>();
            if dbs.is_empty() {
                dbs.push(0);
            }
            dbs
        };
    let total_dbs = databases.len().max(1);
    for (db_index, db) in databases.iter().enumerate() {
        if is_cancelled(&state, job_id).await {
            return Ok(());
        }
        let mut cursor = 0u64;
        loop {
            let page =
                dbx_core::redis_ops::redis_scan_keys_core(&state.app, &request.connection_id, *db, cursor, "*", 200)
                    .await?;
            let mut findings = Vec::new();
            for key in page.keys {
                if is_cancelled(&state, job_id).await {
                    return Ok(());
                }
                let value = dbx_core::redis_ops::redis_get_value_in_db_core(
                    &state.app,
                    &request.connection_id,
                    *db,
                    &key.key_raw,
                )
                .await
                .ok();
                findings.extend(redis_key_findings(&request, *db, &key.key_display, value.as_ref()));
            }
            apply_connection_meta(&state, &request.connection_id, &mut findings).await;
            update_job(&state, job_id, |job| {
                job.findings.extend(findings);
                job.progress = (10 + ((db_index + 1) * 85) / total_dbs).min(95) as u8;
                job.logs.push(log_entry("info", format!("已扫描 Redis DB {db}，cursor {}", page.cursor)));
            })
            .await;
            if page.cursor == 0 {
                break;
            }
            cursor = page.cursor;
        }
    }
    Ok(())
}

fn redis_key_findings(
    request: &AuditScanRequest,
    db: u32,
    key: &str,
    value: Option<&dbx_core::db::redis_driver::RedisValue>,
) -> Vec<AuditFinding> {
    let mut findings = Vec::new();
    if request.mode.includes_field_name() {
        for kind in detect_field(key, key, request.level) {
            findings.push(redis_finding(db, key, "key", kind, "field-name", key.to_string()));
        }
    }
    if request.mode.includes_content() {
        let value_text = value.map(|value| redis_value_text(&value.value)).unwrap_or_default();
        for kind in detect_value(&value_text, request.level) {
            let sample = if request.mask { mask_sensitive_value(&value_text) } else { value_text.clone() };
            findings.push(redis_finding(db, key, "value", kind, "content", sample));
        }
    }
    findings
}

fn redis_finding(
    db: u32,
    key: &str,
    column: &str,
    kind: dbx_core::audit::AuditKind,
    basis: &str,
    sample: String,
) -> AuditFinding {
    AuditFinding {
        connection_id: None,
        connection_name: None,
        db_type: Some("redis".to_string()),
        connection_host: None,
        connection_port: None,
        connection_user: None,
        database: format!("redis-db{db}"),
        schema: Some("redis-key".to_string()),
        table: key.to_string(),
        column: column.to_string(),
        data_type: Some("redis".to_string()),
        kind,
        level: kind.level(),
        mode: if basis == "content" {
            dbx_core::audit::AuditMode::Content
        } else {
            dbx_core::audit::AuditMode::FieldName
        },
        basis: basis.to_string(),
        count: 1,
        samples: vec![AuditSample { column: column.to_string(), value: sample }],
    }
}

fn redis_value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

async fn apply_connection_meta(state: &WebState, connection_id: &str, findings: &mut [AuditFinding]) {
    let configs = state.app.configs.read().await;
    let Some(config) = configs.get(connection_id) else {
        return;
    };
    for finding in findings {
        finding.connection_id = Some(connection_id.to_string());
        finding.connection_name = Some(config.name.clone());
        finding.db_type = Some(format!("{:?}", config.db_type).to_ascii_lowercase());
        finding.connection_host = Some(config.host.clone());
        finding.connection_port = Some(config.port);
        finding.connection_user = Some(config.username.clone());
    }
}

async fn apply_table_connection_meta(state: &WebState, connection_id: &str, table: &mut AuditTableEvidence) {
    let configs = state.app.configs.read().await;
    let Some(config) = configs.get(connection_id) else {
        return;
    };
    table.connection_id = Some(connection_id.to_string());
    table.connection_name = Some(config.name.clone());
    table.db_type = Some(format!("{:?}", config.db_type).to_ascii_lowercase());
    table.connection_host = Some(config.host.clone());
    table.connection_port = Some(config.port);
    table.connection_user = Some(config.username.clone());
}

async fn connection_db_type(state: &WebState, connection_id: &str) -> Option<DatabaseType> {
    state.app.configs.read().await.get(connection_id).map(|config| config.db_type)
}

async fn document_schema(state: &WebState, connection_id: &str) -> String {
    match connection_db_type(state, connection_id).await {
        Some(DatabaseType::Elasticsearch) => "index".to_string(),
        _ => "document".to_string(),
    }
}

fn audit_should_enumerate_schemas(db_type: DatabaseType) -> bool {
    !matches!(
        db_type,
        DatabaseType::Mysql
            | DatabaseType::Doris
            | DatabaseType::StarRocks
            | DatabaseType::Databend
            | DatabaseType::Goldendb
            | DatabaseType::Gbase
            | DatabaseType::ClickHouse
            | DatabaseType::Sqlite
            | DatabaseType::Rqlite
            | DatabaseType::Redis
            | DatabaseType::MongoDb
            | DatabaseType::Elasticsearch
            | DatabaseType::Neo4j
            | DatabaseType::Cassandra
            | DatabaseType::Bigquery
            | DatabaseType::Tdengine
            | DatabaseType::Iotdb
            | DatabaseType::Etcd
    )
}

async fn default_audit_schemas(state: &WebState, connection_id: &str, db_type: Option<DatabaseType>) -> Vec<String> {
    let username = state
        .app
        .configs
        .read()
        .await
        .get(connection_id)
        .map(|config| config.username.trim().to_string())
        .filter(|username| !username.is_empty());
    match db_type {
        Some(
            DatabaseType::Postgres
            | DatabaseType::OpenGauss
            | DatabaseType::Redshift
            | DatabaseType::Kingbase
            | DatabaseType::Highgo
            | DatabaseType::Vastbase
            | DatabaseType::Gaussdb
            | DatabaseType::Kwdb,
        ) => vec!["public".to_string()],
        Some(DatabaseType::SqlServer) => vec!["dbo".to_string()],
        Some(DatabaseType::DuckDb) => vec!["main".to_string()],
        Some(DatabaseType::Trino | DatabaseType::Hive | DatabaseType::Databricks) => vec!["default".to_string()],
        Some(DatabaseType::Snowflake) => vec!["PUBLIC".to_string()],
        Some(DatabaseType::Oracle | DatabaseType::Dameng | DatabaseType::OceanbaseOracle | DatabaseType::Yashandb) => {
            username.map(|value| vec![value.to_ascii_uppercase()]).unwrap_or_else(|| vec![String::new()])
        }
        _ => vec![String::new()],
    }
}

fn is_system_database(db_type: Option<DatabaseType>, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let upper = name.to_ascii_uppercase();
    if matches!(
        lower.as_str(),
        "information_schema" | "mysql" | "performance_schema" | "sys" | "system" | "template0" | "template1" | "postgres"
    ) {
        return true;
    }
    match db_type {
        Some(DatabaseType::MongoDb) => matches!(lower.as_str(), "admin" | "local" | "config"),
        Some(DatabaseType::SqlServer) => matches!(lower.as_str(), "master" | "model" | "msdb" | "tempdb"),
        Some(DatabaseType::ClickHouse) => lower == "system",
        Some(DatabaseType::Snowflake) => upper == "SNOWFLAKE",
        Some(DatabaseType::Trino | DatabaseType::Hive | DatabaseType::Databricks) => {
            matches!(lower.as_str(), "system" | "information_schema")
        }
        Some(DatabaseType::Cassandra) => lower == "system" || lower.starts_with("system_"),
        Some(DatabaseType::Neo4j) => lower == "system",
        Some(DatabaseType::Informix) => matches!(lower.as_str(), "sysmaster" | "sysadmin" | "sysuser" | "sysutils"),
        _ => false,
    }
}

fn is_system_document_collection(db_type: Option<DatabaseType>, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match db_type {
        Some(DatabaseType::MongoDb) => lower.starts_with("system."),
        Some(DatabaseType::Elasticsearch) => lower.starts_with('.'),
        _ => false,
    }
}

fn is_system_schema(db_type: Option<DatabaseType>, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let upper = name.to_ascii_uppercase();
    if matches!(lower.as_str(), "information_schema" | "pg_catalog" | "sys" | "system" | "mysql" | "performance_schema")
        || lower.starts_with("pg_toast")
        || lower.starts_with("pg_temp_")
        || lower.starts_with("pg_toast_temp_")
    {
        return true;
    }
    match db_type {
        Some(DatabaseType::SqlServer) => matches!(
            lower.as_str(),
            "guest"
                | "db_owner"
                | "db_accessadmin"
                | "db_securityadmin"
                | "db_ddladmin"
                | "db_backupoperator"
                | "db_datareader"
                | "db_datawriter"
                | "db_denydatareader"
                | "db_denydatawriter"
        ),
        Some(DatabaseType::Oracle | DatabaseType::Dameng | DatabaseType::OceanbaseOracle | DatabaseType::Yashandb) => {
            upper.starts_with("APEX_")
                || matches!(
                upper.as_str(),
                "SYS"
                    | "SYSTEM"
                    | "ANONYMOUS"
                    | "OUTLN"
                    | "DIP"
                    | "DMSYS"
                    | "DBSNMP"
                    | "EXFSYS"
                    | "FLOWS_FILES"
                    | "XDB"
                    | "MDDATA"
                    | "MDSYS"
                    | "ORDSYS"
                    | "ORDDATA"
                    | "ORDPLUGINS"
                    | "CTXSYS"
                    | "WMSYS"
                    | "APPQOSSYS"
                    | "AUDSYS"
                    | "DVSYS"
                    | "GSMADMIN_INTERNAL"
                    | "LBACSYS"
                    | "OJVMSYS"
                    | "OLAPSYS"
                    | "REMOTE_SCHEDULER_AGENT"
                    | "SI_INFORMTN_SCHEMA"
                    | "SPATIAL_CSW_ADMIN_USR"
                    | "SYSBACKUP"
                    | "SYSDG"
                    | "SYSKM"
                    | "SYSRAC"
                )
        }
        Some(DatabaseType::Db2) => upper.starts_with("SYS") || matches!(upper.as_str(), "SQLJ" | "NULLID"),
        Some(DatabaseType::Snowflake) => matches!(
            upper.as_str(),
            "INFORMATION_SCHEMA"
                | "ACCOUNT_USAGE"
                | "READER_ACCOUNT_USAGE"
                | "ORGANIZATION_USAGE"
                | "DATA_SHARING_USAGE"
                | "CORE"
                | "TELEMETRY"
        ),
        Some(DatabaseType::SapHana) => upper == "SYS" || upper.starts_with("_SYS_"),
        Some(DatabaseType::Vertica) => matches!(upper.as_str(), "V_CATALOG" | "V_MONITOR" | "V_INTERNAL"),
        Some(DatabaseType::Exasol) => {
            upper == "SYS" || matches!(upper.as_str(), "EXA_STATISTICS" | "EXA_TOOLBOX" | "EXA_DB_SIZE")
        }
        Some(DatabaseType::Teradata) => {
            matches!(upper.as_str(), "DBC" | "SYS_CALENDAR" | "SYSUDTLIB") || upper.starts_with("SYS")
        }
        Some(DatabaseType::Informix) => matches!(upper.as_str(), "INFORMIX" | "SYSADMIN" | "SYSMASTER"),
        _ => false,
    }
}

async fn is_cancelled(state: &WebState, job_id: &str) -> bool {
    state.audit_jobs.read().await.get(job_id).map(|job| job.status == AuditJobStatus::Cancelled).unwrap_or(false)
}
