//! Thinking 优化器
//!
//! 根据模型类型自动优化 thinking 配置
//! 来源: cc-switch 项目 (src-tauri/src/proxy/thinking_optimizer.rs)

use serde_json::{json, Value};

/// 根据模型类型自动优化 thinking 配置
///
/// 三路径分发：
/// - skip: haiku 模型直接跳过
/// - adaptive: opus-4-6 / sonnet-4-6 使用 adaptive thinking
/// - legacy: 其他模型注入 enabled thinking + budget_tokens
pub fn optimize(body: &mut Value, enabled: bool) {
    if !enabled {
        return;
    }

    let model = match body.get("model").and_then(|m| m.as_str()) {
        Some(m) => m.to_lowercase(),
        None => return,
    };

    if model.contains("haiku") {
        log::info!("[cc-proxy] thinking: skip(haiku)");
        return;
    }

    if model.contains("opus-4-6") || model.contains("sonnet-4-6") {
        log::info!("[cc-proxy] thinking: adaptive({model})");
        body["thinking"] = json!({"type": "adaptive"});
        body["output_config"] = json!({"effort": "max"});
        append_beta(body, "context-1m-2025-08-07");
        return;
    }

    // legacy path
    log::info!("[cc-proxy] thinking: legacy({model})");

    let max_tokens = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(16384);

    let budget_target = max_tokens.saturating_sub(1);

    let thinking_type = body
        .get("thinking")
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    match thinking_type.as_deref() {
        None | Some("disabled") => {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": budget_target
            });
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
        Some("enabled") => {
            let current_budget = body
                .get("thinking")
                .and_then(|t| t.get("budget_tokens"))
                .and_then(|b| b.as_u64())
                .unwrap_or(0);
            if current_budget < budget_target {
                body["thinking"]["budget_tokens"] = json!(budget_target);
            }
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
        _ => {
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
    }
}

/// 追加 beta 标识到 anthropic_beta 数组（去重）
fn append_beta(body: &mut Value, beta: &str) {
    match body.get("anthropic_beta") {
        Some(Value::Array(arr)) => {
            if arr.iter().any(|v| v.as_str() == Some(beta)) {
                return;
            }
            body["anthropic_beta"]
                .as_array_mut()
                .unwrap()
                .push(json!(beta));
        }
        Some(Value::Null) | None => {
            body["anthropic_beta"] = json!([beta]);
        }
        _ => {
            body["anthropic_beta"] = json!([beta]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_adaptive_sonnet_4_6() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });
        optimize(&mut body, true);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "max");
    }

    #[test]
    fn test_legacy_sonnet_4_5() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });
        optimize(&mut body, true);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16383);
    }

    #[test]
    fn test_skip_haiku() {
        let mut body = json!({
            "model": "anthropic.claude-haiku-4-5-20250514-v1:0",
            "max_tokens": 8192,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let original = body.clone();
        optimize(&mut body, true);
        assert_eq!(body, original);
    }

    #[test]
    fn test_disabled() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let original = body.clone();
        optimize(&mut body, false);
        assert_eq!(body, original);
    }
}
