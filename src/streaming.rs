//! 流式响应转换模块
//!
//! 实现 OpenAI SSE → Anthropic SSE 格式转换
//! 来源: cc-switch 项目 (src-tauri/src/proxy/providers/streaming.rs)

use crate::sse::strip_sse_field;
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 流式响应中收集的 usage 信息
#[derive(Debug, Clone)]
pub struct StreamUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    /// 首个内容 token 的耗时（毫秒），从流开始时刻算起
    pub first_token_ms: Option<u64>,
    /// 流开始时刻（内部使用，不对外序列化）
    stream_start: Instant,
    /// 首次收到内容 delta 的标记（内部使用）
    first_content_received: bool,
}

impl Default for StreamUsage {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            first_token_ms: None,
            stream_start: Instant::now(),
            first_content_received: false,
        }
    }
}

/// 线程安全的 usage 收集器
pub type StreamUsageCollector = Arc<Mutex<StreamUsage>>;

/// 创建新的 usage 收集器
pub fn new_usage_collector() -> StreamUsageCollector {
    Arc::new(Mutex::new(StreamUsage::default()))
}

/// 创建 done 信号对，用于流结束时通知 usage 记录任务
pub fn new_done_signal() -> (tokio::sync::oneshot::Sender<()>, tokio::sync::oneshot::Receiver<()>) {
    tokio::sync::oneshot::channel()
}

/// OpenAI 流式响应数据结构
#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    id: String,
    model: String,
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeltaToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    call_type: Option<String>,
    #[serde(default)]
    function: Option<DeltaFunction>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// OpenAI 流式响应的 usage 信息
#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: u32,
}

#[derive(Debug, Clone)]
struct ToolBlockState {
    anthropic_index: u32,
    id: String,
    name: String,
    started: bool,
    pending_args: String,
}

/// 创建 Anthropic SSE 流，同时通过 `usage_collector` 收集 token 使用量
///
/// `done_signal`: 流结束（message_stop）时发送信号，通知 usage 记录任务可以读取 collector
pub fn create_anthropic_sse_stream<E: std::error::Error + Send + 'static>(
    stream: impl Stream<Item = Result<Bytes, E>> + Send + 'static,
    usage_collector: StreamUsageCollector,
    done_signal: Option<tokio::sync::oneshot::Sender<()>>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    async_stream::stream! {
        let mut done_tx = done_signal;
        let mut buffer = String::new();
        let mut message_id = None;
        let mut current_model = None;
        let mut next_content_index: u32 = 0;
        let mut has_sent_message_start = false;
        let mut current_non_tool_block_type: Option<&'static str> = None;
        let mut current_non_tool_block_index: Option<u32> = None;
        let mut tool_blocks_by_index: HashMap<usize, ToolBlockState> = HashMap::new();
        let mut open_tool_block_indices: HashSet<u32> = HashSet::new();

        tokio::pin!(stream);

        // 流式空闲超时：90 秒内无数据则中断
        let idle_timeout = std::time::Duration::from_secs(90);

        loop {
            let chunk = match tokio::time::timeout(idle_timeout, stream.next()).await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break, // 流正常结束
                Err(_) => {
                    // 空闲超时
                    log::warn!("[cc-proxy] 流式响应空闲超时 (90s)");
                    let error_event = json!({
                        "type": "error",
                        "error": {"type": "idle_timeout", "message": "Stream idle timeout (90s)"}
                    });
                    let sse_data = format!("event: error\ndata: {}\n\n",
                        serde_json::to_string(&error_event).unwrap_or_default());
                    yield Ok(Bytes::from(sse_data));
                    // 发送 done 信号，避免 usage 记录任务永远等待
                    if let Some(tx) = done_tx.take() {
                        let _ = tx.send(());
                    }
                    break;
                }
            };
            match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    buffer.push_str(&text);

                    while let Some(pos) = buffer.find("\n\n") {
                        let line = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();

                        if line.trim().is_empty() {
                            continue;
                        }

                        for l in line.lines() {
                            if let Some(data) = strip_sse_field(l, "data") {
                                if data.trim() == "[DONE]" {
                                    log::debug!("[cc-proxy] <<< OpenAI SSE: [DONE]");
                                    let event = json!({"type": "message_stop"});
                                    let sse_data = format!("event: message_stop\ndata: {}\n\n",
                                        serde_json::to_string(&event).unwrap_or_default());
                                    log::debug!("[cc-proxy] >>> Anthropic SSE: message_stop");
                                    yield Ok(Bytes::from(sse_data));
                                    // 流结束，发送 done 信号通知 usage 记录任务
                                    if let Some(tx) = done_tx.take() {
                                        let _ = tx.send(());
                                    }
                                    continue;
                                }

                                if let Ok(chunk) = serde_json::from_str::<OpenAIStreamChunk>(data) {
                                    log::debug!("[cc-proxy] <<< SSE chunk received");

                                    if message_id.is_none() {
                                        message_id = Some(chunk.id.clone());
                                    }
                                    if current_model.is_none() {
                                        current_model = Some(chunk.model.clone());
                                    }

                                    if let Some(choice) = chunk.choices.first() {
                                        if !has_sent_message_start {
                                            let mut start_usage = json!({
                                                "input_tokens": 0,
                                                "output_tokens": 0
                                            });
                                            if let Some(u) = &chunk.usage {
                                                start_usage["input_tokens"] = json!(u.prompt_tokens);
                                                if let Some(cached) = extract_cache_read_tokens(u) {
                                                    start_usage["cache_read_input_tokens"] = json!(cached);
                                                }
                                                if let Some(created) = u.cache_creation_input_tokens {
                                                    start_usage["cache_creation_input_tokens"] = json!(created);
                                                }
                                                // 收集初始 usage
                                                if let Ok(mut c) = usage_collector.lock() {
                                                    c.input_tokens = u.prompt_tokens;
                                                    c.cache_read_tokens = extract_cache_read_tokens(u).unwrap_or(0);
                                                    c.cache_creation_tokens = u.cache_creation_input_tokens.unwrap_or(0);
                                                }
                                            }

                                            let event = json!({
                                                "type": "message_start",
                                                "message": {
                                                    "id": message_id.clone().unwrap_or_default(),
                                                    "type": "message",
                                                    "role": "assistant",
                                                    "model": current_model.clone().unwrap_or_default(),
                                                    "usage": start_usage
                                                }
                                            });
                                            let sse_data = format!("event: message_start\ndata: {}\n\n",
                                                serde_json::to_string(&event).unwrap_or_default());
                                            yield Ok(Bytes::from(sse_data));
                                            has_sent_message_start = true;
                                        }

                                        // 处理 reasoning（thinking）— 记录首 token 时间
                                        if let Some(reasoning) = &choice.delta.reasoning {
                                            if !reasoning.is_empty() {
                                                if let Ok(mut c) = usage_collector.lock() {
                                                    if !c.first_content_received {
                                                        c.first_content_received = true;
                                                        c.first_token_ms = Some(c.stream_start.elapsed().as_millis() as u64);
                                                    }
                                                }
                                            }
                                            if current_non_tool_block_type != Some("thinking") {
                                                if let Some(index) = current_non_tool_block_index.take() {
                                                    let event = json!({
                                                        "type": "content_block_stop",
                                                        "index": index
                                                    });
                                                    let sse_data = format!("event: content_block_stop\ndata: {}\n\n",
                                                        serde_json::to_string(&event).unwrap_or_default());
                                                    yield Ok(Bytes::from(sse_data));
                                                }
                                                let index = next_content_index;
                                                next_content_index += 1;
                                                let event = json!({
                                                    "type": "content_block_start",
                                                    "index": index,
                                                    "content_block": {
                                                        "type": "thinking",
                                                        "thinking": ""
                                                    }
                                                });
                                                let sse_data = format!("event: content_block_start\ndata: {}\n\n",
                                                    serde_json::to_string(&event).unwrap_or_default());
                                                yield Ok(Bytes::from(sse_data));
                                                current_non_tool_block_type = Some("thinking");
                                                current_non_tool_block_index = Some(index);
                                            }

                                            if let Some(index) = current_non_tool_block_index {
                                                let event = json!({
                                                    "type": "content_block_delta",
                                                    "index": index,
                                                    "delta": {
                                                        "type": "thinking_delta",
                                                        "thinking": reasoning
                                                    }
                                                });
                                                let sse_data = format!("event: content_block_delta\ndata: {}\n\n",
                                                    serde_json::to_string(&event).unwrap_or_default());
                                                yield Ok(Bytes::from(sse_data));
                                            }
                                        }

                                        // 处理文本内容 — 记录首 token 时间
                                        if let Some(content) = &choice.delta.content {
                                            if !content.is_empty() {
                                                if let Ok(mut c) = usage_collector.lock() {
                                                    if !c.first_content_received {
                                                        c.first_content_received = true;
                                                        c.first_token_ms = Some(c.stream_start.elapsed().as_millis() as u64);
                                                    }
                                                }
                                                if current_non_tool_block_type != Some("text") {
                                                    if let Some(index) = current_non_tool_block_index.take() {
                                                        let event = json!({
                                                            "type": "content_block_stop",
                                                            "index": index
                                                        });
                                                        let sse_data = format!("event: content_block_stop\ndata: {}\n\n",
                                                            serde_json::to_string(&event).unwrap_or_default());
                                                        yield Ok(Bytes::from(sse_data));
                                                    }

                                                    let index = next_content_index;
                                                    next_content_index += 1;
                                                    let event = json!({
                                                        "type": "content_block_start",
                                                        "index": index,
                                                        "content_block": {
                                                            "type": "text",
                                                            "text": ""
                                                        }
                                                    });
                                                    let sse_data = format!("event: content_block_start\ndata: {}\n\n",
                                                        serde_json::to_string(&event).unwrap_or_default());
                                                    yield Ok(Bytes::from(sse_data));
                                                    current_non_tool_block_type = Some("text");
                                                    current_non_tool_block_index = Some(index);
                                                }

                                                if let Some(index) = current_non_tool_block_index {
                                                    let event = json!({
                                                        "type": "content_block_delta",
                                                        "index": index,
                                                        "delta": {
                                                            "type": "text_delta",
                                                            "text": content
                                                        }
                                                    });
                                                    let sse_data = format!("event: content_block_delta\ndata: {}\n\n",
                                                        serde_json::to_string(&event).unwrap_or_default());
                                                    yield Ok(Bytes::from(sse_data));
                                                }
                                            }
                                        }

                                        // 处理工具调用
                                        if let Some(tool_calls) = &choice.delta.tool_calls {
                                            if let Some(index) = current_non_tool_block_index.take() {
                                                let event = json!({
                                                    "type": "content_block_stop",
                                                    "index": index
                                                });
                                                let sse_data = format!("event: content_block_stop\ndata: {}\n\n",
                                                    serde_json::to_string(&event).unwrap_or_default());
                                                yield Ok(Bytes::from(sse_data));
                                            }
                                            current_non_tool_block_type = None;

                                            for tool_call in tool_calls {
                                                let (anthropic_index, id, name, should_start, pending_after_start, immediate_delta) = {
                                                    let state = tool_blocks_by_index
                                                        .entry(tool_call.index)
                                                        .or_insert_with(|| {
                                                            let index = next_content_index;
                                                            next_content_index += 1;
                                                            ToolBlockState {
                                                                anthropic_index: index,
                                                                id: String::new(),
                                                                name: String::new(),
                                                                started: false,
                                                                pending_args: String::new(),
                                                            }
                                                        });

                                                    if let Some(id) = &tool_call.id {
                                                        state.id = id.clone();
                                                    }
                                                    if let Some(function) = &tool_call.function {
                                                        if let Some(name) = &function.name {
                                                            state.name = name.clone();
                                                        }
                                                    }

                                                    let should_start = !state.started && !state.id.is_empty() && !state.name.is_empty();
                                                    if should_start {
                                                        state.started = true;
                                                    }
                                                    let pending_after_start = if should_start && !state.pending_args.is_empty() {
                                                        Some(std::mem::take(&mut state.pending_args))
                                                    } else {
                                                        None
                                                    };
                                                    let args_delta = tool_call.function.as_ref().and_then(|f| f.arguments.clone());
                                                    let immediate_delta = if let Some(args) = args_delta {
                                                        if state.started {
                                                            Some(args)
                                                        } else {
                                                            state.pending_args.push_str(&args);
                                                            None
                                                        }
                                                    } else {
                                                        None
                                                    };
                                                    (state.anthropic_index, state.id.clone(), state.name.clone(), should_start, pending_after_start, immediate_delta)
                                                };

                                                if should_start {
                                                    let event = json!({
                                                        "type": "content_block_start",
                                                        "index": anthropic_index,
                                                        "content_block": {
                                                            "type": "tool_use",
                                                            "id": id,
                                                            "name": name
                                                        }
                                                    });
                                                    let sse_data = format!("event: content_block_start\ndata: {}\n\n",
                                                        serde_json::to_string(&event).unwrap_or_default());
                                                    yield Ok(Bytes::from(sse_data));
                                                    open_tool_block_indices.insert(anthropic_index);
                                                }

                                                if let Some(args) = pending_after_start {
                                                    let event = json!({
                                                        "type": "content_block_delta",
                                                        "index": anthropic_index,
                                                        "delta": {
                                                            "type": "input_json_delta",
                                                            "partial_json": args
                                                        }
                                                    });
                                                    let sse_data = format!("event: content_block_delta\ndata: {}\n\n",
                                                        serde_json::to_string(&event).unwrap_or_default());
                                                    yield Ok(Bytes::from(sse_data));
                                                }

                                                if let Some(args) = immediate_delta {
                                                    let event = json!({
                                                        "type": "content_block_delta",
                                                        "index": anthropic_index,
                                                        "delta": {
                                                            "type": "input_json_delta",
                                                            "partial_json": args
                                                        }
                                                    });
                                                    let sse_data = format!("event: content_block_delta\ndata: {}\n\n",
                                                        serde_json::to_string(&event).unwrap_or_default());
                                                    yield Ok(Bytes::from(sse_data));
                                                }
                                            }
                                        }

                                        // 处理 finish_reason
                                        if let Some(finish_reason) = &choice.finish_reason {
                                            if let Some(index) = current_non_tool_block_index.take() {
                                                let event = json!({
                                                    "type": "content_block_stop",
                                                    "index": index
                                                });
                                                let sse_data = format!("event: content_block_stop\ndata: {}\n\n",
                                                    serde_json::to_string(&event).unwrap_or_default());
                                                yield Ok(Bytes::from(sse_data));
                                            }
                                            current_non_tool_block_type = None;

                                            // Late start for blocks that accumulated args before id/name arrived
                                            let mut late_tool_starts: Vec<(u32, String, String, String)> = Vec::new();
                                            for (tool_idx, state) in tool_blocks_by_index.iter_mut() {
                                                if state.started { continue; }
                                                let has_payload = !state.pending_args.is_empty() || !state.id.is_empty() || !state.name.is_empty();
                                                if !has_payload { continue; }
                                                let fallback_id = if state.id.is_empty() { format!("tool_call_{tool_idx}") } else { state.id.clone() };
                                                let fallback_name = if state.name.is_empty() { "unknown_tool".to_string() } else { state.name.clone() };
                                                state.started = true;
                                                let pending = std::mem::take(&mut state.pending_args);
                                                late_tool_starts.push((state.anthropic_index, fallback_id, fallback_name, pending));
                                            }
                                            late_tool_starts.sort_unstable_by_key(|(index, _, _, _)| *index);
                                            for (index, id, name, pending) in late_tool_starts {
                                                let event = json!({
                                                    "type": "content_block_start",
                                                    "index": index,
                                                    "content_block": {"type": "tool_use", "id": id, "name": name}
                                                });
                                                let sse_data = format!("event: content_block_start\ndata: {}\n\n",
                                                    serde_json::to_string(&event).unwrap_or_default());
                                                yield Ok(Bytes::from(sse_data));
                                                open_tool_block_indices.insert(index);
                                                if !pending.is_empty() {
                                                    let delta_event = json!({
                                                        "type": "content_block_delta",
                                                        "index": index,
                                                        "delta": {"type": "input_json_delta", "partial_json": pending}
                                                    });
                                                    let delta_sse = format!("event: content_block_delta\ndata: {}\n\n",
                                                        serde_json::to_string(&delta_event).unwrap_or_default());
                                                    yield Ok(Bytes::from(delta_sse));
                                                }
                                            }

                                            if !open_tool_block_indices.is_empty() {
                                                let mut tool_indices: Vec<u32> = open_tool_block_indices.iter().copied().collect();
                                                tool_indices.sort_unstable();
                                                for index in tool_indices {
                                                    let event = json!({"type": "content_block_stop", "index": index});
                                                    let sse_data = format!("event: content_block_stop\ndata: {}\n\n",
                                                        serde_json::to_string(&event).unwrap_or_default());
                                                    yield Ok(Bytes::from(sse_data));
                                                }
                                                open_tool_block_indices.clear();
                                            }

                                            let stop_reason = map_stop_reason(Some(finish_reason));
                                            let usage_json = chunk.usage.as_ref().map(|u| {
                                                // 收集 usage 到共享收集器
                                                if let Ok(mut collector) = usage_collector.lock() {
                                                    collector.input_tokens = u.prompt_tokens;
                                                    collector.output_tokens = u.completion_tokens;
                                                    collector.cache_read_tokens = extract_cache_read_tokens(u).unwrap_or(0);
                                                    collector.cache_creation_tokens = u.cache_creation_input_tokens.unwrap_or(0);
                                                }
                                                let mut uj = json!({"input_tokens": u.prompt_tokens, "output_tokens": u.completion_tokens});
                                                if let Some(cached) = extract_cache_read_tokens(u) {
                                                    uj["cache_read_input_tokens"] = json!(cached);
                                                }
                                                if let Some(created) = u.cache_creation_input_tokens {
                                                    uj["cache_creation_input_tokens"] = json!(created);
                                                }
                                                uj
                                            });
                                            let event = json!({
                                                "type": "message_delta",
                                                "delta": {"stop_reason": stop_reason, "stop_sequence": null},
                                                "usage": usage_json
                                            });
                                            let sse_data = format!("event: message_delta\ndata: {}\n\n",
                                                serde_json::to_string(&event).unwrap_or_default());
                                            yield Ok(Bytes::from(sse_data));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("[cc-proxy] Stream error: {e}");
                    let error_event = json!({
                        "type": "error",
                        "error": {"type": "stream_error", "message": format!("Stream error: {e}")}
                    });
                    let sse_data = format!("event: error\ndata: {}\n\n",
                        serde_json::to_string(&error_event).unwrap_or_default());
                    yield Ok(Bytes::from(sse_data));
                    // 流出错也发送 done 信号，避免 usage 记录任务永远等待
                    if let Some(tx) = done_tx.take() {
                        let _ = tx.send(());
                    }
                    break;
                }
            }
        }
    }
}

/// Extract cache_read tokens from Usage
fn extract_cache_read_tokens(usage: &Usage) -> Option<u32> {
    if let Some(v) = usage.cache_read_input_tokens {
        return Some(v);
    }
    usage
        .prompt_tokens_details
        .as_ref()
        .map(|d| d.cached_tokens)
        .filter(|&v| v > 0)
}

/// 映射停止原因
fn map_stop_reason(finish_reason: Option<&str>) -> Option<String> {
    finish_reason.map(|r| {
        match r {
            "tool_calls" | "function_call" => "tool_use",
            "stop" => "end_turn",
            "length" => "max_tokens",
            "content_filter" => "end_turn",
            other => {
                log::warn!("[cc-proxy] Unknown finish_reason in streaming: {other}");
                "end_turn"
            }
        }
        .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[test]
    fn test_map_stop_reason() {
        assert_eq!(map_stop_reason(Some("stop")), Some("end_turn".to_string()));
        assert_eq!(map_stop_reason(Some("tool_calls")), Some("tool_use".to_string()));
        assert_eq!(map_stop_reason(Some("length")), Some("max_tokens".to_string()));
        assert_eq!(map_stop_reason(Some("content_filter")), Some("end_turn".to_string()));
    }

    #[tokio::test]
    async fn test_streaming_simple_text() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n"
        );
        let upstream = stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(input.as_bytes().to_vec()))]);
        let collector = new_usage_collector();
        let converted = create_anthropic_sse_stream(upstream, collector, None);
        let chunks: Vec<_> = converted.collect().await;

        let merged = chunks.into_iter()
            .map(|chunk| String::from_utf8_lossy(chunk.unwrap().as_ref()).to_string())
            .collect::<String>();

        // 验证包含关键事件
        assert!(merged.contains("message_start"));
        assert!(merged.contains("content_block_start"));
        assert!(merged.contains("text_delta"));
        assert!(merged.contains("Hello"));
        assert!(merged.contains("message_delta"));
        assert!(merged.contains("end_turn"));
        assert!(merged.contains("message_stop"));
    }
}
