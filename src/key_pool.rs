//! API Key 轮询池 + 上游 URL 解析
//!
//! 使用 Round-Robin 策略从数据库中选取活跃密钥，
//! 每个密钥绑定到一个上游端点，选中后返回 (key_id, api_key, base_url)。
//! 无可用密钥时回退到配置文件中的 fallback 密钥和 URL。

use crate::config::ProxyConfig;
use crate::database::Database;
use crate::error::ProxyError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct KeyPool {
    db: Arc<Database>,
    config: Arc<ProxyConfig>,
    counter: AtomicU64,
}

impl KeyPool {
    pub fn new(db: Arc<Database>, config: Arc<ProxyConfig>) -> Self {
        Self {
            db,
            config,
            counter: AtomicU64::new(0),
        }
    }

    /// 轮询选取下一个可用密钥（基于入站 token 过滤渠道）
    ///
    /// 返回 `(key_id, api_key_value, upstream_base_url, endpoint_id)`：
    /// - key_id 为 Some 时表示来自数据库，None 时表示来自配置 fallback
    pub async fn next_key(&self, inbound_token: &str) -> Result<(Option<String>, String, String, String), ProxyError> {
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
                        return Ok((None, key.clone(), upstream.base_url.clone(), String::new()));
                    }
                }
            }
            return Err(ProxyError::Internal(
                "没有可用的 API Key，请在密钥管理中添加端点和密钥，并绑定到访问密钥".to_string(),
            ));
        }

        let idx = self.counter.fetch_add(1, Ordering::Relaxed) as usize % keys.len();
        let selected = &keys[idx];
        Ok((
            Some(selected.id.clone()),
            selected.api_key.clone(),
            selected.base_url.clone(),
            selected.endpoint_id.clone(),
        ))
    }

    /// 上报密钥使用结果，更新统计
    pub async fn report_result(&self, key_id: &str, success: bool) {
        let db = self.db.clone();
        let key_id = key_id.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = db.increment_key_stats(&key_id, success) {
                log::warn!("[cc-proxy] 更新 key 统计失败: {e}");
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
