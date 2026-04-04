//! 入站认证模块

use crate::database::Database;
use crate::error::ProxyError;
use axum::http::HeaderMap;
use std::sync::Arc;

/// 校验入站请求的认证信息
///
/// 从请求头中提取 token，在 access_tokens 表中查找：
/// - 找到且活跃 → 返回 Ok(Some(token_value))
/// - access_tokens 表为空且 config 也无 auth_token → 放行（开发模式），返回 Ok(None)
/// - 未找到或不活跃 → 返回 Err
///
/// 返回的 String 是匹配到的 access token 原始值，供后续 key_pool 过滤渠道使用。
pub fn validate_auth(
    headers: &HeaderMap,
    db: &Arc<Database>,
    config_token: &Option<String>,
) -> Result<Option<String>, ProxyError> {
    // 从请求头提取入站 token
    let inbound_token = extract_token(headers);

    // 如果有 token，尝试在 access_tokens 表中查找
    if let Some(ref token_val) = inbound_token {
        match db.get_access_token_by_value(token_val) {
            Ok(Some(at)) if at.is_active => {
                return Ok(Some(token_val.clone()));
            }
            Ok(Some(_)) => {
                // token 存在但已禁用
                return Err(ProxyError::AuthError("访问密钥已被禁用".into()));
            }
            Ok(None) => {
                // 不在 access_tokens 表中，继续检查 config fallback
            }
            Err(e) => {
                log::warn!("[cc-proxy] 查询访问密钥失败: {e}");
                // 数据库错误，继续检查 fallback
            }
        }
    }

    // 检查是否有 access_tokens 存在
    let token_count = db.count_access_tokens().unwrap_or(0);
    let config_has_token = config_token.as_ref().map_or(false, |s| !s.is_empty());

    if token_count == 0 && !config_has_token {
        // 开发模式：未配置任何认证，放行
        return Ok(None);
    }

    // fallback: 检查 config auth_token（兼容旧配置文件方式）
    if let Some(ref expected) = config_token {
        if !expected.is_empty() {
            if let Some(ref token_val) = inbound_token {
                if token_val == expected {
                    return Ok(Some(token_val.clone()));
                }
            }
        }
    }

    Err(ProxyError::AuthError(
        "Invalid or missing authentication".into(),
    ))
}

/// 从请求头中提取 token（x-api-key 或 Authorization: Bearer）
fn extract_token(headers: &HeaderMap) -> Option<String> {
    // 检查 x-api-key（Claude Code 默认使用此头）
    if let Some(key) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }

    // 检查 Authorization: Bearer
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    None
}
