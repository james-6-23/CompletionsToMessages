//! 统计 API 模块
//!
//! 提供 REST API 端点供前端仪表板查询使用统计

use crate::database::mask_api_key;
use crate::error::ProxyError;
use crate::server::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// 时间范围查询参数
#[derive(Debug, Deserialize)]
pub struct DaysParam {
    /// 查询最近 N 天的数据，默认 1
    pub days: Option<u32>,
    /// 查询最近 N 小时（优先于 days）
    pub hours: Option<u32>,
    pub channel_id: Option<String>,
}

/// 日志查询参数
#[derive(Debug, Deserialize)]
pub struct LogsParam {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub status_code: Option<u16>,
    pub model: Option<String>,
    pub days: Option<u32>,
    pub channel_id: Option<String>,
}

/// 根据参数计算起止时间戳（Unix 秒）
fn time_range_from_params(hours: Option<u32>, days: Option<u32>) -> (i64, i64) {
    let now = chrono::Utc::now().timestamp();
    let secs = if let Some(h) = hours {
        h.max(1) as i64 * 3600
    } else {
        days.unwrap_or(1).max(1) as i64 * 86400
    };
    (now - secs, now)
}

/// 根据时间跨度秒数推算合理的分桶间隔（秒）
fn interval_from_span(span_secs: i64) -> i64 {
    match span_secs {
        0..=3600 => 300,            // ≤1h：5分钟
        3601..=21600 => 900,        // ≤6h：15分钟
        21601..=86400 => 3600,      // ≤1d：1小时
        86401..=604800 => 3600 * 6, // ≤7d：6小时
        604801..=2592000 => 86400,  // ≤30d：1天
        _ => 86400 * 7,             // >30d：1周
    }
}

/// GET /api/stats/summary — 使用统计摘要
pub async fn get_summary(
    State(state): State<AppState>,
    Query(params): Query<DaysParam>,
) -> Result<Json<Value>, ProxyError> {
    let (start_ts, end_ts) = time_range_from_params(params.hours, params.days);
    let db = Arc::clone(&state.db);
    let channel_id = params.channel_id;

    let summary = tokio::task::spawn_blocking(move || {
        db.get_usage_summary(start_ts, end_ts, channel_id.as_deref())
    })
    .await
    .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
    .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(summary)))
}

/// GET /api/stats/trends — 使用趋势（时间分桶）
pub async fn get_trends(
    State(state): State<AppState>,
    Query(params): Query<DaysParam>,
) -> Result<Json<Value>, ProxyError> {
    let (start_ts, end_ts) = time_range_from_params(params.hours, params.days);
    let interval_secs = interval_from_span(end_ts - start_ts);
    let db = Arc::clone(&state.db);
    let channel_id = params.channel_id;

    let trends = tokio::task::spawn_blocking(move || {
        db.get_usage_trends(start_ts, end_ts, interval_secs, channel_id.as_deref())
    })
    .await
    .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
    .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(trends)))
}

/// GET /api/stats/models — 模型维度统计
pub async fn get_models(
    State(state): State<AppState>,
    Query(params): Query<DaysParam>,
) -> Result<Json<Value>, ProxyError> {
    let (start_ts, end_ts) = time_range_from_params(None, params.days);
    let db = Arc::clone(&state.db);

    let stats = tokio::task::spawn_blocking(move || db.get_model_stats(start_ts, end_ts))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(stats)))
}

/// GET /api/stats/logs — 分页请求日志
pub async fn get_logs(
    State(state): State<AppState>,
    Query(params): Query<LogsParam>,
) -> Result<Json<Value>, ProxyError> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).min(100);
    let (start_ts, end_ts) = time_range_from_params(None, params.days);
    let status_code = params.status_code;
    let model = params.model.clone();
    let channel_id = params.channel_id.clone();
    let db = Arc::clone(&state.db);

    let logs = tokio::task::spawn_blocking(move || {
        db.get_request_logs(page, page_size, status_code, model.as_deref(), channel_id.as_deref(), start_ts, end_ts)
    })
    .await
    .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
    .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(logs)))
}

/// GET /api/stats/pricing — 模型定价列表
pub async fn get_pricing(
    State(state): State<AppState>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);

    let pricing = tokio::task::spawn_blocking(move || db.get_model_pricing())
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(pricing)))
}

/// GET /api/config — 脱敏后的配置信息
pub async fn get_config_info(
    State(state): State<AppState>,
) -> Result<Json<Value>, ProxyError> {
    let config = &state.config;

    // 从数据库获取端点列表和访问密钥数量
    let db = state.db.clone();
    let db2 = state.db.clone();
    let endpoints = tokio::task::spawn_blocking(move || db.list_endpoints())
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    let access_tokens_count = tokio::task::spawn_blocking(move || db2.count_access_tokens())
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or(0);

    let has_auth = access_tokens_count > 0
        || config.auth_token.as_ref().map_or(false, |s| !s.is_empty());

    Ok(Json(json!({
        "listen": config.listen,
        "endpoints_count": endpoints.len(),
        "features": {
            "thinking_optimizer": config.features.thinking_optimizer,
        },
        "auth_enabled": has_auth,
        "access_tokens_count": access_tokens_count,
        "database_path": config.database_path,
    })))
}

// ===== 上游端点管理 =====

/// 添加端点请求体
#[derive(Debug, Deserialize)]
pub struct AddEndpointRequest {
    pub name: String,
    pub base_url: String,
}

/// 更新端点请求体
#[derive(Debug, Deserialize)]
pub struct UpdateEndpointRequest {
    pub name: String,
    pub base_url: String,
}

/// 更新端点状态请求体
#[derive(Debug, Deserialize)]
pub struct UpdateEndpointStatusRequest {
    pub is_active: bool,
}

/// GET /api/endpoints — 列出所有上游端点
pub async fn list_endpoints(
    State(state): State<AppState>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let endpoints = tokio::task::spawn_blocking(move || db.list_endpoints())
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(endpoints)))
}

/// POST /api/endpoints — 添加上游端点
pub async fn add_endpoint(
    State(state): State<AppState>,
    Json(body): Json<AddEndpointRequest>,
) -> Result<Json<Value>, ProxyError> {
    let url = body.base_url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(ProxyError::Internal("端点 URL 不能为空".to_string()));
    }
    let name = body.name.trim().to_string();
    let db = Arc::clone(&state.db);
    let row = tokio::task::spawn_blocking(move || db.add_endpoint(&name, &url))
        .await
        .map_err(|e| ProxyError::Internal(format!("添加端点失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(row)))
}

/// PUT /api/endpoints/:id — 更新上游端点
pub async fn update_endpoint(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateEndpointRequest>,
) -> Result<Json<Value>, ProxyError> {
    let url = body.base_url.trim().trim_end_matches('/').to_string();
    let name = body.name.trim().to_string();
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || db.update_endpoint(&id, &name, &url))
        .await
        .map_err(|e| ProxyError::Internal(format!("更新端点失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

/// PUT /api/endpoints/:id/status — 更新端点启用状态
pub async fn update_endpoint_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateEndpointStatusRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let active = body.is_active;
    tokio::task::spawn_blocking(move || db.update_endpoint_status(&id, active))
        .await
        .map_err(|e| ProxyError::Internal(format!("更新端点状态失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

/// DELETE /api/endpoints/:id — 删除端点（含关联 key）
pub async fn delete_endpoint(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || db.delete_endpoint(&id))
        .await
        .map_err(|e| ProxyError::Internal(format!("删除端点失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

// ===== API Key 管理端点 =====

/// 添加密钥请求体
#[derive(Debug, Deserialize)]
pub struct AddKeyRequest {
    pub endpoint_id: String,
    pub api_key: String,
    #[serde(default)]
    pub label: String,
}

/// 更新密钥状态请求体
#[derive(Debug, Deserialize)]
pub struct UpdateKeyStatusRequest {
    pub is_active: bool,
}

/// Key 列表查询参数
#[derive(Debug, Deserialize)]
pub struct ListKeysParam {
    pub endpoint_id: Option<String>,
}

/// GET /api/keys — 列出 API Key（可选按端点过滤）
pub async fn list_keys(
    State(state): State<AppState>,
    Query(params): Query<ListKeysParam>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let eid = params.endpoint_id.clone();
    let keys = tokio::task::spawn_blocking(move || db.list_api_keys(eid.as_deref()))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(keys)))
}

/// POST /api/keys — 添加 API Key（绑定到端点）
pub async fn add_key(
    State(state): State<AppState>,
    Json(body): Json<AddKeyRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let endpoint_id = body.endpoint_id.clone();
    let key = body.api_key.clone();
    let label = body.label.clone();
    let row = tokio::task::spawn_blocking(move || db.add_api_key(&endpoint_id, &key, &label))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(row)))
}

/// DELETE /api/keys/:id — 删除 API Key
pub async fn delete_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || db.delete_api_key(&id))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

/// PUT /api/keys/:id/status — 更新 API Key 启用状态
pub async fn update_key_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateKeyStatusRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let active = body.is_active;
    tokio::task::spawn_blocking(move || db.update_api_key_status(&id, active))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

/// POST /api/keys/:id/test — 测试 API Key 是否有效
pub async fn test_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ProxyError> {
    // 从 DB 获取完整密钥 + 端点 URL
    let db = Arc::clone(&state.db);
    let id_clone = id.clone();
    let full_key = tokio::task::spawn_blocking(move || db.get_api_key_full(&id_clone))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let Some((api_key, upstream_base)) = full_key else {
        return Err(ProxyError::Internal("Key not found".to_string()));
    };

    if upstream_base.is_empty() {
        return Err(ProxyError::Internal("密钥未绑定有效端点".to_string()));
    }

    let test_body = json!({
        "model": "gpt-4o-mini",
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1
    });

    let upstream_url = format!(
        "{}/v1/chat/completions",
        upstream_base.trim_end_matches('/')
    );

    let resp = state
        .http_client
        .post(&upstream_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&test_body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            Ok(Json(json!({"valid": true, "status": r.status().as_u16()})))
        }
        Ok(r) => {
            let status = r.status().as_u16();
            let body = r.text().await.unwrap_or_default();
            Ok(Json(json!({"valid": false, "status": status, "error": body})))
        }
        Err(e) => Ok(Json(json!({"valid": false, "error": e.to_string()}))),
    }
}

// ===== 访问密钥管理 =====

/// 添加访问密钥请求体
#[derive(Debug, Deserialize)]
pub struct AddAccessTokenRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub channel_ids: Vec<String>,
}

/// 更新访问密钥状态请求体
#[derive(Debug, Deserialize)]
pub struct UpdateAccessTokenStatusRequest {
    pub is_active: bool,
}

/// 更新访问密钥渠道绑定请求体
#[derive(Debug, Deserialize)]
pub struct UpdateAccessTokenChannelsRequest {
    pub channel_ids: Vec<String>,
}

/// GET /api/access-tokens — 列出所有访问密钥
pub async fn list_access_tokens(
    State(state): State<AppState>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let tokens = tokio::task::spawn_blocking(move || db.list_access_tokens())
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(tokens)))
}

/// POST /api/access-tokens — 添加访问密钥
///
/// 返回含完整 token 的行（仅创建时展示一次）
pub async fn add_access_token(
    State(state): State<AppState>,
    Json(body): Json<AddAccessTokenRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let name = body.name.trim().to_string();
    let channel_ids = body.channel_ids;
    let row = tokio::task::spawn_blocking(move || db.add_access_token(&name, &channel_ids))
        .await
        .map_err(|e| ProxyError::Internal(format!("添加访问密钥失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    // 创建时返回完整 token（token_masked 字段此时存储完整值）
    Ok(Json(json!({
        "id": row.id,
        "token": row.token_masked,
        "token_masked": mask_api_key(&row.token_masked),
        "name": row.name,
        "is_active": row.is_active,
        "total_requests": row.total_requests,
        "failed_requests": row.failed_requests,
        "last_used_at": row.last_used_at,
        "channel_ids": row.channel_ids,
        "created_at": row.created_at,
    })))
}

/// DELETE /api/access-tokens/:id — 删除访问密钥
pub async fn delete_access_token(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || db.delete_access_token(&id))
        .await
        .map_err(|e| ProxyError::Internal(format!("删除访问密钥失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

/// PUT /api/access-tokens/:id/status — 更新访问密钥启用状态
pub async fn update_access_token_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateAccessTokenStatusRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let active = body.is_active;
    tokio::task::spawn_blocking(move || db.update_access_token_status(&id, active))
        .await
        .map_err(|e| ProxyError::Internal(format!("更新访问密钥状态失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

/// PUT /api/access-tokens/:id/channels — 更新访问密钥绑定的渠道
pub async fn update_access_token_channels(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateAccessTokenChannelsRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let channel_ids = body.channel_ids;
    tokio::task::spawn_blocking(move || db.update_access_token_channels(&id, &channel_ids))
        .await
        .map_err(|e| ProxyError::Internal(format!("更新访问密钥渠道绑定失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

// ===== 管理登录验证 =====

#[derive(Debug, Deserialize)]
pub struct VerifyAdminRequest {
    pub secret: String,
}

/// 验证管理密钥（前端登录用，不受 admin_auth 中间件保护）
pub async fn verify_admin_secret(
    State(state): State<AppState>,
    Json(body): Json<VerifyAdminRequest>,
) -> Json<Value> {
    let valid = match &state.admin_secret {
        Some(secret) => body.secret == *secret,
        None => true,
    };
    Json(json!({
        "valid": valid,
        "auth_required": state.admin_secret.is_some(),
    }))
}
