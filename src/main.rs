//! CompletionsToMessages: OpenAI Chat Completions → Anthropic Messages API 反向代理
//!
//! 让 Claude Code CLI 通过 OpenAI 兼容端点工作

mod auth;
mod config;
mod database;
mod error;
mod handler;
mod key_pool;
mod server;
mod sse;
mod stats_api;
mod streaming;
mod thinking;
mod transform;
mod usage;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "completions-to-messages", about = "OpenAI Chat Completions ↔ Anthropic Messages API reverse proxy")]
struct Cli {
    /// 配置文件路径
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// 监听地址 (覆盖配置文件)
    #[arg(long)]
    listen: Option<String>,

    /// 上游 API 地址 (覆盖配置文件)
    #[arg(long)]
    upstream_url: Option<String>,

    /// 上游 API Key (覆盖配置文件)
    #[arg(long)]
    upstream_key: Option<String>,

    /// 入站认证 Token (覆盖配置文件)
    #[arg(long)]
    auth_token: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // 加载配置
    let mut config = if let Some(config_path) = &cli.config {
        match config::ProxyConfig::from_file(config_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("配置加载失败: {e}");
                std::process::exit(1);
            }
        }
    } else {
        // 无配置文件时使用默认配置（上游 URL 和密钥通过管理界面配置）
        config::ProxyConfig::default_config()
    };

    // 环境变量覆盖
    config.apply_env_overrides();

    // CLI 参数覆盖
    config.apply_cli_overrides(cli.listen, cli.upstream_url, cli.upstream_key, cli.auth_token);

    // 初始化日志
    std::env::set_var("RUST_LOG", &config.log_level);
    env_logger::init();

    // 启动服务
    if let Err(e) = server::run(config).await {
        log::error!("[cc-proxy] 服务启动失败: {e}");
        std::process::exit(1);
    }
}
