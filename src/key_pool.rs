//! API Key 轮询池 + 上游 URL 解析
//!
//! 使用 Round-Robin 策略从数据库中选取活跃密钥，
//! 无可用密钥时回退到配置文件中的 fallback 密钥。
//! 上游 URL 优先从数据库读取，fallback 到配置文件。

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

    /// 获取上游 base_url（数据库优先，fallback 到 config）
    pub async fn get_upstream_url(&self) -> Result<String, ProxyError> {
        let db = self.db.clone();
        let db_url = tokio::task::spawn_blocking(move || db.get_upstream_url())
            .await
            .map_err(|e| ProxyError::Internal(format!("获取上游 URL 失败: {e}")))?
            .map_err(|e| ProxyError::Internal(format!("获取上游 URL 失败: {e}")))?;

        if let Some(url) = db_url {
            if !url.is_empty() {
                return Ok(url);
            }
        }

        // fallback 到 config
        if let Some(ref upstream) = self.config.upstream {
            if !upstream.base_url.is_empty() {
                return Ok(upstream.base_url.clone());
            }
        }

        Err(ProxyError::Internal(
            "未配置上游 URL，请在管理界面设置".to_string(),
        ))
    }

    /// 轮询选取下一个可用密钥
    ///
    /// 返回 `(key_id, api_key_value)`：
    /// - key_id 为 Some 时表示来自数据库，None 时表示来自配置 fallback
    pub async fn next_key(&self) -> Result<(Option<String>, String), ProxyError> {
        let db = self.db.clone();
        let keys = tokio::task::spawn_blocking(move || db.get_all_active_keys())
            .await
            .map_err(|e| ProxyError::Internal(format!("Key pool error: {e}")))?
            .map_err(|e| ProxyError::Internal(format!("Key pool DB error: {e}")))?;

        if keys.is_empty() {
            // 回退到配置文件中的 fallback 密钥
            if let Some(ref upstream) = self.config.upstream {
                if let Some(ref key) = upstream.api_key {
                    if !key.is_empty() {
                        return Ok((None, key.clone()));
                    }
                }
            }
            return Err(ProxyError::Internal(
                "没有可用的 API Key，请在密钥管理中添加".to_string(),
            ));
        }

        let idx = self.counter.fetch_add(1, Ordering::Relaxed) as usize % keys.len();
        let selected = &keys[idx];
        Ok((Some(selected.id.clone()), selected.api_key.clone()))
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
}
