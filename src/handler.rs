//! 请求处理器
//!
//! 实现 /v1/messages 端点的完整处理管线：
//! 认证 → thinking 优化 → 模型映射 → 格式转换 → 转发 → 响应转换

use crate::{auth, error::{ProxyError, UpstreamHeaders}, server::AppState, streaming, thinking, transform, usage};
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

/// 判断 HTTP 状态码是否可重试
fn is_retryable_status(status: u16) -> bool {
    status >= 500 || status == 429 || status == 529
}

/// 解析上下文窗口溢出错误: "input length and `max_tokens` exceed context limit: X + Y > Z"
/// 返回 (input_tokens, max_tokens, context_limit)
fn parse_context_overflow(body: &str) -> Option<(u64, u64, u64)> {
    let re = regex::Regex::new(r"(\d+)\s*\+\s*(\d+)\s*>\s*(\d+)").ok()?;
    let caps = re.captures(body)?;
    let input = caps.get(1)?.as_str().parse::<u64>().ok()?;
    let max = caps.get(2)?.as_str().parse::<u64>().ok()?;
    let limit = caps.get(3)?.as_str().parse::<u64>().ok()?;
    Some((input, max, limit))
}

/// 从上游响应头中提取需要透传给客户端的关键头部
fn extract_upstream_headers(resp_headers: &reqwest::header::HeaderMap) -> axum::http::HeaderMap {
    let mut headers = axum::http::HeaderMap::new();
    let passthrough_keys = ["retry-after", "x-should-retry", "request-id"];

    for key_name in &passthrough_keys {
        if let Some(val) = resp_headers.get(*key_name) {
            if let Ok(hdr_name) = axum::http::header::HeaderName::from_bytes(key_name.as_bytes()) {
                if let Ok(hdr_val) = axum::http::header::HeaderValue::from_bytes(val.as_bytes()) {
                    headers.insert(hdr_name, hdr_val);
                }
            }
        }
    }

    // 透传所有 anthropic-ratelimit-* 头
    for (key, val) in resp_headers.iter() {
        if key.as_str().starts_with("anthropic-ratelimit-") {
            if let Ok(hdr_name) = axum::http::header::HeaderName::from_bytes(key.as_str().as_bytes()) {
                if let Ok(hdr_val) = axum::http::header::HeaderValue::from_bytes(val.as_bytes()) {
                    headers.insert(hdr_name, hdr_val);
                }
            }
        }
    }

    headers
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

    // 1. 认证校验 — 返回匹配到的 access token 值（None 表示开发模式免认证）
    let matched_token = auth::validate_auth(&headers, &state.db, &state.config.auth_token)?;

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
    let mut openai_body = transform::anthropic_to_openai(body, None)?;

    // 5. 发送上游请求（含重试逻辑）
    //
    // 对非流式请求：遇到可重试错误（5xx / 429 / 529 / 网络错误）时最多重试 2 次，
    //               每次重试选取新的 API Key，指数退避 500ms → 1000ms。
    // 对流式请求：不重试（流一旦开始无法回滚）。
    let max_attempts = if is_stream { 1 } else { 3 };
    let backoff_base_ms: u64 = 500;

    let mut last_error: Option<ProxyError> = None;
    let mut last_retry_after_ms: Option<u64> = None;
    let mut last_key_id: Option<String> = None;
    let mut last_channel_id: String = String::new();
    let mut resp_opt: Option<reqwest::Response> = None;
    let mut upstream_headers_for_resp = axum::http::HeaderMap::new();
    // 上下文溢出自动修正标记（额外重试一次，独立于常规重试循环）
    let mut context_overflow_retried = false;

    let token_for_pool = matched_token.as_deref().unwrap_or("");

    let mut attempt: usize = 0;
    loop {
        if attempt >= max_attempts {
            break;
        }

        if attempt > 0 {
            // 优先使用上游 retry-after 头指定的延迟（上限 30s），否则指数退避
            let delay_ms = if let Some(retry_after) = last_retry_after_ms.take() {
                retry_after.min(30_000)
            } else {
                backoff_base_ms * (1 << (attempt - 1)) // 500ms, 1000ms
            };
            log::info!(
                "[cc-proxy] 重试第 {} 次 (延迟 {}ms), rid={}",
                attempt,
                delay_ms,
                request_id
            );
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }

        // 每次尝试都选取新密钥
        let (key_id, api_key, upstream_base, channel_id) =
            state.key_pool.next_key(token_for_pool).await?;

        last_key_id = key_id.clone();
        last_channel_id = channel_id.clone();

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

        // 发送请求
        let send_result = req.send().await;

        match send_result {
            Err(e) => {
                log::error!("[cc-proxy] 转发失败 (attempt {}): {e}", attempt + 1);

                // 上报密钥使用失败（网络错误，status_code = None）
                if let Some(ref kid) = key_id {
                    let pool = state.key_pool.clone();
                    let kid = kid.clone();
                    tokio::spawn(async move { pool.report_result(&kid, false, None).await });
                }
                if let Some(ref t) = matched_token {
                    let pool = state.key_pool.clone();
                    let t = t.clone();
                    tokio::spawn(async move { pool.report_access_token(&t, false).await });
                }

                let err = if e.is_timeout() {
                    ProxyError::Timeout(format!("上游请求超时: {e}"))
                } else {
                    ProxyError::ForwardFailed(format!("上游请求失败: {e}"))
                };

                // 网络错误可重试
                if attempt + 1 < max_attempts {
                    last_error = Some(err);
                    attempt += 1;
                    continue;
                }
                return Err(err);
            }
            Ok(resp) => {
                let status = resp.status();

                if !status.is_success() {
                    let status_code = status.as_u16();
                    let extracted_headers = extract_upstream_headers(resp.headers());

                    // 上报密钥使用失败（带状态码）
                    if let Some(ref kid) = key_id {
                        let pool = state.key_pool.clone();
                        let kid = kid.clone();
                        let sc = status_code;
                        tokio::spawn(async move { pool.report_result(&kid, false, Some(sc)).await });
                    }
                    if let Some(ref t) = matched_token {
                        let pool = state.key_pool.clone();
                        let t = t.clone();
                        tokio::spawn(async move { pool.report_access_token(&t, false).await });
                    }

                    // 检查是否可重试（常规重试逻辑）
                    if is_retryable_status(status_code) && attempt + 1 < max_attempts {
                        // 解析 retry-after 头，用于下次重试的延迟
                        let retry_after_ms = extracted_headers
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok())
                            .map(|secs| secs * 1000);
                        last_retry_after_ms = retry_after_ms;

                        let body_text = resp.text().await.ok();
                        log::warn!(
                            "[cc-proxy] 上游可重试错误 (attempt {}): status={}, body={:?}",
                            attempt + 1,
                            status_code,
                            body_text.as_deref().unwrap_or("(empty)").chars().take(200).collect::<String>()
                        );
                        last_error = Some(ProxyError::UpstreamError {
                            status: status_code,
                            body: body_text,
                            upstream_headers: Some(UpstreamHeaders(extracted_headers)),
                        });
                        attempt += 1;
                        continue;
                    }

                    // 不可重试或已耗尽重试次数
                    let body_text = resp.text().await.ok();

                    // 上下文窗口溢出自动修正 max_tokens（额外重试一次，独立于常规重试）
                    if status_code == 400 && !context_overflow_retried {
                        if let Some(body_str) = body_text.as_deref() {
                            if let Some((input_tokens, _max_tokens, context_limit)) = parse_context_overflow(body_str) {
                                let new_max = context_limit.saturating_sub(input_tokens).saturating_sub(1000);
                                if new_max >= 1000 {
                                    context_overflow_retried = true;
                                    log::warn!(
                                        "[cc-proxy] 上下文溢出自动修正: input={}, context_limit={}, 新max_tokens={}, rid={}",
                                        input_tokens, context_limit, new_max, request_id
                                    );
                                    // 修正 openai_body 中的 max_tokens / max_completion_tokens
                                    if openai_body.get("max_completion_tokens").is_some() {
                                        openai_body["max_completion_tokens"] = json!(new_max);
                                    } else {
                                        openai_body["max_tokens"] = json!(new_max);
                                    }
                                    // 不增加 attempt 计数，这是额外重试机会
                                    continue;
                                }
                            }
                        }
                    }

                    let latency_ms = start_time.elapsed().as_millis() as u64;

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
                    let err_channel_id = channel_id.clone();
                    let err_key_id = key_id.clone().unwrap_or_default();
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
                            err_channel_id,
                            err_key_id,
                        )
                        .await;
                    });

                    return Err(ProxyError::UpstreamError {
                        status: status_code,
                        body: body_text,
                        upstream_headers: Some(UpstreamHeaders(extracted_headers)),
                    });
                }

                // 成功响应 — 提取上游头部，跳出重试循环
                upstream_headers_for_resp = extract_upstream_headers(resp.headers());
                resp_opt = Some(resp);
                break;
            }
        }
    }

    // 所有重试耗尽仍无成功响应
    let resp = match resp_opt {
        Some(r) => r,
        None => {
            return Err(last_error.unwrap_or_else(|| {
                ProxyError::Internal("所有重试均失败，无有效响应".to_string())
            }));
        }
    };

    // 6. 上报密钥使用成功
    if let Some(ref kid) = last_key_id {
        let pool = state.key_pool.clone();
        let kid = kid.clone();
        tokio::spawn(async move { pool.report_result(&kid, true, Some(200)).await });
    }
    if let Some(ref t) = matched_token {
        let pool = state.key_pool.clone();
        let t = t.clone();
        tokio::spawn(async move { pool.report_access_token(&t, true).await });
    }

    // 7. 响应转换
    if is_stream {
        let db = Arc::clone(&state.db);
        let stream_model = actual_model.clone();
        let stream_req_model = if request_model.is_empty() { None } else { Some(request_model.clone()) };
        let rid = request_id.clone();
        let start = start_time;

        // 创建 usage 收集器和 done 信号
        let usage_collector = streaming::new_usage_collector();
        let (done_tx, done_rx) = streaming::new_done_signal();

        let response = handle_streaming_response(
            resp,
            usage_collector.clone(),
            Some(done_tx),
            upstream_headers_for_resp,
        ).await;

        // 延迟记录 usage：等流传输完毕（done 信号）后从 collector 读取实际值
        let stream_channel_id = last_channel_id.clone();
        let stream_key_id = last_key_id.clone().unwrap_or_default();
        tokio::spawn(async move {
            // 等待流结束信号，最多 5 分钟超时兜底
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                done_rx,
            ).await;
            let latency_ms = start.elapsed().as_millis() as u64;
            let collected = usage_collector.lock().map(|c| c.clone()).unwrap_or_default();
            usage::record_request(
                db,
                rid,
                stream_model,
                stream_req_model,
                usage::TokenUsage {
                    input_tokens: collected.input_tokens,
                    output_tokens: collected.output_tokens,
                    cache_read_tokens: collected.cache_read_tokens,
                    cache_creation_tokens: collected.cache_creation_tokens,
                },
                latency_ms,
                collected.first_token_ms,
                200,
                true,
                None,
                stream_channel_id,
                stream_key_id,
            )
            .await;
        });

        response
    } else {
        handle_non_streaming_response(
            resp,
            Arc::clone(&state.db),
            request_id,
            actual_model,
            if request_model.is_empty() { None } else { Some(request_model) },
            start_time,
            last_channel_id,
            last_key_id.clone().unwrap_or_default(),
        )
        .await
    }
}

/// 处理流式响应
async fn handle_streaming_response(
    resp: reqwest::Response,
    usage_collector: streaming::StreamUsageCollector,
    done_signal: Option<tokio::sync::oneshot::Sender<()>>,
    upstream_headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ProxyError> {
    let stream = resp.bytes_stream();
    let sse_stream = streaming::create_anthropic_sse_stream(stream, usage_collector, done_signal);

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
    // 透传上游响应头到流式响应
    for (key, value) in upstream_headers.iter() {
        headers.insert(key.clone(), value.clone());
    }

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
    channel_id: String,
    key_id: String,
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
            usage::record_request(db, rid, m, rm, u, latency_ms, None, 200, false, None, channel_id, key_id).await;
        });
    }

    Ok(Json(anthropic_response).into_response())
}

/// 处理 /v1/models 请求 — 透传上游模型列表
pub async fn handle_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<axum::response::Response, ProxyError> {
    // 认证校验（与 /v1/messages 一致）
    let matched_token = auth::validate_auth(&headers, &state.db, &state.config.auth_token)?;
    let token_for_pool = matched_token.as_deref().unwrap_or("");

    // 选取一个可用 key 获取上游 URL
    let (_key_id, api_key, upstream_base, _channel_id) =
        state.key_pool.next_key(token_for_pool).await?;

    let models_url = format!(
        "{}/v1/models",
        upstream_base.trim_end_matches('/')
    );

    let resp = state
        .http_client
        .get(&models_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| ProxyError::ForwardFailed(format!("获取模型列表失败: {e}")))?;

    let status = resp.status();
    let body_bytes = resp.bytes().await.map_err(|e| {
        ProxyError::ForwardFailed(format!("读取模型列表响应失败: {e}"))
    })?;

    Ok((
        axum::http::StatusCode::from_u16(status.as_u16()).unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        body_bytes,
    ).into_response())
}
