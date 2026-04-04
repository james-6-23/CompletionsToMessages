//! API Key 轮询池 + 上游 URL 解析
//!
//! 使用 Round-Robin 策略从数据库中选取活跃密钥，
//! 每个密钥绑定到一个上游端点，选中后返回 (key_id, api_key, base_url)。
//! 无可用密钥时回退到配置文件中的 fallback 密钥和 URL。
//! 集成熔断器，连续失败的密钥会被暂时跳过。

use crate::circuit_breaker::CircuitBreaker;
use crate::config::ProxyConfig;
use crate::database::Database;
use crate::error::ProxyError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 标准化模型名，用于模糊匹配
/// - 去掉 `anthropic/` 等供应商前缀
/// - 将 `.` 替换为 `-`（如 `claude-haiku-4.5` → `claude-haiku-4-5`）
/// - 转小写
fn normalize_model(name: &str) -> String {
    let mut s = name.to_lowercase();
    // 去掉供应商前缀
    if let Some(pos) = s.rfind('/') {
        s = s[pos + 1..].to_string();
    }
    // 将 `.` 替换为 `-`
    s = s.replace('.', "-");
    s
}

/// 检查请求的模型名是否匹配端点模型列表中的某一项
/// 支持：精确匹配、标准化后精确匹配、前缀匹配（带日期后缀的版本）
fn model_matches(request_model: &str, endpoint_model: &str) -> bool {
    if request_model == endpoint_model {
        return true;
    }
    let req = normalize_model(request_model);
    let ep = normalize_model(endpoint_model);
    // 标准化后精确匹配
    if req == ep {
        return true;
    }
    // 前缀匹配：请求模型以端点模型为前缀（如 claude-haiku-4-5-20251001 以 claude-haiku-4-5 开头）
    // 或反过来（端点列表里是完整名，请求用的是短名）
    req.starts_with(&format!("{ep}-")) || ep.starts_with(&format!("{req}-"))
}

pub struct KeyPool {
    db: Arc<Database>,
    config: Arc<ProxyConfig>,
    counter: AtomicU64,
    circuit_breaker: CircuitBreaker,
}

impl KeyPool {
    pub fn new(db: Arc<Database>, config: Arc<ProxyConfig>) -> Self {
        Self {
            db,
            config,
            counter: AtomicU64::new(0),
            // 连续 3 次失败触发熔断，半开状态连续 2 次成功恢复，熔断超时 60 秒
            circuit_breaker: CircuitBreaker::new(3, 2, 60),
        }
    }

    /// 轮询选取下一个可用密钥（基于入站 token 过滤渠道，可按模型筛选）
    ///
    /// 返回 `(key_id, api_key_value, upstream_base_url, endpoint_id, proxy_url, mapped_model, max_retries)`：
    /// - key_id 为 Some 时表示来自数据库，None 时表示来自配置 fallback
    /// - model 不为 None 时，优先选择明确支持该模型的端点密钥
    /// - mapped_model 不为 None 时表示需要将请求模型替换为映射后的模型
    pub async fn next_key(
        &self,
        inbound_token: &str,
        model: Option<&str>,
    ) -> Result<(Option<String>, String, String, String, String, Option<String>, u32, u32), ProxyError> {
        let db = self.db.clone();
        let token = inbound_token.to_string();
        let keys = tokio::task::spawn_blocking(move || db.get_active_keys_for_token(&token))
            .await
            .map_err(|e| ProxyError::Internal(format!("Key pool error: {e}")))?
            .map_err(|e| ProxyError::Internal(format!("Key pool DB error: {e}")))?;

        if keys.is_empty() {
            // 回退到配置文件中的 fallback 密钥 + URL
            if let Some(ref upstream) = self.config.upstream {
                if let Some(ref key) = upstream.api_key {
                    if !key.is_empty() && !upstream.base_url.is_empty() {
                        return Ok((None, key.clone(), upstream.base_url.clone(), String::new(), String::new(), None, 0, 0));
                    }
                }
            }
            return Err(ProxyError::Internal(
                "没有可用的 API Key，请在密钥管理中添加端点和密钥，并绑定到访问密钥".to_string(),
            ));
        }

        // 过滤出熔断器允许的密钥
        let available_keys: Vec<_> = keys
            .iter()
            .filter(|k| self.circuit_breaker.is_available(&k.id))
            .collect();

        // 按模型筛选：只保留端点模型列表为空（不限制）或包含请求模型的密钥
        // model_mapping 中的 key 也视为支持的模型
        let model_filtered: Vec<_> = if let Some(m) = model {
            available_keys
                .iter()
                .filter(|k| {
                    k.endpoint_models.is_empty() && k.model_mapping.is_empty()
                        || k.endpoint_models.iter().any(|em| model_matches(m, em))
                        || k.model_mapping.keys().any(|mk| model_matches(m, mk))
                })
                .copied()
                .collect()
        } else {
            available_keys.clone()
        };

        let final_keys = if model_filtered.is_empty() && model.is_some() {
            // 检查是否所有渠道都配置了模型列表或模型映射（即都在做模型限制）
            let all_have_models = available_keys.iter().all(|k| !k.endpoint_models.is_empty() || !k.model_mapping.is_empty());
            if all_have_models {
                // 所有渠道都配了模型列表但都不支持该模型，拒绝请求
                return Err(ProxyError::Internal(format!(
                    "没有渠道支持模型 {}，请检查渠道的模型配置",
                    model.unwrap_or("?")
                )));
            }
            // 存在未配置模型列表的渠道（不限制模型），回退到这些渠道
            let unrestricted: Vec<_> = available_keys
                .iter()
                .filter(|k| k.endpoint_models.is_empty() && k.model_mapping.is_empty())
                .copied()
                .collect();
            if unrestricted.is_empty() {
                available_keys
            } else {
                unrestricted
            }
        } else {
            model_filtered
        };

        let idx = self.counter.fetch_add(1, Ordering::Relaxed) as usize;

        // 如果所有密钥都被熔断，仍然使用轮询选取（不完全阻塞）
        let selected = if final_keys.is_empty() {
            log::warn!("[cc-proxy] 所有密钥均被熔断，回退到轮询选取");
            &keys[idx % keys.len()]
        } else {
            final_keys[idx % final_keys.len()]
        };

        // 查找模型映射：如果选中的端点对请求模型有映射，返回映射后的模型名
        let mapped_model = model.and_then(|m| {
            // 精确匹配
            if let Some(target) = selected.model_mapping.get(m) {
                return Some(target.clone());
            }
            // 标准化匹配（处理 claude-haiku-4.5 vs claude-haiku-4-5 等情况）
            let req_norm = normalize_model(m);
            for (from, to) in &selected.model_mapping {
                if normalize_model(from) == req_norm {
                    return Some(to.clone());
                }
            }
            None
        });

        Ok((
            Some(selected.id.clone()),
            selected.api_key.clone(),
            selected.base_url.clone(),
            selected.endpoint_id.clone(),
            selected.proxy_url.clone(),
            mapped_model,
            selected.max_failures,
            selected.max_retries,
        ))
    }

    /// 上报密钥使用结果，更新统计和熔断器状态
    ///
    /// `status_code`: 上游 HTTP 状态码，None 表示网络错误（无响应）。
    /// `max_failures`: 端点配置的最大失败阈值（0 = 不限制）。超过则永久禁用密钥。
    /// - 401：key 无效，直接标记为失效（数据库 is_active=0）
    /// - 402：余额不足，触发熔断
    /// - 429/529：限流/过载，触发熔断
    /// - 5xx：服务端错误，触发熔断
    /// - 其他 4xx：不触发熔断
    pub async fn report_result(&self, key_id: &str, success: bool, status_code: Option<u16>, max_failures: u32) {
        // 401 直接标记 key 失效
        if status_code == Some(401) {
            log::warn!(
                "[cc-proxy] 密钥 {} 认证失败 (401)，标记为失效",
                &key_id[..key_id.len().min(8)]
            );
            self.circuit_breaker.record_failure(key_id);
            let db = self.db.clone();
            let kid = key_id.to_string();
            let _ = tokio::task::spawn_blocking(move || {
                if let Err(e) = db.update_api_key_status(&kid, false) {
                    log::warn!("[cc-proxy] 标记密钥失效失败: {e}");
                }
            })
            .await;
            let db = self.db.clone();
            let kid = key_id.to_string();
            let _ = tokio::task::spawn_blocking(move || {
                if let Err(e) = db.increment_key_stats(&kid, false) {
                    log::warn!("[cc-proxy] 更新 key 统计失败: {e}");
                }
            })
            .await;
            return;
        }

        // 更新熔断器状态：根据状态码智能判断
        if success {
            self.circuit_breaker.record_success(key_id);
        } else {
            let should_fuse = match status_code {
                None => true,                      // 网络错误，触发熔断
                Some(402) => true,                 // 余额不足，触发熔断（自动切换到其他健康 key）
                Some(429) | Some(529) => true,     // 限流/过载，触发熔断
                Some(code) if code >= 500 => true, // 服务端错误，触发熔断
                Some(_) => false,                  // 其他 4xx 客户端错误，不触发熔断
            };
            if should_fuse {
                self.circuit_breaker.record_failure(key_id);
            } else {
                // 客户端错误不代表密钥有问题，记为成功以维持熔断器健康状态
                self.circuit_breaker.record_success(key_id);
            }
        }

        let db = self.db.clone();
        let key_id = key_id.to_string();
        let max_f = max_failures;
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = db.increment_key_stats(&key_id, success) {
                log::warn!("[cc-proxy] 更新 key 统计失败: {e}");
            }
            // 黑名单阈值检查：失败次数超过端点配置的阈值时，永久禁用密钥
            if !success && max_f > 0 {
                if let Ok(keys) = db.list_api_keys(None) {
                    if let Some(k) = keys.iter().find(|k| k.id == key_id) {
                        if k.failed_requests >= max_f as u64 {
                            log::warn!(
                                "[cc-proxy] 密钥 {} 失败次数 ({}) 达到阈值 ({})，标记为失效",
                                &key_id[..key_id.len().min(8)],
                                k.failed_requests,
                                max_f
                            );
                            if let Err(e) = db.update_api_key_status(&key_id, false) {
                                log::warn!("[cc-proxy] 标记密钥失效失败: {e}");
                            }
                        }
                    }
                }
            }
        })
        .await;
    }

    /// 上报访问密钥使用结果，更新统计
    pub async fn report_access_token(&self, token: &str, success: bool) {
        let db = self.db.clone();
        let token = token.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            // 先查找 token 对应的 id
            match db.get_access_token_by_value(&token) {
                Ok(Some(at)) => {
                    if let Err(e) = db.increment_access_token_stats(&at.id, success) {
                        log::warn!("[cc-proxy] 更新访问密钥统计失败: {e}");
                    }
                }
                Ok(None) => {} // token 不存在（可能是 fallback 配置），忽略
                Err(e) => log::warn!("[cc-proxy] 查询访问密钥失败: {e}"),
            }
        })
        .await;
    }
}
