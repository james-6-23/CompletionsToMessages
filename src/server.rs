//! HTTP 服务模块

use crate::config::ProxyConfig;
use crate::database::Database;
use crate::handler;
use crate::key_pool::KeyPool;
use crate::stats_api;
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, post, put, delete},
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;
use tower_http::services::ServeDir;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ProxyConfig>,
    pub http_client: reqwest::Client,
    pub db: Arc<Database>,
    pub key_pool: Arc<KeyPool>,
    pub admin_secret: Option<String>,
}

/// 管理接口鉴权中间件
async fn admin_auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let Some(ref secret) = state.admin_secret else {
        // 未设置 ADMIN_SECRET，放行（开发模式）
        return Ok(next.run(req).await);
    };

    // 从 query 参数、header 或 cookie 中提取 token
    let token = req
        .headers()
        .get("x-admin-secret")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            req.headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .map(|s| s.to_string())
        })
        .or_else(|| {
            // 从 URL query 参数中提取 ?secret=xxx
            req.uri()
                .query()
                .and_then(|q| {
                    q.split('&')
                        .find_map(|pair| pair.strip_prefix("secret="))
                        .map(|s| s.to_string())
                })
        });

    match token {
        Some(t) if t == *secret => Ok(next.run(req).await),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "type": "error",
                "error": {"type": "admin_auth_error", "message": "管理密钥无效"}
            })),
        )),
    }
}

/// 启动 HTTP 服务
pub async fn run(config: ProxyConfig) -> Result<(), Box<dyn std::error::Error>> {
    // 初始化数据库
    let db = Database::new(&config.database_path).map_err(|e| {
        format!("数据库初始化失败: {e}")
    })?;
    log::info!("[cc-proxy] 数据库已初始化: {}", config.database_path);

    let timeout = std::time::Duration::from_secs(config.timeouts.request_timeout_secs);
    let http_client = reqwest::Client::builder()
        .timeout(timeout)
        .build()?;

    let db = Arc::new(db);
    let admin_secret = std::env::var("ADMIN_SECRET").ok().filter(|s| !s.is_empty());
    let config = Arc::new(config.clone());
    let key_pool = Arc::new(KeyPool::new(db.clone(), config.clone()));

    let state = AppState {
        config: config.clone(),
        http_client,
        db,
        key_pool,
        admin_secret: admin_secret.clone(),
    };

    // 管理 API 路由（受 ADMIN_SECRET 保护）
    let admin_api = Router::new()
        .route("/stats/summary", get(stats_api::get_summary))
        .route("/stats/trends", get(stats_api::get_trends))
        .route("/stats/models", get(stats_api::get_models))
        .route("/stats/logs", get(stats_api::get_logs))
        .route("/stats/pricing", get(stats_api::get_pricing))
        .route("/config", get(stats_api::get_config_info))
        .route("/keys", get(stats_api::list_keys).post(stats_api::add_key))
        .route("/keys/:id", delete(stats_api::delete_key))
        .route("/keys/:id/status", put(stats_api::update_key_status))
        .route("/keys/:id/full", get(stats_api::get_key_full))
        .route("/keys/:id/test", post(stats_api::test_key))
        .route("/endpoints", get(stats_api::list_endpoints).post(stats_api::add_endpoint))
        .route("/endpoints/:id", put(stats_api::update_endpoint).delete(stats_api::delete_endpoint))
        .route("/endpoints/:id/status", put(stats_api::update_endpoint_status))
        .route("/endpoints/:id/models", get(stats_api::get_endpoint_models))
        .route("/endpoints/:id/sync-models", post(stats_api::sync_endpoint_models))
        .route("/access-tokens", get(stats_api::list_access_tokens).post(stats_api::add_access_token))
        .route("/access-tokens/:id", delete(stats_api::delete_access_token))
        .route("/access-tokens/:id/status", put(stats_api::update_access_token_status))
        .route("/access-tokens/:id/channels", put(stats_api::update_access_token_channels))
        .route_layer(middleware::from_fn_with_state(state.clone(), admin_auth))
        .with_state(state.clone());

    // 登录验证端点（不受 admin_auth 保护，用于前端验证密钥）
    let auth_check = Router::new()
        .route("/api/admin/verify", post(stats_api::verify_admin_secret))
        .with_state(state.clone());

    let mut app = Router::new()
        // 代理核心路由（用客户端 auth_token 认证）
        .route("/v1/messages", post(handler::handle_messages))
        .route("/v1/models", get(handler::handle_models))
        .route("/health", get(handler::health_check))
        // 管理 API（用 ADMIN_SECRET 认证）
        .nest("/api", admin_api)
        .merge(auth_check)
        // 200MB body 限制
        .layer(DefaultBodyLimit::max(200 * 1024 * 1024))
        .with_state(state);

    // 静态文件服务
    let web_dist_path = std::path::Path::new("web/dist");
    if web_dist_path.exists() && web_dist_path.is_dir() {
        let serve_dir = ServeDir::new("web/dist")
            .fallback(tower_http::services::ServeFile::new("web/dist/index.html"));
        app = app.fallback_service(serve_dir);
        log::info!("[cc-proxy] 前端静态文件服务: 已启用");
    }

    let listener = tokio::net::TcpListener::bind(&config.listen).await?;
    log::info!("[cc-proxy] 服务启动: http://{}", config.listen);
    if admin_secret.is_some() {
        log::info!("[cc-proxy] 管理界面鉴权: 已启用 (ADMIN_SECRET)");
    } else {
        log::warn!("[cc-proxy] 管理界面鉴权: 未启用（建议设置 ADMIN_SECRET 环境变量）");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    log::info!("[cc-proxy] 服务已停止");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    log::info!("[cc-proxy] 收到关闭信号，正在优雅关闭...");
}
