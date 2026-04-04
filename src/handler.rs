//! 请求处理器
//!
//! 实现 /v1/messages 端点的完整处理管线：
//! 认证 → thinking 优化 → 模型映射 → 格式转换 → 转发 → 响应转换

use crate::{auth, error::ProxyError, server::AppState, streaming, thinking, transform, usage};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// 健康检查
pub async fn health_check() -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
}

/// 处理 /v1/messages 请求
///
/// 完整管线：
/// 1. 认证校验
/// 2. thinking 优化（可选）
/// 3. 模型映射
/// 4. Anthropic → OpenAI 格式转换
/// 5. 转发到上游
/// 6. 响应转换（流式 / 非流式）
pub async fn handle_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<axum::response::Response, ProxyError> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4().to_string();

    // 1. 认证校验
    auth::validate_auth(&headers, &state.db, &state.config.auth_token)?;

    let mut body = body;

    // 2. thinking 优化器（可选）
    thinking::optimize(&mut body, state.config.features.thinking_optimizer);

    // 3. 提取模型名（直接透传，不做映射）
    let is_stream = body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    let request_model = body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown")
        .to_string();
    let actual_model = request_model.clone();

    log::info!(
        "[cc-proxy] {} 请求: model={}, stream={}, rid={}",
        if is_stream { "流式" } else { "非流式" },
        actual_model,
        is_stream,
        request_id
    );

    // 4. Anthropic → OpenAI 格式转换
    let openai_body = transform::anthropic_to_openai(body, None)?;

    // 5. 选取 API Key（轮询）+ 获取上游 URL
    let (key_id, api_key) = state.key_pool.next_key().await?;
    let upstream_base = state.key_pool.get_upstream_url().await?;

    // 6. 构建上游请求
    let upstream_url = format!(
        "{}/v1/chat/completions",
        upstream_base.trim_end_matches('/')
    );

    let req = state
        .http_client
        .post(&upstream_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&openai_body);

    // 7. 发送请求
    let resp = req.send().await.map_err(|e| {
        log::error!("[cc-proxy] 转发失败: {e}");
        // 上报密钥使用失败
        if let Some(ref kid) = key_id {
            let pool = state.key_pool.clone();
            let kid = kid.clone();
            tokio::spawn(async move { pool.report_result(&kid, false).await });
        }
        if e.is_timeout() {
            ProxyError::Timeout(format!("上游请求超时: {e}"))
        } else {
            ProxyError::ForwardFailed(format!("上游请求失败: {e}"))
        }
    })?;

    let status = resp.status();

    // 处理上游错误
    if !status.is_success() {
        let status_code = status.as_u16();
        let body_text = resp.text().await.ok();
        let latency_ms = start_time.elapsed().as_millis() as u64;

        // 上报密钥使用失败
        if let Some(ref kid) = key_id {
            let pool = state.key_pool.clone();
            let kid = kid.clone();
            tokio::spawn(async move { pool.report_result(&kid, false).await });
        }

        log::warn!(
            "[cc-proxy] 上游错误: status={}, model={}, body={:?}",
            status_code,
            request_model,
            body_text.as_deref().unwrap_or("(empty)").chars().take(200).collect::<String>()
        );

        // 记录错误请求
        let db = Arc::clone(&state.db);
        let err_model = actual_model.clone();
        let err_req_model = if request_model.is_empty() { None } else { Some(request_model.clone()) };
        let err_msg = body_text.as_deref().map(|s| s.chars().take(500).collect::<String>());
        let rid = request_id.clone();
        tokio::spawn(async move {
            usage::record_request(
                db,
                rid,
                err_model,
                err_req_model,
                usage::TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                },
                latency_ms,
                None,
                status_code,
                is_stream,
                err_msg,
            )
            .await;
        });

        return Err(ProxyError::UpstreamError {
            status: status_code,
            body: body_text,
        });
    }

    // 8. 响应转换
    if is_stream {
        // 流式请求：记录基本信息，token 使用量暂记为 0
        let latency_ms = start_time.elapsed().as_millis() as u64;
        let db = Arc::clone(&state.db);
        let stream_model = actual_model.clone();
        let stream_req_model = if request_model.is_empty() { None } else { Some(request_model.clone()) };
        let rid = request_id.clone();
        let response = handle_streaming_response(resp).await;

        // 上报密钥使用成功
        if let Some(ref kid) = key_id {
            let pool = state.key_pool.clone();
            let kid = kid.clone();
            tokio::spawn(async move { pool.report_result(&kid, true).await });
        }

        tokio::spawn(async move {
            usage::record_request(
                db,
                rid,
                stream_model,
                stream_req_model,
                usage::TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                },
                latency_ms,
                None,
                200,
                true,
                None,
            )
            .await;
        });

        response
    } else {
        // 上报密钥使用成功（非流式）
        if let Some(ref kid) = key_id {
            let pool = state.key_pool.clone();
            let kid = kid.clone();
            tokio::spawn(async move { pool.report_result(&kid, true).await });
        }

        handle_non_streaming_response(
            resp,
            Arc::clone(&state.db),
            request_id,
            actual_model,
            if request_model.is_empty() { None } else { Some(request_model) },
            start_time,
        )
        .await
    }
}

/// 处理流式响应
async fn handle_streaming_response(
    resp: reqwest::Response,
) -> Result<axum::response::Response, ProxyError> {
    let stream = resp.bytes_stream();
    let sse_stream = streaming::create_anthropic_sse_stream(stream);

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "Content-Type",
        axum::http::HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(
        "Cache-Control",
        axum::http::HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        "Connection",
        axum::http::HeaderValue::from_static("keep-alive"),
    );

    let body = axum::body::Body::from_stream(sse_stream);
    Ok((headers, body).into_response())
}

/// 处理非流式响应（含使用量记录）
async fn handle_non_streaming_response(
    resp: reqwest::Response,
    db: Arc<crate::database::Database>,
    request_id: String,
    model: String,
    request_model: Option<String>,
    start_time: std::time::Instant,
) -> Result<axum::response::Response, ProxyError> {
    let body_bytes = resp.bytes().await.map_err(|e| {
        log::error!("[cc-proxy] 读取上游响应失败: {e}");
        ProxyError::ForwardFailed(format!("读取上游响应失败: {e}"))
    })?;

    let upstream_response: Value = serde_json::from_slice(&body_bytes).map_err(|e| {
        log::error!(
            "[cc-proxy] 解析上游响应失败: {e}, body: {}",
            String::from_utf8_lossy(&body_bytes).chars().take(500).collect::<String>()
        );
        ProxyError::TransformError(format!("Failed to parse upstream response: {e}"))
    })?;

    let anthropic_response = transform::openai_to_anthropic(upstream_response).map_err(|e| {
        log::error!("[cc-proxy] 转换响应失败: {e}");
        e
    })?;

    // 提取使用量并异步记录
    let latency_ms = start_time.elapsed().as_millis() as u64;
    let token_usage = usage::extract_usage_from_anthropic_response(&anthropic_response);

    if let Some(u) = token_usage {
        let rid = request_id;
        let m = model;
        let rm = request_model;
        tokio::spawn(async move {
            usage::record_request(db, rid, m, rm, u, latency_ms, None, 200, false, None).await;
        });
    }

    Ok(Json(anthropic_response).into_response())
}
