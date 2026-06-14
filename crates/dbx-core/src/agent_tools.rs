use std::sync::Arc;

use serde_json::json;

use crate::agent_events::{ToolCall, ToolDefinition, ToolResult};
use crate::connection::AppState;
use crate::models::connection::DatabaseType;
use crate::query::QueryExecutionOptions;
use crate::sql_risk::SqlRisk;
use crate::types::QueryResult;

/// Maximum number of tables returned by list_tables tool.
const LIST_TABLES_LIMIT: usize = 200;

/// Maximum number of rows returned by execute_query tool.
const EXECUTE_QUERY_LIMIT: usize = 50;

/// Maximum number of rows returned by get_sample_data tool.
const SAMPLE_DATA_LIMIT: usize = 20;

/// Absolute maximum rows any query tool may request.
const MAX_ALLOWED_ROWS: usize = 100;

/// Get all available tool definitions for Phase 1 (read-only tools).
pub fn read_only_tools() -> Vec<ToolDefinition> {
    vec![list_tables_tool(), get_columns_tool()]
}

/// Get all available tool definitions (Phase 2: read-only + execute_query + get_sample_data).
pub fn all_tools() -> Vec<ToolDefinition> {
    vec![list_tables_tool(), get_columns_tool(), execute_query_tool(), get_sample_data_tool()]
}

/// list_tables tool definition.
fn list_tables_tool() -> ToolDefinition {
    ToolDefinition {
        name: "list_tables",
        description: "List all tables and views in the current database. Returns table names, types, and comments.",
        parameters: json!({
            "type": "object",
            "properties": {
                "schema": {
                    "type": "string",
                    "description": "Schema name to list tables from (optional, defaults to current database)"
                }
            },
            "required": []
        }),
        read_only: true,
    }
}

/// get_columns tool definition.
fn get_columns_tool() -> ToolDefinition {
    ToolDefinition {
        name: "get_columns",
        description:
            "Get column definitions for a table: names, types, primary keys, nullable, defaults, and comments. \
             Use this when the user asks about table structure, column details, or field information — \
             even if some schema context was provided, this tool returns the authoritative and complete column list.",
        parameters: json!({
            "type": "object",
            "properties": {
                "table": {
                    "type": "string",
                    "description": "Table name to get columns for"
                },
                "schema": {
                    "type": "string",
                    "description": "Schema name (optional, defaults to current database)"
                }
            },
            "required": ["table"]
        }),
        read_only: true,
    }
}
/// execute_query tool definition (Phase 2).
fn execute_query_tool() -> ToolDefinition {
    ToolDefinition {
        name: "execute_query",
        description: "Execute a read-only SQL query and return results (max 50 rows). \
                      Only SELECT, WITH, SHOW, DESCRIBE, EXPLAIN statements are allowed. \
                      Write operations (INSERT/UPDATE/DELETE/DDL) are blocked.",
        parameters: json!({
            "type": "object",
            "properties": {
                "sql": {
                    "type": "string",
                    "description": "The SQL query to execute"
                },
                "limit": {
                    "type": "number",
                    "description": "Max rows to return (default 50, max 100)"
                }
            },
            "required": ["sql"]
        }),
        read_only: true,
    }
}

/// get_sample_data tool definition (Phase 2).
fn get_sample_data_tool() -> ToolDefinition {
    ToolDefinition {
        name: "get_sample_data",
        description: "Get sample rows from a table to understand its data. Returns up to 20 rows.",
        parameters: json!({
            "type": "object",
            "properties": {
                "table": {
                    "type": "string",
                    "description": "Table name"
                },
                "schema": {
                    "type": "string",
                    "description": "Schema name (optional)"
                },
                "limit": {
                    "type": "number",
                    "description": "Max rows (default 20)"
                }
            },
            "required": ["table"]
        }),
        read_only: true,
    }
}

/// Execute a tool call and return the result.
pub async fn execute_tool(
    tool_call: &ToolCall,
    state: &Arc<AppState>,
    connection_id: &str,
    database: &str,
    db_type: &DatabaseType,
) -> ToolResult {
    let result = match tool_call.name.as_str() {
        "list_tables" => execute_list_tables(tool_call, state, connection_id, database, db_type).await,
        "get_columns" => execute_get_columns(tool_call, state, connection_id, database, db_type).await,
        "execute_query" => execute_execute_query(tool_call, state, connection_id, database, db_type).await,
        "get_sample_data" => execute_get_sample_data(tool_call, state, connection_id, database, db_type).await,
        _ => Err(format!("Unknown tool: {}", tool_call.name)),
    };

    match result {
        Ok(content) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            content,
            is_error: false,
        },
        Err(err) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            content: format!("Error: {err}"),
            is_error: true,
        },
    }
}

async fn execute_list_tables(
    tool_call: &ToolCall,
    state: &Arc<AppState>,
    connection_id: &str,
    database: &str,
    _db_type: &DatabaseType,
) -> Result<String, String> {
    let schema = tool_call.arguments.get("schema").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // Request one extra to detect whether more tables exist beyond the limit.
    let tables = crate::schema::list_tables_core(
        state,
        connection_id,
        database,
        &schema,
        None,
        Some(LIST_TABLES_LIMIT + 1),
        None,
    )
    .await
    .map_err(|e| format!("Failed to list tables: {e}"))?;

    let total = tables.len();
    let truncated = total > LIST_TABLES_LIMIT;

    let mut lines = Vec::new();
    let display_count = if truncated { LIST_TABLES_LIMIT } else { total };
    for table in tables.iter().take(display_count) {
        let mut line = format!("- {} ({})", table.name, table.table_type);
        if let Some(comment) = &table.comment {
            let trimmed = comment.trim();
            if !trimmed.is_empty() {
                line.push_str(&format!(" -- {}", trimmed));
            }
        }
        lines.push(line);
    }

    if truncated {
        lines.push(format!("... (showing {LIST_TABLES_LIMIT} of {total} tables)"));
    }

    if lines.is_empty() {
        return Ok("No tables found in this database/schema.".to_string());
    }

    Ok(lines.join("\n"))
}

async fn execute_get_columns(
    tool_call: &ToolCall,
    state: &Arc<AppState>,
    connection_id: &str,
    database: &str,
    _db_type: &DatabaseType,
) -> Result<String, String> {
    let table = tool_call
        .arguments
        .get("table")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: table")?
        .trim()
        .to_string();

    if table.is_empty() {
        return Err("Table name cannot be empty".to_string());
    }
    if table.len() > 256 {
        return Err(format!("Table name too long: {} characters (max 256)", table.len()));
    }
    // Reject names with characters that are unlikely to be valid identifiers
    if table.contains(';') || table.contains('\'') || table.contains('"') || table.contains('\\') {
        return Err(format!("Table name contains invalid characters: '{}'", table));
    }

    let schema = tool_call.arguments.get("schema").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let columns = crate::schema::get_columns_core(state, connection_id, database, &schema, &table)
        .await
        .map_err(|e| format!("Failed to get columns for {table}: {e}"))?;

    if columns.is_empty() {
        return Ok(format!("No columns found for table '{table}'."));
    }

    let mut lines = Vec::new();
    lines.push(format!("Columns of {table}:"));
    for col in &columns {
        let mut flags: Vec<String> = Vec::new();
        if col.is_primary_key {
            flags.push("PK".to_string());
        }
        if col.is_nullable {
            flags.push("nullable".to_string());
        } else {
            flags.push("NOT NULL".to_string());
        }
        if let Some(default) = &col.column_default {
            if !default.is_empty() {
                flags.push(format!("default {default}"));
            }
        }
        if let Some(extra) = &col.extra {
            if !extra.is_empty() {
                flags.push(extra.clone());
            }
        }

        let flags_str = if flags.is_empty() { String::new() } else { format!(" ({})", flags.join(", ")) };

        let comment_str = col
            .comment
            .as_ref()
            .filter(|c| !c.trim().is_empty())
            .map(|c| format!(" -- {}", c.trim()))
            .unwrap_or_default();

        lines.push(format!("  - {}: {}{}{}", col.name, col.data_type, flags_str, comment_str));
    }

    Ok(lines.join("\n"))
}

/// Execute a read-only SQL query via the execute_query tool.
async fn execute_execute_query(
    tool_call: &ToolCall,
    state: &Arc<AppState>,
    connection_id: &str,
    database: &str,
    db_type: &DatabaseType,
) -> Result<String, String> {
    let sql = tool_call.arguments.get("sql").and_then(|v| v.as_str()).ok_or("Missing required parameter: sql")?.trim();

    if sql.is_empty() {
        return Err("SQL query cannot be empty".to_string());
    }

    let limit = tool_call
        .arguments
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|l| (l as usize).min(MAX_ALLOWED_ROWS))
        .unwrap_or(EXECUTE_QUERY_LIMIT);

    // Classify SQL risk using sqlparser AST
    let db_type_str = format!("{:?}", db_type).to_lowercase();
    let risk = crate::sql_risk::classify_sql_risk(sql, &db_type_str)?;
    match risk {
        SqlRisk::ReadOnly => { /* proceed */ }
        _ => {
            return Err(format!(
                "Blocked: {} statement detected. Only read-only queries (SELECT, SHOW, DESCRIBE, EXPLAIN) are allowed.",
                risk
            ));
        }
    }

    // Execute query using existing infrastructure
    let options = QueryExecutionOptions { max_rows: Some(limit), timeout_secs: Some(30), ..Default::default() };
    let result =
        crate::query::execute_sql_statement_with_options(state, connection_id, database, sql, None, None, options)
            .await?;

    format_query_result_as_text(&result, limit)
}

/// Format a QueryResult as a Markdown table for LLM consumption.
fn format_query_result_as_text(result: &QueryResult, limit: usize) -> Result<String, String> {
    if result.rows.is_empty() {
        return Ok("Query returned 0 rows.".to_string());
    }

    let mut lines = Vec::new();

    // Header row
    lines.push(format!("| {} |", result.columns.join(" | ")));
    // Separator row
    lines.push(format!("|{}|", result.columns.iter().map(|_| "---").collect::<Vec<_>>().join("|")));

    // Data rows
    for row in &result.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|v| match v {
                serde_json::Value::Null => "NULL".to_string(),
                serde_json::Value::String(s) => {
                    // Truncate long strings to keep result compact
                    if s.len() > 200 {
                        let truncated: String =
                            s.char_indices().take_while(|(i, _)| *i < 200).map(|(_, c)| c).collect();
                        format!("{}...", truncated)
                    } else {
                        s.clone()
                    }
                }
                other => other.to_string(),
            })
            .collect();
        lines.push(format!("| {} |", cells.join(" | ")));
    }

    // Truncation notice
    if result.truncated || result.rows.len() >= limit {
        lines.push(format!("... (showing {} rows, result may be truncated)", result.rows.len()));
    }

    // Stats line
    lines.push(format!("({} rows, {}ms)", result.rows.len(), result.execution_time_ms));

    Ok(lines.join("\n"))
}

/// Get sample data from a table via the get_sample_data tool.
async fn execute_get_sample_data(
    tool_call: &ToolCall,
    state: &Arc<AppState>,
    connection_id: &str,
    database: &str,
    db_type: &DatabaseType,
) -> Result<String, String> {
    let table =
        tool_call.arguments.get("table").and_then(|v| v.as_str()).ok_or("Missing required parameter: table")?.trim();

    if table.is_empty() {
        return Err("Table name cannot be empty".to_string());
    }
    if table.contains(';') || table.contains('\'') || table.contains('"') || table.contains('\\') {
        return Err(format!("Table name contains invalid characters: '{}'", table));
    }

    let schema = tool_call.arguments.get("schema").and_then(|v| v.as_str());
    let limit = tool_call
        .arguments
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|l| (l as usize).min(MAX_ALLOWED_ROWS))
        .unwrap_or(SAMPLE_DATA_LIMIT);

    // Build SELECT * FROM table LIMIT N
    let schema_prefix = schema.filter(|s| !s.is_empty()).map(|s| format!("\"{}\".", s)).unwrap_or_default();
    let sql = format!("SELECT * FROM {}\"{}\" LIMIT {}", schema_prefix, table, limit);

    // Delegate to execute_execute_query with a synthetic tool call
    let synthetic_call = ToolCall {
        id: tool_call.id.clone(),
        name: "execute_query".to_string(),
        arguments: serde_json::json!({ "sql": sql, "limit": limit }),
    };
    execute_execute_query(&synthetic_call, state, connection_id, database, db_type).await
}
