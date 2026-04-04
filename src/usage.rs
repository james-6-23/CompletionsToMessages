//! Token 使用量提取和费用计算模块

use crate::database::{Database, ModelPricingRow};
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;

/// Token 使用量
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
}

/// 费用明细
pub struct CostBreakdown {
    pub input_cost: String,
    pub output_cost: String,
    pub cache_read_cost: String,
    pub cache_creation_cost: String,
    pub total_cost: String,
}

/// 从已转换的 Anthropic 格式响应中提取 token 使用量
///
/// 响应格式包含 `usage.input_tokens`, `usage.output_tokens`,
/// `usage.cache_read_input_tokens`, `usage.cache_creation_input_tokens`
pub fn extract_usage_from_anthropic_response(body: &Value) -> Option<TokenUsage> {
    let usage = body.get("usage")?;

    let input_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let output_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let cache_read_tokens = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let cache_creation_tokens = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    Some(TokenUsage {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
    })
}

/// 根据 token 使用量和定价计算费用
///
/// 费用 = tokens * cost_per_million / 1_000_000
pub fn calculate_cost(usage: &TokenUsage, pricing: &ModelPricingRow) -> CostBreakdown {
    let million = Decimal::from(1_000_000);

    let input_rate = Decimal::from_str(&pricing.input_cost_per_million).unwrap_or(Decimal::ZERO);
    let output_rate = Decimal::from_str(&pricing.output_cost_per_million).unwrap_or(Decimal::ZERO);
    let cache_read_rate = Decimal::from_str(&pricing.cache_read_cost_per_million).unwrap_or(Decimal::ZERO);
    let cache_creation_rate = Decimal::from_str(&pricing.cache_creation_cost_per_million).unwrap_or(Decimal::ZERO);

    let input_cost = Decimal::from(usage.input_tokens) * input_rate / million;
    let output_cost = Decimal::from(usage.output_tokens) * output_rate / million;
    let cache_read_cost = Decimal::from(usage.cache_read_tokens) * cache_read_rate / million;
    let cache_creation_cost = Decimal::from(usage.cache_creation_tokens) * cache_creation_rate / million;
    let total_cost = input_cost + output_cost + cache_read_cost + cache_creation_cost;

    CostBreakdown {
        input_cost: input_cost.to_string(),
        output_cost: output_cost.to_string(),
        cache_read_cost: cache_read_cost.to_string(),
        cache_creation_cost: cache_creation_cost.to_string(),
        total_cost: total_cost.to_string(),
    }
}

/// 零费用明细（未找到定价时使用）
fn zero_cost() -> CostBreakdown {
    CostBreakdown {
        input_cost: "0".to_string(),
        output_cost: "0".to_string(),
        cache_read_cost: "0".to_string(),
        cache_creation_cost: "0".to_string(),
        total_cost: "0".to_string(),
    }
}

/// 记录一次请求的使用数据到数据库
#[allow(clippy::too_many_arguments)]
pub async fn record_request(
    db: Arc<Database>,
    request_id: String,
    model: String,
    request_model: Option<String>,
    usage: TokenUsage,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    status_code: u16,
    is_streaming: bool,
    error_message: Option<String>,
) {
    // 在阻塞线程中执行数据库操作
    let result = tokio::task::spawn_blocking(move || {
        // 查询定价并计算费用
        let cost = match db.get_pricing_for_model(&model) {
            Ok(Some(pricing)) => calculate_cost(&usage, &pricing),
            _ => zero_cost(),
        };

        let created_at = chrono::Utc::now().timestamp();

        db.insert_request_log(
            &request_id,
            &model,
            request_model.as_deref(),
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_creation_tokens,
            &cost.input_cost,
            &cost.output_cost,
            &cost.cache_read_cost,
            &cost.cache_creation_cost,
            &cost.total_cost,
            latency_ms,
            first_token_ms,
            status_code,
            is_streaming,
            error_message.as_deref(),
            created_at,
        )
    })
    .await;

    match result {
        Ok(Ok(())) => {
            log::debug!("[cc-proxy] 使用数据已记录");
        }
        Ok(Err(e)) => {
            log::warn!("[cc-proxy] 记录使用数据失败: {e}");
        }
        Err(e) => {
            log::warn!("[cc-proxy] 记录使用数据任务失败: {e}");
        }
    }
}
