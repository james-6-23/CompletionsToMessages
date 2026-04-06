//! 提示缓存优化模块
//!
//! 通过稳定 system prompt + tools 前缀的字节表示，最大化上游 API 的 prompt cache 命中率。
//!
//! 原理：
//! - OpenAI / DeepSeek 等 API 基于请求前缀的字节级匹配做自动缓存（≥1024 tokens）
//! - Claude Code 每轮对话发送相同的 system prompt + tools（~11K tokens）
//! - 本模块缓存已转换的前缀，保证同一会话内 system+tools 部分字节完全一致
//! - 同时生成 `prompt_cache_key` 哈希，辅助支持该字段的上游做缓存路由

use dashmap::DashMap;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 缓存条目：已转换的 system messages + tools（OpenAI 格式）
struct CacheEntry {
    /// 已转换的 system messages（OpenAI 格式 JSON 数组）
    system_messages: Vec<Value>,
    /// 已转换的 tools（OpenAI 格式 JSON 数组）
    tools: Vec<Value>,
    /// 前缀哈希，用作 prompt_cache_key
    cache_key: String,
    /// 创建时间
    created_at: Instant,
}

/// 提示前缀缓存
///
/// 将 Anthropic 格式的 system + tools 转换结果缓存起来，
/// 同一前缀在 TTL 内直接复用，保证字节级一致。
pub struct PromptCache {
    /// 缓存存储：hash(原始 system+tools) → 已转换结果
    entries: DashMap<u64, CacheEntry>,
    /// 缓存过期时间（与上游 prompt cache TTL 对齐）
    ttl: Duration,
    /// 最大缓存条目数（防止内存无限增长）
    max_entries: usize,
}

/// 缓存查询结果
pub struct CacheResult {
    /// 已缓存的 system messages（OpenAI 格式），直接替换到请求中
    pub system_messages: Vec<Value>,
    /// 已缓存的 tools（OpenAI 格式），直接替换到请求中
    pub tools: Vec<Value>,
    /// 前缀哈希 key，注入到请求的 prompt_cache_key 字段
    pub cache_key: String,
    /// 是否命中缓存
    pub hit: bool,
}

impl PromptCache {
    pub fn new(ttl_secs: u64, max_entries: usize) -> Self {
        Self {
            entries: DashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
            max_entries,
        }
    }

    /// 计算 Anthropic 原始 system + tools 的哈希
    ///
    /// 只对原始请求中的 system 和 tools 字段做哈希，不含 messages（每轮都变）
    fn compute_prefix_hash(system: Option<&Value>, tools: Option<&Value>) -> u64 {
        let mut hasher = DefaultHasher::new();
        // 使用原始 JSON 字节做哈希，保证相同结构得到相同哈希
        if let Some(s) = system {
            s.to_string().hash(&mut hasher);
        }
        if let Some(t) = tools {
            t.to_string().hash(&mut hasher);
        }
        hasher.finish()
    }

    /// 生成短哈希 key（16 位十六进制）
    fn hash_to_key(hash: u64) -> String {
        format!("{:016x}", hash)
    }

    /// 查询或填充缓存
    ///
    /// - 命中：返回已缓存的转换结果（字节完全一致）
    /// - 未命中：调用 `convert_fn` 转换，缓存结果后返回
    pub fn get_or_convert<F>(
        &self,
        body: &Value,
        convert_fn: F,
    ) -> CacheResult
    where
        F: FnOnce(&Value) -> (Vec<Value>, Vec<Value>),
    {
        let system = body.get("system");
        let tools = body.get("tools");

        let prefix_hash = Self::compute_prefix_hash(system, tools);
        let cache_key = Self::hash_to_key(prefix_hash);

        // 尝试命中缓存
        if let Some(entry) = self.entries.get(&prefix_hash) {
            if entry.created_at.elapsed() < self.ttl {
                return CacheResult {
                    system_messages: entry.system_messages.clone(),
                    tools: entry.tools.clone(),
                    cache_key: entry.cache_key.clone(),
                    hit: true,
                };
            }
            // 过期，移除
            drop(entry);
            self.entries.remove(&prefix_hash);
        }

        // 缓存未命中，执行转换
        let (system_messages, tools_converted) = convert_fn(body);

        // 驱逐：超过上限时移除最旧条目
        if self.entries.len() >= self.max_entries {
            self.evict_oldest();
        }

        let result = CacheResult {
            system_messages: system_messages.clone(),
            tools: tools_converted.clone(),
            cache_key: cache_key.clone(),
            hit: false,
        };

        self.entries.insert(prefix_hash, CacheEntry {
            system_messages,
            tools: tools_converted,
            cache_key,
            created_at: Instant::now(),
        });

        result
    }

    /// 驱逐最旧的条目
    fn evict_oldest(&self) {
        let mut oldest_key = None;
        let mut oldest_time = Instant::now();

        for entry in self.entries.iter() {
            if entry.created_at < oldest_time {
                oldest_time = entry.created_at;
                oldest_key = Some(*entry.key());
            }
        }

        if let Some(key) = oldest_key {
            self.entries.remove(&key);
        }
    }

    /// 缓存统计
    pub fn stats(&self) -> (usize, usize) {
        (self.entries.len(), self.max_entries)
    }
}

/// 创建全局共享的 PromptCache
///
/// TTL 默认 5 分钟（与主流 API 的 prompt cache TTL 对齐）
/// 最大 200 条（每条约 50-100KB，总内存约 10-20MB）
pub fn create_prompt_cache() -> Arc<PromptCache> {
    Arc::new(PromptCache::new(300, 200))
}

/// 从 Anthropic body 中提取 system messages（转换为 OpenAI 格式）
pub fn convert_system_to_openai(body: &Value) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(system) = body.get("system") {
        if let Some(text) = system.as_str() {
            messages.push(serde_json::json!({"role": "system", "content": text}));
        } else if let Some(arr) = system.as_array() {
            for msg in arr {
                if let Some(text) = msg.get("text").and_then(|t| t.as_str()) {
                    let mut sys_msg = serde_json::json!({"role": "system", "content": text});
                    if let Some(cc) = msg.get("cache_control") {
                        sys_msg["cache_control"] = cc.clone();
                    }
                    messages.push(sys_msg);
                }
            }
        }
    }
    messages
}

/// 从 Anthropic body 中提取 tools（转换为 OpenAI 格式）
pub fn convert_tools_to_openai(body: &Value) -> Vec<Value> {
    let Some(tools) = body.get("tools").and_then(|t| t.as_array()) else {
        return Vec::new();
    };

    tools
        .iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) != Some("BatchTool"))
        .map(|t| {
            let mut tool = serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                    "description": t.get("description"),
                    "parameters": crate::transform::clean_schema(
                        t.get("input_schema").cloned().unwrap_or(serde_json::json!({}))
                    )
                }
            });
            if let Some(cc) = t.get("cache_control") {
                tool["cache_control"] = cc.clone();
            }
            tool
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cache_hit() {
        let cache = PromptCache::new(60, 100);

        let body = json!({
            "system": [{"type": "text", "text": "You are helpful.", "cache_control": {"type": "ephemeral"}}],
            "tools": [{"name": "bash", "description": "Run bash", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "hello"}]
        });

        // 第一次：miss
        let r1 = cache.get_or_convert(&body, |b| {
            (convert_system_to_openai(b), convert_tools_to_openai(b))
        });
        assert!(!r1.hit);
        assert!(!r1.cache_key.is_empty());

        // 第二次：hit
        let r2 = cache.get_or_convert(&body, |b| {
            (convert_system_to_openai(b), convert_tools_to_openai(b))
        });
        assert!(r2.hit);
        assert_eq!(r1.cache_key, r2.cache_key);

        // 字节完全一致
        assert_eq!(
            serde_json::to_string(&r1.system_messages).unwrap(),
            serde_json::to_string(&r2.system_messages).unwrap()
        );
    }

    #[test]
    fn test_different_system_different_key() {
        let cache = PromptCache::new(60, 100);

        let body1 = json!({"system": "prompt A", "messages": []});
        let body2 = json!({"system": "prompt B", "messages": []});

        let r1 = cache.get_or_convert(&body1, |b| {
            (convert_system_to_openai(b), convert_tools_to_openai(b))
        });
        let r2 = cache.get_or_convert(&body2, |b| {
            (convert_system_to_openai(b), convert_tools_to_openai(b))
        });

        assert_ne!(r1.cache_key, r2.cache_key);
    }

    #[test]
    fn test_same_system_different_messages_same_key() {
        let cache = PromptCache::new(60, 100);

        let body1 = json!({
            "system": "You are helpful.",
            "messages": [{"role": "user", "content": "hello"}]
        });
        let body2 = json!({
            "system": "You are helpful.",
            "messages": [{"role": "user", "content": "different message"}]
        });

        let r1 = cache.get_or_convert(&body1, |b| {
            (convert_system_to_openai(b), convert_tools_to_openai(b))
        });
        let r2 = cache.get_or_convert(&body2, |b| {
            (convert_system_to_openai(b), convert_tools_to_openai(b))
        });

        // system 相同 → cache_key 相同 → 第二次命中
        assert_eq!(r1.cache_key, r2.cache_key);
        assert!(r2.hit);
    }

    #[test]
    fn test_eviction() {
        let cache = PromptCache::new(60, 2);

        for i in 0..3 {
            let body = json!({"system": format!("prompt {i}"), "messages": []});
            cache.get_or_convert(&body, |b| {
                (convert_system_to_openai(b), convert_tools_to_openai(b))
            });
        }

        // 最多 2 条
        assert!(cache.entries.len() <= 2);
    }
}
