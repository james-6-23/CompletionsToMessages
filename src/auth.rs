//! 入站认证模块

use axum::http::HeaderMap;
use crate::database::Database;
use crate::error::ProxyError;
use std::sync::Arc;

/// 校验入站请求的认证信息
///
/// 优先从数据库读取 auth_token，fallback 到 config。
/// 两者都未设置则放行（但 /api/* 管理接口除外）。
pub fn validate_auth(
    headers: &HeaderMap,
    db: &Arc<Database>,
    config_token: &Option<String>,
) -> Result<(), ProxyError> {
    // 从数据库获取 token，fallback 到 config
    let db_token = db.get_setting("auth_token").unwrap_or(None);
    let expected_token = db_token.as_deref()
        .filter(|s| !s.is_empty())
        .or(config_token.as_deref().filter(|s| !s.is_empty()));

    let Some(expected) = expected_token else {
        return Ok(()); // 未配置认证，放行
    };

    // 检查 x-api-key（Claude Code 默认使用此头）
    if let Some(key) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        if key == expected {
            return Ok(());
        }
    }

    // 检查 Authorization: Bearer
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            if token == expected {
                return Ok(());
            }
        }
    }

    Err(ProxyError::AuthError("Invalid or missing authentication".into()))
}
