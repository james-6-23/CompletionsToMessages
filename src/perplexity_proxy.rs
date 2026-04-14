//! Perplexity 号池代理模块
//!
//! 将 CompletionsToMessages 管理 API 的 /api/perplexity/* 请求
//! 透明转发到内部 perplexity-svc 服务，并注入管理员 Token。

use crate::error::ProxyError;
use crate::server::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

/// GET /api/perplexity/status — 查询号池整体状态
///
/// 转发到 perplexity-svc 的 GET /pool/status（无需认证）
pub async fn get_status(State(state): State<AppState>) -> Result<Json<Value>, ProxyError> {
    let url = pplx_url(&state)?;

    let resp = state
        .http_client
        .get(format!("{url}/pool/status"))
        .send()
        .await
        .map_err(|e| ProxyError::ForwardFailed(format!("连接 Perplexity 服务失败: {e}")))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| ProxyError::TransformError(format!("解析响应失败: {e}")))?;

    Ok(Json(body))
}

/// POST /api/perplexity/pool/:action — 代理号池管理操作
///
/// 支持的 action：list / add / remove / enable / disable / reset
/// 需要鉴权的操作（add/remove/enable/disable/reset）会自动注入 X-Admin-Token。
pub async fn pool_action(
    State(state): State<AppState>,
    Path(action): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ProxyError> {
    let url = pplx_url(&state)?;
    let body_val = body.map(|b| b.0).unwrap_or_else(|| json!({}));

    let mut req = state
        .http_client
        .post(format!("{url}/pool/{action}"))
        .json(&body_val);

    // 注入 Perplexity 管理员 Token
    if let Some(ref token) = state.pplx_admin_token {
        req = req.header("X-Admin-Token", token.as_str());
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ProxyError::ForwardFailed(format!("连接 Perplexity 服务失败: {e}")))?;

    let result: Value = resp
        .json()
        .await
        .map_err(|e| ProxyError::TransformError(format!("解析响应失败: {e}")))?;

    Ok(Json(result))
}

/// 获取 Perplexity 服务 URL，未配置时返回友好错误
fn pplx_url(state: &AppState) -> Result<&str, ProxyError> {
    state
        .pplx_service_url
        .as_deref()
        .ok_or_else(|| ProxyError::ConfigError("PPLX_SERVICE_URL 未配置".to_string()))
}
