//! 统计 API 模块
//!
//! 提供 REST API 端点供前端仪表板查询使用统计

use crate::error::ProxyError;
use crate::server::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// 天数查询参数
#[derive(Debug, Deserialize)]
pub struct DaysParam {
    /// 查询最近 N 天的数据，默认 1
    pub days: Option<u32>,
}

/// 日志查询参数
#[derive(Debug, Deserialize)]
pub struct LogsParam {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub status_code: Option<u16>,
    pub model: Option<String>,
    pub days: Option<u32>,
}

/// 根据天数计算起止时间戳（Unix 秒）
fn time_range_from_days(days: Option<u32>) -> (i64, i64) {
    let now = chrono::Utc::now().timestamp();
    let days = days.unwrap_or(1).max(1) as i64;
    let start = now - days * 86400;
    (start, now)
}

/// 根据天数推算合理的分桶间隔（秒）
fn interval_from_days(days: u32) -> i64 {
    match days {
        0..=1 => 3600,       // 1天：按小时
        2..=7 => 3600 * 6,   // 2-7天：按6小时
        8..=30 => 86400,     // 8-30天：按天
        _ => 86400 * 7,      // 超过30天：按周
    }
}

/// GET /api/stats/summary — 使用统计摘要
pub async fn get_summary(
    State(state): State<AppState>,
    Query(params): Query<DaysParam>,
) -> Result<Json<Value>, ProxyError> {
    let (start_ts, end_ts) = time_range_from_days(params.days);
    let db = Arc::clone(&state.db);

    let summary = tokio::task::spawn_blocking(move || db.get_usage_summary(start_ts, end_ts))
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
    let days = params.days.unwrap_or(1).max(1);
    let (start_ts, end_ts) = time_range_from_days(Some(days));
    let interval_secs = interval_from_days(days);
    let db = Arc::clone(&state.db);

    let trends = tokio::task::spawn_blocking(move || {
        db.get_usage_trends(start_ts, end_ts, interval_secs)
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
    let (start_ts, end_ts) = time_range_from_days(params.days);
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
    let (start_ts, end_ts) = time_range_from_days(params.days);
    let status_code = params.status_code;
    let model = params.model.clone();
    let db = Arc::clone(&state.db);

    let logs = tokio::task::spawn_blocking(move || {
        db.get_request_logs(page, page_size, status_code, model.as_deref(), start_ts, end_ts)
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

    // 从数据库获取上游 URL 和 auth_token 状态
    let upstream_url = state.key_pool.get_upstream_url().await.unwrap_or_default();
    let db = state.db.clone();
    let db_auth = tokio::task::spawn_blocking(move || db.get_setting("auth_token"))
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten();
    let has_auth = db_auth.as_ref().map_or(false, |s| !s.is_empty())
        || config.auth_token.as_ref().map_or(false, |s| !s.is_empty());

    Ok(Json(json!({
        "listen": config.listen,
        "upstream": {
            "base_url": upstream_url,
        },
        "features": {
            "thinking_optimizer": config.features.thinking_optimizer,
        },
        "auth_enabled": has_auth,
        "auth_token_masked": db_auth.as_deref().map(|t| if t.len() > 8 { format!("{}...{}", &t[..4], &t[t.len()-4..]) } else { "****".to_string() }),
        "database_path": config.database_path,
    })))
}

// ===== API Key 管理端点 =====

/// 添加密钥请求体
#[derive(Debug, Deserialize)]
pub struct AddKeyRequest {
    pub api_key: String,
    #[serde(default)]
    pub label: String,
}

/// 更新密钥状态请求体
#[derive(Debug, Deserialize)]
pub struct UpdateKeyStatusRequest {
    pub is_active: bool,
}

/// GET /api/keys — 列出所有 API Key（脱敏）
pub async fn list_keys(
    State(state): State<AppState>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let keys = tokio::task::spawn_blocking(move || db.list_api_keys())
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(keys)))
}

/// POST /api/keys — 添加 API Key
pub async fn add_key(
    State(state): State<AppState>,
    Json(body): Json<AddKeyRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let key = body.api_key.clone();
    let label = body.label.clone();
    let row = tokio::task::spawn_blocking(move || db.add_api_key(&key, &label))
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
    // 1. 从 DB 获取完整密钥
    let db = Arc::clone(&state.db);
    let id_clone = id.clone();
    let full_key = tokio::task::spawn_blocking(move || db.get_api_key_full(&id_clone))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let Some(api_key) = full_key else {
        return Err(ProxyError::Internal("Key not found".to_string()));
    };

    // 2. 获取上游 URL 并发送测试请求
    let upstream_base = state.key_pool.get_upstream_url().await
        .map_err(|e| ProxyError::Internal(format!("获取上游 URL 失败: {e}")))?;

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

// ===== 上游 URL 管理 =====

/// 获取上游 URL
pub async fn get_upstream_url(
    State(state): State<AppState>,
) -> Result<Json<Value>, ProxyError> {
    let url = state.key_pool.get_upstream_url().await.unwrap_or_default();
    Ok(Json(json!({"base_url": url})))
}

/// 设置上游 URL
#[derive(Debug, Deserialize)]
pub struct SetUpstreamUrlRequest {
    pub base_url: String,
}

pub async fn set_upstream_url(
    State(state): State<AppState>,
    Json(body): Json<SetUpstreamUrlRequest>,
) -> Result<Json<Value>, ProxyError> {
    let url = body.base_url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(ProxyError::Internal("上游 URL 不能为空".to_string()));
    }

    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.set_upstream_url(&url))
        .await
        .map_err(|e| ProxyError::Internal(format!("设置上游 URL 失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({"ok": true})))
}

// ===== Auth Token 管理 =====

/// 获取当前 auth_token（脱敏）
pub async fn get_auth_token(
    State(state): State<AppState>,
) -> Result<Json<Value>, ProxyError> {
    let db = state.db.clone();
    let token = tokio::task::spawn_blocking(move || db.get_setting("auth_token"))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let (has_token, masked) = match token.as_deref() {
        Some(t) if !t.is_empty() => {
            let m = if t.len() > 8 {
                format!("{}...{}", &t[..4], &t[t.len()-4..])
            } else {
                "****".to_string()
            };
            (true, Some(m))
        }
        _ => (false, None),
    };

    Ok(Json(json!({
        "has_token": has_token,
        "token_masked": masked,
    })))
}

/// 设置/生成 auth_token
#[derive(Debug, Deserialize)]
pub struct SetAuthTokenRequest {
    /// 如果为空则自动生成
    #[serde(default)]
    pub token: String,
}

pub async fn set_auth_token(
    State(state): State<AppState>,
    Json(body): Json<SetAuthTokenRequest>,
) -> Result<Json<Value>, ProxyError> {
    let token = if body.token.trim().is_empty() {
        // 自动生成
        format!("sk-proxy-{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
    } else {
        body.token.trim().to_string()
    };

    let db = state.db.clone();
    let t = token.clone();
    tokio::task::spawn_blocking(move || db.set_setting("auth_token", &t))
        .await
        .map_err(|e| ProxyError::Internal(format!("保存失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({
        "ok": true,
        "token": token,
    })))
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
        None => true, // 未设置 ADMIN_SECRET 时任何密钥都通过
    };
    Json(json!({
        "valid": valid,
        "auth_required": state.admin_secret.is_some(),
    }))
}
