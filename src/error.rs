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
    UpstreamError {
        status: u16,
        body: Option<String>,
        /// 从上游响应中提取的关键头部，透传给客户端
        #[source]
        upstream_headers: Option<UpstreamHeaders>,
    },

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

/// 封装上游响应头，用于透传给客户端
#[derive(Debug)]
pub struct UpstreamHeaders(pub axum::http::HeaderMap);

impl std::fmt::Display for UpstreamHeaders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UpstreamHeaders({} entries)", self.0.len())
    }
}

impl std::error::Error for UpstreamHeaders {}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        match self {
            ProxyError::UpstreamError {
                status: upstream_status,
                body: upstream_body,
                upstream_headers,
            } => {
                let http_status =
                    StatusCode::from_u16(upstream_status).unwrap_or(StatusCode::BAD_GATEWAY);

                let error_body = if let Some(body_str) = &upstream_body {
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

                let mut resp = (http_status, Json(error_body)).into_response();
                // 透传上游响应头
                if let Some(UpstreamHeaders(headers)) = upstream_headers {
                    for (key, value) in headers.iter() {
                        resp.headers_mut().insert(key.clone(), value.clone());
                    }
                }
                resp
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

                (http_status, Json(error_body)).into_response()
            }
        }
    }
}
