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
    /// 查询最近 N 分钟（优先于 hours）
    pub minutes: Option<u32>,
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
    pub hours: Option<u32>,
    pub channel_id: Option<String>,
}

/// 根据参数计算起止时间戳（Unix 秒）
fn time_range_from_params(
    minutes: Option<u32>,
    hours: Option<u32>,
    days: Option<u32>,
) -> (i64, i64) {
    let now = chrono::Utc::now().timestamp();
    let secs = if let Some(m) = minutes {
        m.max(1) as i64 * 60
    } else if let Some(h) = hours {
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
    let (start_ts, end_ts) = time_range_from_params(params.minutes, params.hours, params.days);
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
    let (start_ts, end_ts) = time_range_from_params(params.minutes, params.hours, params.days);
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
    let (start_ts, end_ts) = time_range_from_params(None, params.hours, params.days);
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
    let (start_ts, end_ts) = time_range_from_params(None, params.hours, params.days);
    let status_code = params.status_code;
    let model = params.model.clone();
    let channel_id = params.channel_id.clone();
    let db = Arc::clone(&state.db);

    let logs = tokio::task::spawn_blocking(move || {
        db.get_request_logs(
            page,
            page_size,
            status_code,
            model.as_deref(),
            channel_id.as_deref(),
            start_ts,
            end_ts,
        )
    })
    .await
    .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
    .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(logs)))
}

/// GET /api/stats/pricing — 模型定价列表
pub async fn get_pricing(State(state): State<AppState>) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);

    let pricing = tokio::task::spawn_blocking(move || db.get_model_pricing())
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!(pricing)))
}

/// GET /api/config — 脱敏后的配置信息
pub async fn get_config_info(State(state): State<AppState>) -> Result<Json<Value>, ProxyError> {
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

    let has_auth =
        access_tokens_count > 0 || config.auth_token.as_ref().map_or(false, |s| !s.is_empty());

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

// ===== KV 设置 =====

/// GET /api/settings/:key — 读取设置
pub async fn get_setting(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let k = key.clone();
    let val = tokio::task::spawn_blocking(move || db.get_setting(&k))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询设置失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;
    Ok(Json(json!({ "key": key, "value": val })))
}

/// PUT /api/settings/:key — 写入设置
pub async fn set_setting(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ProxyError> {
    let value = body
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let db = Arc::clone(&state.db);
    let k = key.clone();
    tokio::task::spawn_blocking(move || db.set_setting(&k, &value))
        .await
        .map_err(|e| ProxyError::Internal(format!("保存设置失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;
    Ok(Json(json!({ "ok": true })))
}

// ===== 上游端点管理 =====

/// 标准化代理 URL
/// 支持格式：
/// - `http://...` / `https://...` / `socks5://...` → 原样返回
/// - `主机:端口:用户名:密码` → `http://用户名:密码@主机:端口`
/// - `用户名:密码@主机:端口` → `http://用户名:密码@主机:端口`
/// - `主机:端口` → `http://主机:端口`
/// - 空字符串 → 空字符串
fn normalize_proxy_url(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    // 已有协议前缀，直接返回
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("socks5://") {
        return s.to_string();
    }
    // 格式: user:pass@host:port
    if s.contains('@') {
        return format!("http://{s}");
    }
    // 格式: host:port:user:pass（按 : 分割 4 段）
    let parts: Vec<&str> = s.splitn(4, ':').collect();
    if parts.len() == 4 {
        let (host, port, user, pass) = (parts[0], parts[1], parts[2], parts[3]);
        return format!("http://{user}:{pass}@{host}:{port}");
    }
    // 格式: host:port
    format!("http://{s}")
}

/// 添加端点请求体
#[derive(Debug, Deserialize)]
pub struct AddEndpointRequest {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub website_url: String,
    #[serde(default)]
    pub logo_url: String,
    #[serde(default)]
    pub proxy_url: String,
}

/// 更新端点请求体
#[derive(Debug, Deserialize)]
pub struct UpdateEndpointRequest {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub website_url: String,
    #[serde(default)]
    pub logo_url: String,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub model_mapping: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub max_failures: Option<u32>,
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub strip_tools: Option<bool>,
}

/// 更新端点状态请求体
#[derive(Debug, Deserialize)]
pub struct UpdateEndpointStatusRequest {
    pub is_active: bool,
}

/// GET /api/endpoints — 列出所有上游端点
pub async fn list_endpoints(State(state): State<AppState>) -> Result<Json<Value>, ProxyError> {
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
    let website = body.website_url.trim().to_string();
    let logo = body.logo_url.trim().to_string();
    let proxy = normalize_proxy_url(&body.proxy_url);
    let db = Arc::clone(&state.db);
    let row = tokio::task::spawn_blocking(move || db.add_endpoint(&name, &url, &website, &logo, &proxy))
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
    let website = body.website_url.trim().to_string();
    let logo = body.logo_url.trim().to_string();
    let proxy = normalize_proxy_url(&body.proxy_url);
    let mapping = body.model_mapping.clone();
    let max_failures = body.max_failures;
    let max_retries = body.max_retries;
    let strip_tools = body.strip_tools;
    let db = Arc::clone(&state.db);
    let ep_id = id.clone();
    tokio::task::spawn_blocking(move || {
        db.update_endpoint(&ep_id, &name, &url, &website, &logo, &proxy)?;
        if let Some(m) = mapping {
            db.update_endpoint_model_mapping(&ep_id, &m)?;
        }
        if max_failures.is_some() || max_retries.is_some() || strip_tools.is_some() {
            db.update_endpoint_limits(
                &ep_id,
                max_failures.unwrap_or(0),
                max_retries.unwrap_or(0),
                strip_tools.unwrap_or(false),
            )?;
        }
        Ok::<(), String>(())
    })
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

/// 批量添加密钥请求体
#[derive(Debug, Deserialize)]
pub struct BatchAddKeysRequest {
    pub endpoint_id: String,
    pub api_keys: Vec<String>,
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
///
/// 首次为渠道添加密钥时，自动从上游同步模型列表
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

    // 检查渠道是否尚未同步模型列表，是则后台自动同步
    let ep_id = body.endpoint_id.clone();
    let db2 = Arc::clone(&state.db);
    let ep_models: Vec<String> = tokio::task::spawn_blocking(move || {
        db2.list_endpoints()
            .unwrap_or_default()
            .into_iter()
            .find(|e| e.id == ep_id)
            .map(|e| e.models)
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    if ep_models.is_empty() {
        let state2 = state.clone();
        let ep_id2 = body.endpoint_id.clone();
        let api_key_val = body.api_key.clone();
        tokio::spawn(async move {
            // 用刚添加的 key 请求上游模型列表
            let db3 = Arc::clone(&state2.db);
            let ep_id3 = ep_id2.clone();
            let base_url = tokio::task::spawn_blocking(move || db3.get_endpoint_url(&ep_id3))
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten();

            if let Some(url) = base_url {
                let models_url = format!("{}/v1/models", url.trim_end_matches('/'));
                if let Ok(resp) = state2
                    .http_client
                    .get(&models_url)
                    .header("Authorization", format!("Bearer {}", api_key_val))
                    .send()
                    .await
                {
                    if resp.status().is_success() {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            let model_ids: Vec<String> = body
                                .get("data")
                                .and_then(|d| d.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|m| {
                                            let mid =
                                                m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                            if crate::handler::is_claude_model(mid) {
                                                Some(mid.to_string())
                                            } else {
                                                None
                                            }
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();

                            if !model_ids.is_empty() {
                                let count = model_ids.len();
                                let db4 = Arc::clone(&state2.db);
                                let ep_id4 = ep_id2.clone();
                                let _ = tokio::task::spawn_blocking(move || {
                                    db4.update_endpoint_models(&ep_id4, &model_ids)
                                })
                                .await;
                                log::info!(
                                    "[cc-proxy] 自动同步渠道 {} 模型列表: {} 个",
                                    ep_id2,
                                    count
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    Ok(Json(json!(row)))
}

/// POST /api/keys/batch — 批量添加 API Key（单事务，高性能）
pub async fn batch_add_keys(
    State(state): State<AppState>,
    Json(body): Json<BatchAddKeysRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let endpoint_id = body.endpoint_id.clone();
    let api_keys = body.api_keys.clone();

    let rows =
        tokio::task::spawn_blocking(move || db.add_api_keys_batch(&endpoint_id, &api_keys))
            .await
            .map_err(|e| ProxyError::Internal(format!("批量添加失败: {e}")))?
            .map_err(|e| ProxyError::Internal(e))?;

    let count = rows.len();

    // 首次为渠道添加密钥时，自动从上游同步模型列表
    let ep_id = body.endpoint_id.clone();
    let db2 = Arc::clone(&state.db);
    let ep_models: Vec<String> = tokio::task::spawn_blocking(move || {
        db2.list_endpoints()
            .unwrap_or_default()
            .into_iter()
            .find(|e| e.id == ep_id)
            .map(|e| e.models)
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    if ep_models.is_empty() && count > 0 {
        let state2 = state.clone();
        let ep_id2 = body.endpoint_id.clone();
        let first_key = body
            .api_keys
            .iter()
            .find(|k| !k.trim().is_empty())
            .cloned()
            .unwrap_or_default();
        if !first_key.is_empty() {
            tokio::spawn(async move {
                let db3 = Arc::clone(&state2.db);
                let ep_id3 = ep_id2.clone();
                let base_url =
                    tokio::task::spawn_blocking(move || db3.get_endpoint_url(&ep_id3))
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .flatten();
                if let Some(url) = base_url {
                    let models_url = format!("{}/v1/models", url.trim_end_matches('/'));
                    if let Ok(resp) = state2
                        .http_client
                        .get(&models_url)
                        .header("Authorization", format!("Bearer {}", first_key))
                        .send()
                        .await
                    {
                        if resp.status().is_success() {
                            if let Ok(body) = resp.json::<serde_json::Value>().await {
                                let model_ids: Vec<String> = body
                                    .get("data")
                                    .and_then(|d| d.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|m| {
                                                let mid = m
                                                    .get("id")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("");
                                                if crate::handler::is_claude_model(mid) {
                                                    Some(mid.to_string())
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                if !model_ids.is_empty() {
                                    let cnt = model_ids.len();
                                    let db4 = Arc::clone(&state2.db);
                                    let ep_id4 = ep_id2.clone();
                                    let _ = tokio::task::spawn_blocking(move || {
                                        db4.update_endpoint_models(&ep_id4, &model_ids)
                                    })
                                    .await;
                                    log::info!(
                                        "[cc-proxy] 自动同步渠道 {} 模型列表: {} 个",
                                        ep_id2,
                                        cnt
                                    );
                                }
                            }
                        }
                    }
                }
            });
        }
    }

    Ok(Json(json!({ "ok": true, "count": count })))
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

/// 批量密钥操作请求体
#[derive(Debug, Deserialize)]
pub struct BatchKeyActionRequest {
    pub endpoint_id: String,
    /// "all" | "valid" | "invalid"
    #[serde(default = "default_all")]
    pub status: String,
}

fn default_all() -> String {
    "all".to_string()
}

/// DELETE /api/keys/batch — 批量删除密钥
pub async fn batch_delete_keys(
    State(state): State<AppState>,
    Json(body): Json<BatchKeyActionRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let eid = body.endpoint_id.clone();
    let filter = match body.status.as_str() {
        "valid" => Some(true),
        "invalid" => Some(false),
        _ => None,
    };
    let count = tokio::task::spawn_blocking(move || db.delete_keys_by_endpoint(&eid, filter))
        .await
        .map_err(|e| ProxyError::Internal(format!("批量删除失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;
    Ok(Json(json!({"ok": true, "count": count})))
}

/// POST /api/keys/restore — 批量恢复失效密钥
pub async fn batch_restore_keys(
    State(state): State<AppState>,
    Json(body): Json<BatchKeyActionRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let eid = body.endpoint_id.clone();
    let count = tokio::task::spawn_blocking(move || db.restore_invalid_keys(&eid))
        .await
        .map_err(|e| ProxyError::Internal(format!("恢复密钥失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;
    Ok(Json(json!({"ok": true, "count": count})))
}

/// POST /api/keys/export — 导出密钥（完整值）
pub async fn export_keys(
    State(state): State<AppState>,
    Json(body): Json<BatchKeyActionRequest>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let eid = body.endpoint_id.clone();
    let filter = match body.status.as_str() {
        "valid" => Some(true),
        "invalid" => Some(false),
        _ => None,
    };
    let keys = tokio::task::spawn_blocking(move || db.export_keys(&eid, filter))
        .await
        .map_err(|e| ProxyError::Internal(format!("导出失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;
    Ok(Json(json!({"ok": true, "keys": keys, "count": keys.len()})))
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

/// GET /api/keys/:id/full — 获取完整 API Key（用于复制）
pub async fn get_key_full(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ProxyError> {
    let db = Arc::clone(&state.db);
    let full_key = tokio::task::spawn_blocking(move || db.get_api_key_full(&id))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询任务失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let Some((api_key, _)) = full_key else {
        return Err(ProxyError::Internal("Key not found".to_string()));
    };

    Ok(Json(json!({"api_key": api_key})))
}

/// GET /api/endpoints/:id/models — 获取端点的模型列表
pub async fn get_endpoint_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ProxyError> {
    // 获取端点 URL + 第一个活跃 key
    let db = Arc::clone(&state.db);
    let id_clone = id.clone();
    let endpoint_url = tokio::task::spawn_blocking(move || db.get_endpoint_url(&id_clone))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let Some(base_url) = endpoint_url else {
        return Err(ProxyError::Internal("端点不存在或未启用".to_string()));
    };

    let db2 = Arc::clone(&state.db);
    let keys = tokio::task::spawn_blocking(move || db2.list_api_keys(Some(&id)))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let active_key = keys.iter().find(|k| k.is_active);
    let Some(key_row) = active_key else {
        return Err(ProxyError::Internal("该端点暂无活跃密钥".to_string()));
    };

    // 获取完整 key
    let db3 = Arc::clone(&state.db);
    let key_id = key_row.id.clone();
    let full = tokio::task::spawn_blocking(move || db3.get_api_key_full(&key_id))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let Some((api_key, _)) = full else {
        return Err(ProxyError::Internal("无法获取密钥".to_string()));
    };

    let models_url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let resp = state
        .http_client
        .get(&models_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| ProxyError::Internal(format!("请求模型列表失败: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Ok(Json(json!({"error": body, "status": status, "data": []})));
    }

    let body: Value = resp.json().await.unwrap_or(json!({"data": []}));
    Ok(Json(body))
}

/// POST /api/endpoints/:id/sync-models — 从上游同步端点支持的模型列表
///
/// 请求上游 /v1/models，过滤出 Claude 系列模型，保存到数据库
pub async fn sync_endpoint_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ProxyError> {
    // 1. 获取端点 URL + 第一个活跃 key
    let db = Arc::clone(&state.db);
    let id_clone = id.clone();
    let endpoint_url = tokio::task::spawn_blocking(move || db.get_endpoint_url(&id_clone))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let Some(base_url) = endpoint_url else {
        return Err(ProxyError::Internal("端点不存在或未启用".to_string()));
    };

    let db2 = Arc::clone(&state.db);
    let id_clone2 = id.clone();
    let keys = tokio::task::spawn_blocking(move || db2.list_api_keys(Some(&id_clone2)))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let active_key = keys.iter().find(|k| k.is_active);
    let Some(key_row) = active_key else {
        return Err(ProxyError::Internal("该端点暂无活跃密钥".to_string()));
    };

    // 获取完整 key
    let db3 = Arc::clone(&state.db);
    let key_id = key_row.id.clone();
    let full = tokio::task::spawn_blocking(move || db3.get_api_key_full(&key_id))
        .await
        .map_err(|e| ProxyError::Internal(format!("查询失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    let Some((api_key, _)) = full else {
        return Err(ProxyError::Internal("无法获取密钥".to_string()));
    };

    // 2. 请求上游 /v1/models
    let models_url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let resp = state
        .http_client
        .get(&models_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| ProxyError::Internal(format!("请求模型列表失败: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(ProxyError::Internal(format!(
            "上游返回错误 {status}: {body}"
        )));
    }

    let body: Value = resp.json().await.unwrap_or(json!({"data": []}));

    // 3. 过滤 Claude 模型，提取 ID
    let model_ids: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let mid = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if crate::handler::is_claude_model(mid) {
                        Some(mid.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // 4. 保存到数据库
    let db4 = Arc::clone(&state.db);
    let id_clone3 = id.clone();
    let models_to_save = model_ids.clone();
    tokio::task::spawn_blocking(move || db4.update_endpoint_models(&id_clone3, &models_to_save))
        .await
        .map_err(|e| ProxyError::Internal(format!("保存模型列表失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    log::info!(
        "[cc-proxy] 端点 {} 同步模型列表: {} 个模型",
        id,
        model_ids.len()
    );

    // 5. 返回结果
    Ok(Json(json!({
        "ok": true,
        "models": model_ids,
        "count": model_ids.len(),
    })))
}

/// PUT /api/endpoints/:id/models — 手动设置端点支持的模型列表
pub async fn update_endpoint_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ProxyError> {
    let models: Vec<String> = body
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let db = Arc::clone(&state.db);
    let ep_id = id.clone();
    let models_clone = models.clone();
    tokio::task::spawn_blocking(move || db.update_endpoint_models(&ep_id, &models_clone))
        .await
        .map_err(|e| ProxyError::Internal(format!("更新模型列表失败: {e}")))?
        .map_err(|e| ProxyError::Internal(e))?;

    Ok(Json(json!({
        "ok": true,
        "models": models,
        "count": models.len(),
    })))
}

/// 测试密钥请求体
#[derive(Debug, Deserialize)]
pub struct TestKeyRequest {
    #[serde(default)]
    pub model: Option<String>,
}

/// POST /api/keys/:id/test — 测试 API Key 是否有效
pub async fn test_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TestKeyRequest>,
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

    let model = body.model.unwrap_or_else(|| "gpt-4o-mini".to_string());
    let test_body = json!({
        "model": model,
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
            // 测试成功：自动恢复为有效状态
            let db = Arc::clone(&state.db);
            let key_id = id.clone();
            let _ = tokio::task::spawn_blocking(move || db.update_api_key_status(&key_id, true))
                .await;
            log::info!(
                "[cc-proxy] 密钥 {} 测试通过，自动恢复为有效状态",
                &id[..id.len().min(8)]
            );
            Ok(Json(json!({"valid": true, "status": r.status().as_u16()})))
        }
        Ok(r) => {
            let status = r.status().as_u16();
            let body = r.text().await.unwrap_or_default();
            Ok(Json(
                json!({"valid": false, "status": status, "error": body}),
            ))
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
pub async fn list_access_tokens(State(state): State<AppState>) -> Result<Json<Value>, ProxyError> {
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

// ===== 代理连通性测试 =====

/// 测试代理请求体
#[derive(Debug, Deserialize)]
pub struct TestProxyRequest {
    pub proxy_url: String,
}

/// POST /api/endpoints/test-proxy — 测试代理连通性，返回延迟和地区
pub async fn test_proxy(
    State(_state): State<AppState>,
    Json(body): Json<TestProxyRequest>,
) -> Result<Json<Value>, ProxyError> {
    let proxy_url = normalize_proxy_url(&body.proxy_url);
    if proxy_url.is_empty() {
        return Err(ProxyError::Internal("代理地址不能为空".to_string()));
    }

    // 构建带代理的 HTTP 客户端
    let proxy = reqwest::Proxy::all(&proxy_url)
        .map_err(|e| ProxyError::Internal(format!("无效的代理地址: {e}")))?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| ProxyError::Internal(format!("构建代理客户端失败: {e}")))?;

    // 通过代理请求 ip-api.com 获取 IP 地理位置
    let start = std::time::Instant::now();
    let resp = client
        .get("http://ip-api.com/json?lang=zh-CN&fields=status,country,regionName,city,query,message")
        .send()
        .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match resp {
        Ok(r) if r.status().is_success() => {
            let body: Value = r.json().await.unwrap_or(json!({}));
            if body.get("status").and_then(|s| s.as_str()) == Some("success") {
                let country = body["country"].as_str().unwrap_or("未知");
                let region = body["regionName"].as_str().unwrap_or("");
                let city = body["city"].as_str().unwrap_or("");
                let ip = body["query"].as_str().unwrap_or("未知");
                let location = if !city.is_empty() {
                    format!("{country} {region} {city}")
                } else if !region.is_empty() {
                    format!("{country} {region}")
                } else {
                    country.to_string()
                };
                Ok(Json(json!({
                    "ok": true,
                    "latency_ms": latency_ms,
                    "location": location,
                    "ip": ip,
                })))
            } else {
                let msg = body["message"].as_str().unwrap_or("IP 查询失败");
                Ok(Json(json!({
                    "ok": false,
                    "error": msg,
                    "latency_ms": latency_ms,
                })))
            }
        }
        Ok(r) => {
            let status = r.status().as_u16();
            Ok(Json(json!({
                "ok": false,
                "error": format!("IP 查询返回 HTTP {status}"),
                "latency_ms": latency_ms,
            })))
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                "代理连接超时".to_string()
            } else {
                format!("代理连接失败: {e}")
            };
            Ok(Json(json!({
                "ok": false,
                "error": msg,
                "latency_ms": latency_ms,
            })))
        }
    }
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
