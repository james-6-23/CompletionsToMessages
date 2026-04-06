//! 请求处理器
//!
//! 实现 /v1/messages 端点的完整处理管线：
//! 认证 → thinking 优化 → 模型映射 → 格式转换 → 转发 → 响应转换

use crate::{
    auth,
    error::{ProxyError, UpstreamHeaders},
    prompt_cache,
    server::AppState,
    streaming, thinking, transform, usage,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use std::sync::{Arc, LazyLock};

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
    status >= 500 || status == 401 || status == 402 || status == 429 || status == 529
}

/// 预编译正则（避免每次调用都编译）
static CONTEXT_OVERFLOW_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(\d+)\s*\+\s*(\d+)\s*>\s*(\d+)").unwrap()
});

/// 解析上下文窗口溢出错误: "input length and `max_tokens` exceed context limit: X + Y > Z"
/// 返回 (input_tokens, max_tokens, context_limit)
fn parse_context_overflow(body: &str) -> Option<(u64, u64, u64)> {
    let caps = CONTEXT_OVERFLOW_RE.captures(body)?;
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
            if let Ok(hdr_name) =
                axum::http::header::HeaderName::from_bytes(key.as_str().as_bytes())
            {
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

    // 4. Anthropic → OpenAI 格式转换（system+tools 走缓存，保证字节级稳定以提升上游 prompt cache 命中率）
    let prefix_result = state.prompt_cache.get_or_convert(&body, |b| {
        (
            prompt_cache::convert_system_to_openai(b),
            prompt_cache::convert_tools_to_openai(b),
        )
    });

    if prefix_result.hit {
        log::debug!(
            "[cc-proxy] prompt cache 命中, key={}, rid={}",
            prefix_result.cache_key,
            request_id
        );
    }

    let mut openai_body = transform::anthropic_to_openai_with_cached_prefix(
        body,
        &prefix_result.system_messages,
        &prefix_result.tools,
        Some(&prefix_result.cache_key),
    )?;

    // 流式请求走独立路径：立即返回 SSE 响应，后台执行重试 + 流转发 + keepalive 心跳
    // 解决 Cloudflare 524 超时：重试等待和上游请求期间每 20 秒发送 SSE keepalive 注释
    if is_stream {
        return handle_stream_with_keepalive(
            state, openai_body, actual_model, request_model, request_id, start_time, matched_token,
        )
        .await;
    }

    // 5. 非流式请求：发送上游请求（含重试逻辑）
    //
    // 遇到可重试错误（5xx / 401 / 402 / 429 / 529 / 网络错误）时重试，
    // 每次重试选取新的 API Key，指数退避 500ms → 1000ms。
    let mut max_attempts: usize = 5;
    let backoff_base_ms: u64 = 500;

    let mut last_error: Option<ProxyError> = None;
    let mut last_retry_after_ms: Option<u64> = None;
    let mut last_key_id: Option<String> = None;
    let mut last_channel_id: String = String::new();
    let mut resp_opt: Option<reqwest::Response> = None;
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

        // 每次尝试都选取新密钥（按请求模型筛选渠道）
        let (key_id, api_key, upstream_base, channel_id, proxy_url, mapped_model, ep_max_failures, ep_max_retries, ep_strip_tools) = state
            .key_pool
            .next_key(token_for_pool, Some(&actual_model))
            .await?;

        last_key_id = key_id.clone();
        last_channel_id = channel_id.clone();

        // 首次选 key 时，应用端点自定义的 max_retries
        if attempt == 0 && ep_max_retries > 0 {
            max_attempts = ep_max_retries as usize;
        }

        // 应用模型映射：将请求中的模型名替换为映射后的模型名
        if let Some(ref mapped) = mapped_model {
            openai_body["model"] = json!(mapped);
        }

        // 剥离 tools：不兼容 function calling 的上游
        if ep_strip_tools {
            if let Some(obj) = openai_body.as_object_mut() {
                obj.remove("tools");
                obj.remove("tool_choice");
            }
        }

        let upstream_url = format!(
            "{}/v1/chat/completions",
            upstream_base.trim_end_matches('/')
        );

        // 选择 HTTP 客户端：有代理则使用/缓存代理 client，否则用默认 client
        let client = if proxy_url.is_empty() {
            state.http_client.clone()
        } else {
            state
                .proxy_clients
                .entry(proxy_url.clone())
                .or_insert_with(|| {
                    let proxy = reqwest::Proxy::all(&proxy_url).expect("invalid proxy url");
                    reqwest::Client::builder()
                        .proxy(proxy)
                        .timeout(std::time::Duration::from_secs(
                            state.config.timeouts.request_timeout_secs,
                        ))
                        .pool_max_idle_per_host(16)
                        .build()
                        .expect("failed to build proxy client")
                })
                .clone()
        };

        let req = client
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
                    let mf = ep_max_failures;
                    tokio::spawn(async move { pool.report_result(&kid, false, None, mf).await });
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
                        let mf = ep_max_failures;
                        tokio::spawn(
                            async move { pool.report_result(&kid, false, Some(sc), mf).await },
                        );
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
                            body_text
                                .as_deref()
                                .unwrap_or("(empty)")
                                .chars()
                                .take(200)
                                .collect::<String>()
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
                            if let Some((input_tokens, _max_tokens, context_limit)) =
                                parse_context_overflow(body_str)
                            {
                                let new_max = context_limit
                                    .saturating_sub(input_tokens)
                                    .saturating_sub(1000);
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
                        body_text
                            .as_deref()
                            .unwrap_or("(empty)")
                            .chars()
                            .take(200)
                            .collect::<String>()
                    );

                    // 记录错误请求（跳过可重试状态码的失败记录，避免全 0 噪声日志）
                    if !is_retryable_status(status_code) {
                    let db = Arc::clone(&state.db);
                    let err_model = actual_model.clone();
                    let err_req_model = if request_model.is_empty() {
                        None
                    } else {
                        Some(request_model.clone())
                    };
                    let err_msg = body_text
                        .as_deref()
                        .map(|s| s.chars().take(500).collect::<String>());
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
                    }

                    return Err(ProxyError::UpstreamError {
                        status: status_code,
                        body: body_text,
                        upstream_headers: Some(UpstreamHeaders(extracted_headers)),
                    });
                }

                // 成功响应
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
        tokio::spawn(async move { pool.report_result(&kid, true, Some(200), 0).await });
    }
    if let Some(ref t) = matched_token {
        let pool = state.key_pool.clone();
        let t = t.clone();
        tokio::spawn(async move { pool.report_access_token(&t, true).await });
    }

    // 7. 非流式响应转换（流式请求已在上方提前返回）
    handle_non_streaming_response(
        resp,
        Arc::clone(&state.db),
        request_id,
        actual_model,
        if request_model.is_empty() {
            None
        } else {
            Some(request_model)
        },
        start_time,
        last_channel_id,
        last_key_id.clone().unwrap_or_default(),
    )
    .await
}

/// 发送 SSE 错误事件到客户端流
async fn send_sse_error(
    tx: &tokio::sync::mpsc::Sender<Result<bytes::Bytes, std::io::Error>>,
    message: &str,
) {
    let error_event = json!({
        "type": "error",
        "error": {"type": "api_error", "message": message}
    });
    let sse_data = format!(
        "event: error\ndata: {}\n\n",
        serde_json::to_string(&error_event).unwrap_or_default()
    );
    let _ = tx.send(Ok(bytes::Bytes::from(sse_data))).await;
}

/// 流式请求处理：立即返回 SSE 响应，后台执行重试 + 流转发 + keepalive 心跳
///
/// 解决 Cloudflare 524 超时：重试等待和上游请求期间每 20 秒发送 SSE keepalive 注释，
/// 确保 Cloudflare 在 100 秒超时窗口内持续收到数据。
async fn handle_stream_with_keepalive(
    state: AppState,
    mut openai_body: Value,
    actual_model: String,
    request_model: String,
    request_id: String,
    start_time: std::time::Instant,
    matched_token: Option<String>,
) -> Result<axum::response::Response, ProxyError> {
    use bytes::Bytes;
    use futures::StreamExt;

    let (data_tx, data_rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);
    let stream_max_duration = state.config.timeouts.stream_max_duration_secs;

    // 后台任务：重试循环 + keepalive 心跳 + SSE 流转发 + usage 记录
    tokio::spawn(async move {
        let keepalive = Bytes::from(": keepalive\n\n");
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(20));
        ticker.tick().await; // 跳过首次立即触发

        let mut max_attempts: usize = 5;
        let backoff_base_ms: u64 = 500;
        let mut last_error_msg: Option<String> = None;
        let mut last_retry_after_ms: Option<u64> = None;
        let mut last_key_id: Option<String> = None;
        let mut last_channel_id = String::new();
        let mut context_overflow_retried = false;
        let token_for_pool = matched_token.as_deref().unwrap_or("").to_string();

        let mut attempt: usize = 0;
        let mut success_resp: Option<reqwest::Response> = None;

        // === 重试循环（期间持续发送 keepalive 防止 Cloudflare 524） ===
        loop {
            if attempt >= max_attempts {
                break;
            }

            if attempt > 0 {
                let delay_ms = if let Some(ra) = last_retry_after_ms.take() {
                    ra.min(30_000)
                } else {
                    backoff_base_ms * (1 << (attempt - 1))
                };
                log::info!(
                    "[cc-proxy] 流式重试第 {} 次 (延迟 {}ms), rid={}",
                    attempt,
                    delay_ms,
                    request_id
                );

                // 延迟期间持续发送 keepalive
                let deadline =
                    tokio::time::Instant::now() + std::time::Duration::from_millis(delay_ms);
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        _ = ticker.tick() => {
                            if data_tx.send(Ok(keepalive.clone())).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }

            // 每次尝试选取新密钥
            let key_result = state
                .key_pool
                .next_key(&token_for_pool, Some(&actual_model))
                .await;
            let (
                key_id,
                api_key,
                upstream_base,
                channel_id,
                proxy_url,
                mapped_model,
                ep_max_failures,
                ep_max_retries,
                ep_strip_tools,
            ) = match key_result {
                Ok(k) => k,
                Err(e) => {
                    send_sse_error(&data_tx, &format!("{e}")).await;
                    return;
                }
            };

            last_key_id = key_id.clone();
            last_channel_id = channel_id.clone();

            if attempt == 0 && ep_max_retries > 0 {
                max_attempts = ep_max_retries as usize;
            }

            if let Some(ref mapped) = mapped_model {
                openai_body["model"] = json!(mapped);
            }

            if ep_strip_tools {
                if let Some(obj) = openai_body.as_object_mut() {
                    obj.remove("tools");
                    obj.remove("tool_choice");
                }
            }

            let upstream_url = format!(
                "{}/v1/chat/completions",
                upstream_base.trim_end_matches('/')
            );

            let client = if proxy_url.is_empty() {
                state.http_client.clone()
            } else {
                state
                    .proxy_clients
                    .entry(proxy_url.clone())
                    .or_insert_with(|| {
                        let proxy = reqwest::Proxy::all(&proxy_url).expect("invalid proxy url");
                        reqwest::Client::builder()
                            .proxy(proxy)
                            .timeout(std::time::Duration::from_secs(
                                state.config.timeouts.request_timeout_secs,
                            ))
                            .pool_max_idle_per_host(16)
                            .build()
                            .expect("failed to build proxy client")
                    })
                    .clone()
            };

            let req = client
                .post(&upstream_url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&openai_body);

            // 发送请求，期间持续 keepalive
            let send_fut = req.send();
            tokio::pin!(send_fut);
            let send_result = loop {
                tokio::select! {
                    result = &mut send_fut => break result,
                    _ = ticker.tick() => {
                        if data_tx.send(Ok(keepalive.clone())).await.is_err() {
                            return;
                        }
                    }
                }
            };

            match send_result {
                Err(e) => {
                    log::error!(
                        "[cc-proxy] 流式转发失败 (attempt {}): {e}",
                        attempt + 1
                    );
                    if let Some(ref kid) = key_id {
                        let pool = state.key_pool.clone();
                        let kid = kid.clone();
                        let mf = ep_max_failures;
                        tokio::spawn(
                            async move { pool.report_result(&kid, false, None, mf).await },
                        );
                    }
                    if let Some(ref t) = matched_token {
                        let pool = state.key_pool.clone();
                        let t = t.clone();
                        tokio::spawn(async move { pool.report_access_token(&t, false).await });
                    }

                    if attempt + 1 < max_attempts {
                        last_error_msg = Some(format!("{e}"));
                        attempt += 1;
                        continue;
                    }
                    send_sse_error(&data_tx, &format!("上游请求失败: {e}")).await;
                    return;
                }
                Ok(resp) => {
                    let status = resp.status();

                    if !status.is_success() {
                        let status_code = status.as_u16();
                        let extracted_headers = extract_upstream_headers(resp.headers());

                        if let Some(ref kid) = key_id {
                            let pool = state.key_pool.clone();
                            let kid = kid.clone();
                            let sc = status_code;
                            let mf = ep_max_failures;
                            tokio::spawn(async move {
                                pool.report_result(&kid, false, Some(sc), mf).await
                            });
                        }
                        if let Some(ref t) = matched_token {
                            let pool = state.key_pool.clone();
                            let t = t.clone();
                            tokio::spawn(
                                async move { pool.report_access_token(&t, false).await },
                            );
                        }

                        // 可重试错误
                        if is_retryable_status(status_code) && attempt + 1 < max_attempts {
                            last_retry_after_ms = extracted_headers
                                .get("retry-after")
                                .and_then(|v| v.to_str().ok())
                                .and_then(|s| s.parse::<u64>().ok())
                                .map(|secs| secs * 1000);

                            let body_text = resp.text().await.ok();
                            log::warn!(
                                "[cc-proxy] 流式上游可重试错误 (attempt {}): status={}, body={:?}",
                                attempt + 1,
                                status_code,
                                body_text
                                    .as_deref()
                                    .unwrap_or("(empty)")
                                    .chars()
                                    .take(200)
                                    .collect::<String>()
                            );
                            last_error_msg = Some(format!("status={}", status_code));
                            attempt += 1;
                            continue;
                        }

                        // 不可重试或已耗尽
                        let body_text = resp.text().await.ok();

                        // 上下文窗口溢出自动修正
                        if status_code == 400 && !context_overflow_retried {
                            if let Some(body_str) = body_text.as_deref() {
                                if let Some((input_tokens, _max_tokens, context_limit)) =
                                    parse_context_overflow(body_str)
                                {
                                    let new_max = context_limit
                                        .saturating_sub(input_tokens)
                                        .saturating_sub(1000);
                                    if new_max >= 1000 {
                                        context_overflow_retried = true;
                                        log::warn!(
                                            "[cc-proxy] 上下文溢出自动修正: input={}, context_limit={}, 新max_tokens={}, rid={}",
                                            input_tokens, context_limit, new_max, request_id
                                        );
                                        if openai_body.get("max_completion_tokens").is_some() {
                                            openai_body["max_completion_tokens"] = json!(new_max);
                                        } else {
                                            openai_body["max_tokens"] = json!(new_max);
                                        }
                                        continue;
                                    }
                                }
                            }
                        }

                        // 记录错误请求
                        let latency_ms = start_time.elapsed().as_millis() as u64;
                        if !is_retryable_status(status_code) {
                            let db = Arc::clone(&state.db);
                            let err_model = actual_model.clone();
                            let err_req_model = if request_model.is_empty() {
                                None
                            } else {
                                Some(request_model.clone())
                            };
                            let err_msg = body_text
                                .as_deref()
                                .map(|s| s.chars().take(500).collect::<String>());
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
                                    true,
                                    err_msg,
                                    err_channel_id,
                                    err_key_id,
                                )
                                .await;
                            });
                        }

                        let error_msg = body_text
                            .as_deref()
                            .unwrap_or("上游错误")
                            .chars()
                            .take(500)
                            .collect::<String>();
                        send_sse_error(&data_tx, &error_msg).await;
                        return;
                    }

                    // 成功响应
                    success_resp = Some(resp);
                    break;
                }
            }
        }

        // 所有重试耗尽仍无成功响应
        let resp = match success_resp {
            Some(r) => r,
            None => {
                send_sse_error(
                    &data_tx,
                    &format!(
                        "所有重试均失败: {}",
                        last_error_msg.unwrap_or_default()
                    ),
                )
                .await;
                return;
            }
        };

        // 上报密钥使用成功
        if let Some(ref kid) = last_key_id {
            let pool = state.key_pool.clone();
            let kid = kid.clone();
            tokio::spawn(async move { pool.report_result(&kid, true, Some(200), 0).await });
        }
        if let Some(ref t) = matched_token {
            let pool = state.key_pool.clone();
            let t = t.clone();
            tokio::spawn(async move { pool.report_access_token(&t, true).await });
        }

        // 转发上游 SSE 流
        let usage_collector = streaming::new_usage_collector();
        let (done_tx, _done_rx) = streaming::new_done_signal();
        let uc_for_record = usage_collector.clone();

        let sse_stream = streaming::create_anthropic_sse_stream(
            resp.bytes_stream(),
            usage_collector,
            Some(done_tx),
            stream_max_duration,
        );
        tokio::pin!(sse_stream);

        while let Some(item) = sse_stream.next().await {
            if data_tx.send(item).await.is_err() {
                break; // 客户端已断开
            }
        }

        // 流结束，记录 usage
        let latency_ms = start_time.elapsed().as_millis() as u64;
        let collected = uc_for_record
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        usage::record_request(
            state.db.clone(),
            request_id,
            actual_model,
            if request_model.is_empty() {
                None
            } else {
                Some(request_model)
            },
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
            last_channel_id,
            last_key_id.unwrap_or_default(),
        )
        .await;
    });

    // 立即返回 SSE 响应（数据由后台任务通过 channel 推送）
    let out_stream = async_stream::stream! {
        let mut rx = data_rx;
        while let Some(item) = rx.recv().await {
            yield item;
        }
    };

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

    let body = axum::body::Body::from_stream(out_stream);
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
            String::from_utf8_lossy(&body_bytes)
                .chars()
                .take(500)
                .collect::<String>()
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
            usage::record_request(
                db, rid, m, rm, u, latency_ms, None, 200, false, None, channel_id, key_id,
            )
            .await;
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

    // 选取一个可用 key 获取上游 URL（模型列表请求不按模型筛选）
    let (_key_id, api_key, upstream_base, _channel_id, _proxy_url, _mapped, _mf, _mr, _st) =
        state.key_pool.next_key(token_for_pool, None).await?;

    let models_url = format!("{}/v1/models", upstream_base.trim_end_matches('/'));

    let resp = state
        .http_client
        .get(&models_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| ProxyError::ForwardFailed(format!("获取模型列表失败: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_bytes = resp.bytes().await.unwrap_or_default();
        return Ok((
            axum::http::StatusCode::from_u16(status.as_u16())
                .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body_bytes,
        )
            .into_response());
    }

    // 解析并过滤，只保留 Claude/Anthropic 模型
    let body: Value = resp.json().await.unwrap_or(json!({"data": []}));
    let filtered_data: Vec<&Value> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|m| {
                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    is_claude_model(id)
                })
                .collect()
        })
        .unwrap_or_default();

    let result = json!({
        "object": "list",
        "data": filtered_data,
    });

    Ok(Json(result).into_response())
}

/// 判断模型 ID 是否为 Claude 系列
pub fn is_claude_model(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
    id.contains("claude") || id.starts_with("anthropic/") || id.starts_with("anthropic:")
}
