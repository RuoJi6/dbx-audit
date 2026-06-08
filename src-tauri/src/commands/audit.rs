use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::commands::connection::AppState;
use dbx_core::audit::scanner::AuditSqlDialect;
use dbx_core::audit::{
    audit_column_findings, audit_job_to_json, audit_job_to_xlsx, build_sample_rows_sql, detect_field, detect_value,
    mask_sensitive_value, parse_fscan_text, AuditExportFormat, AuditExportResult, AuditFinding, AuditJobState,
    AuditJobStatus, AuditLogEntry, AuditSample, AuditScanRequest, ParsedFscanTargets,
};
use dbx_core::models::connection::{ConnectionConfig, DatabaseType};
use dbx_core::query::QueryExecutionOptions;
use serde_json::Value;

static AUDIT_JOBS: OnceLock<Mutex<HashMap<String, AuditJobState>>> = OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<String, AuditJobState>> {
    AUDIT_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn log_entry(level: &str, message: impl Into<String>) -> AuditLogEntry {
    AuditLogEntry { time: Utc::now().format("%H:%M:%S").to_string(), level: level.to_string(), message: message.into() }
}

fn update_job(job_id: &str, update: impl FnOnce(&mut AuditJobState)) {
    if let Ok(mut jobs) = jobs().lock() {
        if let Some(job) = jobs.get_mut(job_id) {
            update(job);
        }
    }
}

#[tauri::command]
pub async fn audit_start_scan(state: State<'_, Arc<AppState>>, request: AuditScanRequest) -> Result<String, String> {
    if request.connection_id.trim().is_empty() {
        return Err("请选择连接".to_string());
    }

    let job_id = Uuid::new_v4().to_string();
    let job = AuditJobState {
        job_id: job_id.clone(),
        status: AuditJobStatus::Running,
        progress: 1,
        request: request.clone(),
        logs: vec![log_entry("info", "审计扫描已加入任务队列")],
        findings: Vec::new(),
        errors: Vec::new(),
        started_at: now_rfc3339(),
        finished_at: None,
    };

    jobs().lock().map_err(|err| err.to_string())?.insert(job_id.clone(), job);

    let state = state.inner().clone();
    let spawned_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_field_name_scan(state, request, &spawned_job_id).await;
        update_job(&spawned_job_id, |job| {
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
        });
    });

    Ok(job_id)
}

#[tauri::command]
pub async fn audit_cancel_scan(job_id: String) -> Result<bool, String> {
    let mut found = false;
    update_job(&job_id, |job| {
        found = true;
        if job.status == AuditJobStatus::Running {
            job.status = AuditJobStatus::Cancelled;
            job.progress = 100;
            job.finished_at = Some(now_rfc3339());
            job.logs.push(log_entry("info", "审计扫描已取消"));
        }
    });
    Ok(found)
}

#[tauri::command]
pub async fn audit_get_job(job_id: String) -> Result<Option<AuditJobState>, String> {
    Ok(jobs().lock().map_err(|err| err.to_string())?.get(&job_id).cloned())
}

#[tauri::command]
pub async fn audit_export_report(
    job_id: String,
    format: AuditExportFormat,
    path: String,
) -> Result<AuditExportResult, String> {
    let job = jobs()
        .lock()
        .map_err(|err| err.to_string())?
        .get(&job_id)
        .cloned()
        .ok_or_else(|| format!("未找到审计任务：{job_id}"))?;

    let path_buf = PathBuf::from(path.trim());
    if path_buf.as_os_str().is_empty() {
        return Err("请填写导出路径".to_string());
    }

    match format {
        AuditExportFormat::Json => {
            let json = audit_job_to_json(&job)?;
            std::fs::write(&path_buf, json).map_err(|err| err.to_string())?;
        }
        AuditExportFormat::Xlsx => {
            let workbook = audit_job_to_xlsx(&job)?;
            std::fs::write(&path_buf, workbook).map_err(|err| err.to_string())?;
        }
    }

    Ok(AuditExportResult { path: path_buf.to_string_lossy().to_string(), format, finding_count: job.findings.len() })
}

#[tauri::command]
pub async fn audit_export_report_snapshot(
    job: AuditJobState,
    format: AuditExportFormat,
    path: String,
) -> Result<AuditExportResult, String> {
    let path_buf = PathBuf::from(path.trim());
    if path_buf.as_os_str().is_empty() {
        return Err("请填写导出路径".to_string());
    }
    match format {
        AuditExportFormat::Json => {
            let json = audit_job_to_json(&job)?;
            std::fs::write(&path_buf, json).map_err(|err| err.to_string())?;
        }
        AuditExportFormat::Xlsx => {
            let workbook = audit_job_to_xlsx(&job)?;
            std::fs::write(&path_buf, workbook).map_err(|err| err.to_string())?;
        }
    }
    Ok(AuditExportResult { path: path_buf.to_string_lossy().to_string(), format, finding_count: job.findings.len() })
}

#[tauri::command]
pub async fn audit_open_output_directory(path: String) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("请填写输出路径".to_string());
    }

    let directory = PathBuf::from(path);
    if !directory.exists() {
        return Err(format!("目录不存在：{}", directory.to_string_lossy()));
    }
    if !directory.is_dir() {
        return Err(format!("不是有效目录：{}", directory.to_string_lossy()));
    }

    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg(&directory).status()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(&directory).status()
    } else {
        Command::new("xdg-open").arg(&directory).status()
    }
    .map_err(|err| err.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("打开目录失败：{}", directory.to_string_lossy()))
    }
}

#[tauri::command]
pub async fn audit_parse_fscan(text_or_file: String) -> Result<ParsedFscanTargets, String> {
    let input = if std::path::Path::new(&text_or_file).is_file() {
        std::fs::read_to_string(&text_or_file).map_err(|err| err.to_string())?
    } else {
        text_or_file
    };
    Ok(parse_fscan_text(&input))
}

#[tauri::command]
pub async fn audit_load_task_store(state: State<'_, Arc<AppState>>) -> Result<Option<serde_json::Value>, String> {
    state.storage.load_audit_task_store().await
}

#[tauri::command]
pub async fn audit_save_task_store(state: State<'_, Arc<AppState>>, store: serde_json::Value) -> Result<(), String> {
    state.storage.save_audit_task_store(&store).await
}

async fn run_field_name_scan(state: Arc<AppState>, request: AuditScanRequest, job_id: &str) -> Result<(), String> {
    prepare_audit_connection(&state, &request).await?;
    if connection_db_type(&state, &request.connection_id).await == Some(DatabaseType::Redis) {
        return run_redis_scan(state, request, job_id).await;
    }
    let schema = request.schema.clone().unwrap_or_default();
    let databases = audit_target_databases(&state, &request, job_id).await?;
    let total_databases = databases.len().max(1);

    for (database_index, database) in databases.iter().enumerate() {
        if is_cancelled(job_id) {
            return Ok(());
        }

        update_job(job_id, |job| {
            job.progress = (5 + ((database_index * 10) / total_databases)).min(15) as u8;
            job.logs.push(log_entry("info", format!("正在读取数据库元数据：{database}")));
        });

        let tables = if request.tables.is_empty() {
            dbx_core::schema::list_tables_core(&state, &request.connection_id, database, &schema, None, None)
                .await?
                .into_iter()
                .map(|table| table.name)
                .collect::<Vec<_>>()
        } else {
            request.tables.clone()
        };

        let total_tables = tables.len().max(1);
        for (table_index, table) in tables.iter().enumerate() {
            if is_cancelled(job_id) {
                return Ok(());
            }

            let columns =
                dbx_core::schema::get_columns_core(&state, &request.connection_id, database, &schema, table).await?;
            let mut findings = audit_column_findings(
                database,
                if schema.is_empty() { None } else { Some(schema.as_str()) },
                table,
                &columns,
                request.mode,
                request.level,
            );
            apply_connection_meta(&state, &request.connection_id, &mut findings).await;

            if request.mode.includes_content() && !findings.is_empty() {
                match attach_sample_rows(
                    &state,
                    &request,
                    database,
                    if schema.is_empty() { None } else { Some(schema.as_str()) },
                    table,
                    &mut findings,
                )
                .await
                {
                    Ok(()) => {}
                    Err(err) => update_job(job_id, |job| {
                        job.logs.push(log_entry("warn", format!("样例采集失败：{database}.{table}，{err}")));
                    }),
                }
            }

            update_job(job_id, |job| {
                job.findings.extend(findings);
                let database_progress = (database_index * 85) / total_databases;
                let table_progress = ((table_index + 1) * 85) / (total_databases * total_tables);
                job.progress = (10 + database_progress + table_progress).min(95) as u8;
                job.logs.push(log_entry("info", format!("已扫描表：{database}.{table}")));
            });
        }
    }

    Ok(())
}

async fn prepare_audit_connection(state: &AppState, request: &AuditScanRequest) -> Result<(), String> {
    if let Some(snapshot) = &request.connection {
        let mut config: ConnectionConfig = serde_json::from_value(snapshot.clone()).map_err(|err| err.to_string())?;
        config.id = request.connection_id.clone();
        let config = config.canonicalized();
        state.configs.write().await.insert(request.connection_id.clone(), config);
    }

    if state.configs.read().await.get(&request.connection_id).is_none() {
        return Err("Connection config not found".to_string());
    }

    state.get_or_create_pool(&request.connection_id, None).await?;
    Ok(())
}

async fn attach_sample_rows(
    state: &AppState,
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
        state,
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
        finding.count = finding.samples.len() as u64;
    }

    Ok(())
}

async fn audit_sql_dialect(state: &AppState, connection_id: &str) -> Option<AuditSqlDialect> {
    let configs = state.configs.read().await;
    let db_type = &configs.get(connection_id)?.db_type;
    match db_type {
        DatabaseType::Mysql | DatabaseType::Doris | DatabaseType::StarRocks | DatabaseType::Goldendb => {
            Some(AuditSqlDialect::Mysql)
        }
        DatabaseType::Postgres
        | DatabaseType::OpenGauss
        | DatabaseType::Redshift
        | DatabaseType::Kingbase
        | DatabaseType::Highgo
        | DatabaseType::Vastbase
        | DatabaseType::Gaussdb
        | DatabaseType::Kwdb => Some(AuditSqlDialect::Postgres),
        DatabaseType::SqlServer => Some(AuditSqlDialect::Mssql),
        DatabaseType::Oracle | DatabaseType::Dameng | DatabaseType::OceanbaseOracle | DatabaseType::Yashandb => {
            Some(AuditSqlDialect::Oracle)
        }
        _ => None,
    }
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

async fn audit_target_databases(
    state: &AppState,
    request: &AuditScanRequest,
    job_id: &str,
) -> Result<Vec<String>, String> {
    if let Some(database) = request.database.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
        return Ok(vec![database.to_string()]);
    }

    update_job(job_id, |job| {
        job.progress = 3;
        job.logs.push(log_entry("info", "数据库为空，正在枚举可访问数据库"));
    });

    let databases = dbx_core::schema::list_databases_core(state, &request.connection_id)
        .await?
        .into_iter()
        .map(|database| database.name)
        .filter(|name| request.include_system || !is_system_database(name))
        .collect::<Vec<_>>();

    if databases.is_empty() {
        return Err("未发现可扫描数据库，请手工填写数据库名".to_string());
    }

    Ok(databases)
}

async fn run_redis_scan(state: Arc<AppState>, request: AuditScanRequest, job_id: &str) -> Result<(), String> {
    update_job(job_id, |job| {
        job.progress = 5;
        job.logs.push(log_entry("info", "正在扫描 Redis keys"));
    });
    let databases = if let Some(database) = request.database.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
        vec![database.parse::<u32>().map_err(|_| "Redis 数据库必须是数字".to_string())?]
    } else {
        let mut dbs = dbx_core::redis_ops::redis_list_databases_core(&state, &request.connection_id)
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
        if is_cancelled(job_id) {
            return Ok(());
        }
        let mut cursor = 0u64;
        loop {
            let page = dbx_core::redis_ops::redis_scan_keys_core(&state, &request.connection_id, *db, cursor, "*", 200).await?;
            let mut findings = Vec::new();
            for key in page.keys {
                if is_cancelled(job_id) {
                    return Ok(());
                }
                let value = dbx_core::redis_ops::redis_get_value_in_db_core(&state, &request.connection_id, *db, &key.key_raw)
                    .await
                    .ok();
                findings.extend(redis_key_findings(&request, *db, &key.key_display, value.as_ref()));
            }
            apply_connection_meta(&state, &request.connection_id, &mut findings).await;
            update_job(job_id, |job| {
                job.findings.extend(findings);
                job.progress = (10 + ((db_index + 1) * 85) / total_dbs).min(95) as u8;
                job.logs.push(log_entry("info", format!("已扫描 Redis DB {db}，cursor {}", page.cursor)));
            });
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
            findings.push(redis_finding(request, db, key, "key", kind, "field-name", key.to_string()));
        }
    }
    if request.mode.includes_content() {
        let value_text = value.map(|value| redis_value_text(&value.value)).unwrap_or_default();
        for kind in detect_value(&value_text, request.level) {
            let sample = if request.mask { mask_sensitive_value(&value_text) } else { value_text.clone() };
            findings.push(redis_finding(request, db, key, "value", kind, "content", sample));
        }
    }
    findings
}

fn redis_finding(
    _request: &AuditScanRequest,
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
        database: format!("redis-db{db}"),
        schema: Some("redis-key".to_string()),
        table: key.to_string(),
        column: column.to_string(),
        data_type: Some("redis".to_string()),
        kind,
        level: kind.level(),
        mode: if basis == "content" { dbx_core::audit::AuditMode::Content } else { dbx_core::audit::AuditMode::FieldName },
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

async fn apply_connection_meta(state: &AppState, connection_id: &str, findings: &mut [AuditFinding]) {
    let configs = state.configs.read().await;
    let Some(config) = configs.get(connection_id) else {
        return;
    };
    for finding in findings {
        finding.connection_id = Some(connection_id.to_string());
        finding.connection_name = Some(config.name.clone());
        finding.db_type = Some(format!("{:?}", config.db_type).to_ascii_lowercase());
    }
}

async fn connection_db_type(state: &AppState, connection_id: &str) -> Option<DatabaseType> {
    state.configs.read().await.get(connection_id).map(|config| config.db_type)
}

fn is_system_database(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "information_schema" | "mysql" | "performance_schema" | "sys" | "template0" | "template1" | "postgres"
    )
}

fn is_cancelled(job_id: &str) -> bool {
    jobs()
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(job_id).map(|job| job.status == AuditJobStatus::Cancelled))
        .unwrap_or(false)
}
