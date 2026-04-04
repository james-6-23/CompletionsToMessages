use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("格式转换错误: {0}")]
    TransformError(String),

    #[error("上游错误 (状态码 {status}): {body:?}")]
    UpstreamError { status: u16, body: Option<String> },

    #[error("请求转发失败: {0}")]
    ForwardFailed(String),

    #[error("认证失败: {0}")]
    AuthError(String),

    #[error("配置错误: {0}")]
    ConfigError(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            ProxyError::UpstreamError {
                status: upstream_status,
                body: upstream_body,
            } => {
                let http_status =
                    StatusCode::from_u16(*upstream_status).unwrap_or(StatusCode::BAD_GATEWAY);

                let error_body = if let Some(body_str) = upstream_body {
                    // 尝试解析上游 JSON 并透传
                    if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(body_str) {
                        json_body
                    } else {
                        json!({
                            "type": "error",
                            "error": {
                                "type": "upstream_error",
                                "message": body_str,
                            }
                        })
                    }
                } else {
                    json!({
                        "type": "error",
                        "error": {
                            "type": "upstream_error",
                            "message": format!("Upstream error (status {})", upstream_status),
                        }
                    })
                };

                (http_status, error_body)
            }
            _ => {
                let (http_status, error_type) = match &self {
                    ProxyError::TransformError(_) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "transform_error")
                    }
                    ProxyError::ForwardFailed(_) => (StatusCode::BAD_GATEWAY, "forward_error"),
                    ProxyError::AuthError(_) => (StatusCode::UNAUTHORIZED, "authentication_error"),
                    ProxyError::ConfigError(_) => (StatusCode::BAD_REQUEST, "config_error"),
                    ProxyError::Timeout(_) => (StatusCode::GATEWAY_TIMEOUT, "timeout_error"),
                    ProxyError::Internal(_) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    }
                    ProxyError::UpstreamError { .. } => unreachable!(),
                };

                let error_body = json!({
                    "type": "error",
                    "error": {
                        "type": error_type,
                        "message": self.to_string(),
                    }
                });

                (http_status, error_body)
            }
        };

        (status, Json(body)).into_response()
    }
}
