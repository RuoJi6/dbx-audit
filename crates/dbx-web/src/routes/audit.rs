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
    audit_column_findings, audit_job_to_json, audit_job_to_xlsx, detect_field, detect_value, mask_sensitive_value,
    parse_fscan_text, AuditExportFormat, AuditExportResult, AuditFinding, AuditJobState, AuditJobStatus,
    AuditLogEntry, AuditSample, AuditScanRequest, ParsedFscanTargets,
};
use dbx_core::models::connection::DatabaseType;
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
    if connection_db_type(&state, &request.connection_id).await == Some(DatabaseType::Redis) {
        return run_redis_scan(state, request, job_id).await;
    }
    let schema = request.schema.clone().unwrap_or_default();
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

        let tables = if request.tables.is_empty() {
            dbx_core::schema::list_tables_core(&state.app, &request.connection_id, database, &schema, None, None)
                .await?
                .into_iter()
                .map(|table| table.name)
                .collect::<Vec<_>>()
        } else {
            request.tables.clone()
        };

        let total_tables = tables.len().max(1);
        for (table_index, table) in tables.iter().enumerate() {
            if is_cancelled(&state, job_id).await {
                return Ok(());
            }

            let columns =
                dbx_core::schema::get_columns_core(&state.app, &request.connection_id, database, &schema, table)
                    .await?;
            let mut findings = audit_column_findings(
                database,
                if schema.is_empty() { None } else { Some(schema.as_str()) },
                table,
                &columns,
                request.mode,
                request.level,
            );
            apply_connection_meta(&state, &request.connection_id, &mut findings).await;
            if request.mask {
                for finding in &mut findings {
                    finding.samples.clear();
                }
            }

            update_job(&state, job_id, |job| {
                job.findings.extend(findings);
                let database_progress = (database_index * 85) / total_databases;
                let table_progress = ((table_index + 1) * 85) / (total_databases * total_tables);
                job.progress = (10 + database_progress + table_progress).min(95) as u8;
                job.logs.push(log_entry("info", format!("已扫描表：{database}.{table}")));
            })
            .await;
        }
    }

    Ok(())
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

    let databases = dbx_core::schema::list_databases_core(&state.app, &request.connection_id)
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

async fn run_redis_scan(state: Arc<WebState>, request: AuditScanRequest, job_id: &str) -> Result<(), String> {
    update_job(&state, job_id, |job| {
        job.progress = 5;
        job.logs.push(log_entry("info", "正在扫描 Redis keys"));
    })
    .await;
    let databases = if let Some(database) = request.database.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
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
            let page = dbx_core::redis_ops::redis_scan_keys_core(&state.app, &request.connection_id, *db, cursor, "*", 200)
                .await?;
            let mut findings = Vec::new();
            for key in page.keys {
                if is_cancelled(&state, job_id).await {
                    return Ok(());
                }
                let value =
                    dbx_core::redis_ops::redis_get_value_in_db_core(&state.app, &request.connection_id, *db, &key.key_raw)
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

async fn apply_connection_meta(state: &WebState, connection_id: &str, findings: &mut [AuditFinding]) {
    let configs = state.app.configs.read().await;
    let Some(config) = configs.get(connection_id) else {
        return;
    };
    for finding in findings {
        finding.connection_id = Some(connection_id.to_string());
        finding.connection_name = Some(config.name.clone());
        finding.db_type = Some(format!("{:?}", config.db_type).to_ascii_lowercase());
    }
}

async fn connection_db_type(state: &WebState, connection_id: &str) -> Option<DatabaseType> {
    state.app.configs.read().await.get(connection_id).map(|config| config.db_type)
}

fn is_system_database(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "information_schema" | "mysql" | "performance_schema" | "sys" | "template0" | "template1" | "postgres"
    )
}

async fn is_cancelled(state: &WebState, job_id: &str) -> bool {
    state.audit_jobs.read().await.get(job_id).map(|job| job.status == AuditJobStatus::Cancelled).unwrap_or(false)
}
