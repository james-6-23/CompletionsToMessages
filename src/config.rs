//! 配置模块
//!
//! 支持 YAML 配置文件 + 环境变量 + CLI 参数覆盖

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    /// 监听地址
    #[serde(default = "default_listen")]
    pub listen: String,

    /// 入站认证 token（可选，不设置则不校验）
    #[serde(default)]
    pub auth_token: Option<String>,

    /// 上游 OpenAI 兼容 API 配置（可选 fallback，推荐通过管理界面配置）
    #[serde(default)]
    pub upstream: Option<UpstreamConfig>,

    /// 功能开关
    #[serde(default)]
    pub features: FeatureConfig,

    /// 超时配置
    #[serde(default)]
    pub timeouts: TimeoutConfig,

    /// 日志级别
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// SQLite 数据库路径
    #[serde(default = "default_db_path")]
    pub database_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamConfig {
    /// 上游 API 基础 URL
    pub base_url: String,

    /// 上游 API Key（可选 fallback，推荐通过管理界面添加密钥）
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FeatureConfig {
    /// 高强度思考优化器
    #[serde(default)]
    pub thinking_optimizer: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimeoutConfig {
    /// 请求总超时（秒）
    #[serde(default = "default_600")]
    pub request_timeout_secs: u64,
}

fn default_listen() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_db_path() -> String {
    "data/completions-to-messages.db".to_string()
}

fn default_600() -> u64 {
    600
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: default_600(),
        }
    }
}

impl ProxyConfig {
    /// 从 YAML 文件加载配置
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("读取配置文件失败: {e}"))?;
        serde_yaml::from_str(&content).map_err(|e| format!("解析配置文件失败: {e}"))
    }

    /// 用环境变量覆盖配置
    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("CC_PROXY_LISTEN") {
            self.listen = v;
        }
        if let Ok(v) = std::env::var("CC_PROXY_AUTH_TOKEN") {
            self.auth_token = Some(v);
        }
        if let Ok(url) = std::env::var("CC_PROXY_UPSTREAM_URL") {
            let upstream = self.upstream.get_or_insert(UpstreamConfig {
                base_url: String::new(),
                api_key: None,
            });
            upstream.base_url = url;
        }
        if let Ok(key) = std::env::var("CC_PROXY_UPSTREAM_KEY") {
            let upstream = self.upstream.get_or_insert(UpstreamConfig {
                base_url: String::new(),
                api_key: None,
            });
            upstream.api_key = Some(key);
        }
        if let Ok(v) = std::env::var("CC_PROXY_LOG_LEVEL") {
            self.log_level = v;
        }
    }

    /// 用 CLI 参数覆盖配置
    pub fn apply_cli_overrides(
        &mut self,
        listen: Option<String>,
        upstream_url: Option<String>,
        upstream_key: Option<String>,
        auth_token: Option<String>,
    ) {
        if let Some(v) = listen {
            self.listen = v;
        }
        if let Some(url) = upstream_url {
            let upstream = self.upstream.get_or_insert(UpstreamConfig {
                base_url: String::new(),
                api_key: None,
            });
            upstream.base_url = url;
        }
        if let Some(key) = upstream_key {
            let upstream = self.upstream.get_or_insert(UpstreamConfig {
                base_url: String::new(),
                api_key: None,
            });
            upstream.api_key = Some(key);
        }
        if let Some(v) = auth_token {
            self.auth_token = Some(v);
        }
    }

    /// 创建默认配置
    pub fn default_config() -> Self {
        Self {
            listen: default_listen(),
            auth_token: None,
            upstream: None,
            features: FeatureConfig::default(),
            timeouts: TimeoutConfig::default(),
            log_level: default_log_level(),
            database_path: default_db_path(),
        }
    }
}
