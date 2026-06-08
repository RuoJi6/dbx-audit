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
    audit_column_findings, audit_findings_to_xlsx, audit_job_to_json, parse_fscan_text, AuditExportFormat,
    AuditExportResult, AuditJobState, AuditJobStatus, AuditLogEntry, AuditScanRequest, ParsedFscanTargets,
};

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
pub struct ParseFscanRequest {
    pub text_or_file: String,
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
            let workbook = audit_findings_to_xlsx(&job.findings).map_err(AppError)?;
            std::fs::write(&path_buf, workbook).map_err(|err| AppError(err.to_string()))?;
        }
    }

    Ok(Json(AuditExportResult {
        path: path_buf.to_string_lossy().to_string(),
        format: body.format,
        finding_count: job.findings.len(),
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

async fn run_field_name_scan(state: Arc<WebState>, request: AuditScanRequest, job_id: &str) -> Result<(), String> {
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
        .filter(|name| !is_system_database(name))
        .collect::<Vec<_>>();

    if databases.is_empty() {
        return Err("未发现可扫描数据库，请手工填写数据库名".to_string());
    }

    Ok(databases)
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
