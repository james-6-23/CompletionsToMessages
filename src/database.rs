//! SQLite 数据库模块
//!
//! 管理使用统计数据的存储与查询
//!
//! 并发优化：读写分离架构
//! - 1 个写连接（Mutex 保护）：处理所有 INSERT/UPDATE/DELETE
//! - N 个读连接（连接池）：处理所有 SELECT 查询，互不阻塞
//! - WAL 模式：允许读写并发

use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::Arc;

/// 读连接池：多个只读连接轮询使用
struct ReadPool {
    conns: Vec<Mutex<Connection>>,
    counter: std::sync::atomic::AtomicUsize,
}

impl ReadPool {
    fn new(path: &str, size: usize) -> Result<Self, String> {
        let mut conns = Vec::with_capacity(size);
        for _ in 0..size {
            let conn = Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| format!("打开只读连接失败: {e}"))?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA query_only=ON;")
                .ok();
            conns.push(Mutex::new(conn));
        }
        Ok(Self {
            conns,
            counter: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// 轮询获取一个读连接
    fn get(&self) -> &Mutex<Connection> {
        let idx = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % self.conns.len();
        &self.conns[idx]
    }
}

/// 线程安全的数据库连接封装（读写分离）
#[derive(Clone)]
pub struct Database {
    writer: Arc<Mutex<Connection>>,
    reader: Arc<ReadPool>,
}

/// 使用统计摘要
#[derive(Debug, Serialize)]
pub struct UsageSummary {
    pub total_requests: u64,
    pub total_cost: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
}

/// 时间分桶趋势数据
#[derive(Debug, Serialize)]
pub struct UsageTrend {
    pub date: String,
    pub request_count: u64,
    pub total_cost: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
}

/// 模型维度统计
#[derive(Debug, Serialize)]
pub struct ModelStats {
    pub model: String,
    pub request_count: u64,
    pub total_cost: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub avg_latency_ms: f64,
}

/// 请求日志条目
#[derive(Debug, Serialize)]
pub struct RequestLogEntry {
    pub request_id: String,
    pub model: String,
    pub request_model: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    pub input_cost_usd: String,
    pub output_cost_usd: String,
    pub cache_read_cost_usd: String,
    pub cache_creation_cost_usd: String,
    pub total_cost_usd: String,
    pub latency_ms: u64,
    pub first_token_ms: Option<u64>,
    pub status_code: u16,
    pub is_streaming: bool,
    pub error_message: Option<String>,
    pub channel_id: String,
    pub key_id: String,
    pub created_at: i64,
}

/// 分页查询结果
#[derive(Debug, Serialize)]
pub struct PaginatedLogs {
    pub data: Vec<RequestLogEntry>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

/// 模型定价信息
#[derive(Debug, Clone, Serialize)]
pub struct ModelPricingRow {
    pub model_id: String,
    pub display_name: String,
    pub input_cost_per_million: String,
    pub output_cost_per_million: String,
    pub cache_read_cost_per_million: String,
    pub cache_creation_cost_per_million: String,
}

/// 上游端点行
#[derive(Debug, Clone, Serialize)]
pub struct EndpointRow {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub website_url: String,
    pub logo_url: String,
    pub proxy_url: String,
    pub is_active: bool,
    pub key_count: u64,
    pub models: Vec<String>,
    pub created_at: i64,
}

/// API Key 行（对外展示，密钥脱敏）
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyRow {
    pub id: String,
    pub endpoint_id: String,
    pub api_key_masked: String,
    pub label: String,
    pub is_active: bool,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
}

/// 活跃密钥（内部使用，含完整密钥值 + 所属端点 URL）
#[derive(Debug, Clone)]
pub struct ActiveKey {
    pub id: String,
    pub api_key: String,
    pub endpoint_id: String,
    pub base_url: String,
    pub proxy_url: String,
    /// 端点支持的模型列表（空 = 不限制，支持所有模型）
    pub endpoint_models: Vec<String>,
}

/// 访问密钥行（对外展示，token 脱敏）
#[derive(Debug, Clone, Serialize)]
pub struct AccessTokenRow {
    pub id: String,
    pub token_masked: String,
    pub name: String,
    pub is_active: bool,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub last_used_at: Option<i64>,
    pub channel_ids: Vec<String>,
    pub created_at: i64,
}

/// 将 API Key 脱敏，保留前 4 位和后 4 位
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "****".to_string();
    }
    let prefix = &key[..4];
    let suffix = &key[key.len() - 4..];
    format!("{}...{}", prefix, suffix)
}

impl Database {
    /// 创建数据库连接并初始化表结构
    pub fn new(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("打开数据库失败: {e}"))?;

        // 启用 WAL 模式 + 性能 PRAGMA
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=5000;
             PRAGMA cache_size=-8000;
             PRAGMA mmap_size=268435456;",
        )
        .map_err(|e| format!("设置 PRAGMA 失败: {e}"))?;

        // 创建请求日志表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS proxy_request_logs (
                request_id TEXT PRIMARY KEY,
                model TEXT NOT NULL,
                request_model TEXT,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                input_cost_usd TEXT NOT NULL DEFAULT '0',
                output_cost_usd TEXT NOT NULL DEFAULT '0',
                cache_read_cost_usd TEXT NOT NULL DEFAULT '0',
                cache_creation_cost_usd TEXT NOT NULL DEFAULT '0',
                total_cost_usd TEXT NOT NULL DEFAULT '0',
                latency_ms INTEGER NOT NULL DEFAULT 0,
                first_token_ms INTEGER,
                status_code INTEGER NOT NULL DEFAULT 200,
                is_streaming INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_logs_created_at ON proxy_request_logs(created_at);
            CREATE INDEX IF NOT EXISTS idx_logs_model ON proxy_request_logs(model);
            CREATE INDEX IF NOT EXISTS idx_logs_status ON proxy_request_logs(status_code);",
        )
        .map_err(|e| format!("创建 proxy_request_logs 表失败: {e}"))?;

        // 创建模型定价表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS model_pricing (
                model_id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                input_cost_per_million TEXT NOT NULL DEFAULT '0',
                output_cost_per_million TEXT NOT NULL DEFAULT '0',
                cache_read_cost_per_million TEXT NOT NULL DEFAULT '0',
                cache_creation_cost_per_million TEXT NOT NULL DEFAULT '0'
            );",
        )
        .map_err(|e| format!("创建 model_pricing 表失败: {e}"))?;

        // 预填充常见模型定价（使用 INSERT OR IGNORE 避免重复）
        let pricing_data = vec![
            ("gpt-4o", "GPT-4o", "2.50", "10.00", "1.25", "3.75"),
            (
                "gpt-4o-mini",
                "GPT-4o Mini",
                "0.15",
                "0.60",
                "0.075",
                "0.225",
            ),
            ("o3", "o3", "10.00", "40.00", "5.00", "15.00"),
            ("o3-mini", "o3-mini", "1.10", "4.40", "0.55", "1.65"),
            ("o4-mini", "o4-mini", "1.10", "4.40", "0.55", "1.65"),
            ("gpt-5", "GPT-5", "10.00", "40.00", "5.00", "15.00"),
        ];

        for (model_id, display_name, input, output, cache_read, cache_creation) in pricing_data {
            conn.execute(
                "INSERT OR IGNORE INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![model_id, display_name, input, output, cache_read, cache_creation],
            ).map_err(|e| format!("插入模型定价失败: {e}"))?;
        }

        // 创建上游端点表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS upstream_endpoints (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                base_url TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            );",
        )
        .map_err(|e| format!("创建 upstream_endpoints 表失败: {e}"))?;

        // 迁移：为 upstream_endpoints 添加 models 列（JSON 数组）
        {
            let has_col: bool = conn
                .prepare("PRAGMA table_info(upstream_endpoints)")
                .and_then(|mut stmt| {
                    let names: Vec<String> = stmt
                        .query_map([], |row| row.get::<_, String>(1))
                        .unwrap()
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(names.contains(&"models".to_string()))
                })
                .unwrap_or(false);

            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE upstream_endpoints ADD COLUMN models TEXT NOT NULL DEFAULT '[]';",
                )
                .map_err(|e| format!("迁移 upstream_endpoints 添加 models 列失败: {e}"))?;
                log::info!("[cc-proxy] 已迁移 upstream_endpoints 表，添加 models 列");
            }
        }

        // 迁移：为 upstream_endpoints 添加 website_url 列
        {
            let has_col: bool = conn
                .prepare("PRAGMA table_info(upstream_endpoints)")
                .and_then(|mut stmt| {
                    let names: Vec<String> = stmt
                        .query_map([], |row| row.get::<_, String>(1))
                        .unwrap()
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(names.contains(&"website_url".to_string()))
                })
                .unwrap_or(false);

            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE upstream_endpoints ADD COLUMN website_url TEXT NOT NULL DEFAULT '';"
                ).map_err(|e| format!("迁移 upstream_endpoints 添加 website_url 列失败: {e}"))?;
                log::info!("[cc-proxy] 已迁移 upstream_endpoints 表，添加 website_url 列");
            }
        }

        // 迁移：为 upstream_endpoints 添加 logo_url 列
        {
            let has_col: bool = conn
                .prepare("PRAGMA table_info(upstream_endpoints)")
                .and_then(|mut stmt| {
                    let names: Vec<String> = stmt
                        .query_map([], |row| row.get::<_, String>(1))
                        .unwrap()
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(names.contains(&"logo_url".to_string()))
                })
                .unwrap_or(false);

            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE upstream_endpoints ADD COLUMN logo_url TEXT NOT NULL DEFAULT '';",
                )
                .map_err(|e| format!("迁移 upstream_endpoints 添加 logo_url 列失败: {e}"))?;
                log::info!("[cc-proxy] 已迁移 upstream_endpoints 表，添加 logo_url 列");
            }
        }

        // 迁移：为 upstream_endpoints 添加 proxy_url 列
        {
            let has_col: bool = conn
                .prepare("PRAGMA table_info(upstream_endpoints)")
                .and_then(|mut stmt| {
                    let names: Vec<String> = stmt
                        .query_map([], |row| row.get::<_, String>(1))
                        .unwrap()
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(names.contains(&"proxy_url".to_string()))
                })
                .unwrap_or(false);

            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE upstream_endpoints ADD COLUMN proxy_url TEXT NOT NULL DEFAULT '';",
                )
                .map_err(|e| format!("迁移 upstream_endpoints 添加 proxy_url 列失败: {e}"))?;
                log::info!("[cc-proxy] 已迁移 upstream_endpoints 表，添加 proxy_url 列");
            }
        }

        // 创建 API 密钥管理表（含 endpoint_id 外键）
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                endpoint_id TEXT NOT NULL DEFAULT '',
                api_key TEXT NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                is_active INTEGER NOT NULL DEFAULT 1,
                total_requests INTEGER NOT NULL DEFAULT 0,
                failed_requests INTEGER NOT NULL DEFAULT 0,
                last_used_at INTEGER,
                created_at INTEGER NOT NULL
            );",
        )
        .map_err(|e| format!("创建 api_keys 表失败: {e}"))?;

        // 迁移：如果 api_keys 表缺少 endpoint_id 列，添加之
        {
            let has_col: bool = conn
                .prepare("PRAGMA table_info(api_keys)")
                .and_then(|mut stmt| {
                    let names: Vec<String> = stmt
                        .query_map([], |row| row.get::<_, String>(1))
                        .unwrap()
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(names.contains(&"endpoint_id".to_string()))
                })
                .unwrap_or(false);

            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE api_keys ADD COLUMN endpoint_id TEXT NOT NULL DEFAULT '';",
                )
                .map_err(|e| format!("迁移 api_keys 添加 endpoint_id 列失败: {e}"))?;
                log::info!("[cc-proxy] 已迁移 api_keys 表，添加 endpoint_id 列");
            }
        }

        // 迁移：为 proxy_request_logs 添加 channel_id 列
        {
            let has_col: bool = conn
                .prepare("PRAGMA table_info(proxy_request_logs)")
                .and_then(|mut stmt| {
                    let names: Vec<String> = stmt
                        .query_map([], |row| row.get::<_, String>(1))
                        .unwrap()
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(names.contains(&"channel_id".to_string()))
                })
                .unwrap_or(false);

            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE proxy_request_logs ADD COLUMN channel_id TEXT NOT NULL DEFAULT '';"
                ).map_err(|e| format!("迁移 proxy_request_logs 添加 channel_id 列失败: {e}"))?;
                log::info!("[cc-proxy] 已迁移 proxy_request_logs 表，添加 channel_id 列");
            }
        }

        // 迁移：为 proxy_request_logs 添加 key_id 列
        {
            let has_col: bool = conn
                .prepare("PRAGMA table_info(proxy_request_logs)")
                .and_then(|mut stmt| {
                    let names: Vec<String> = stmt
                        .query_map([], |row| row.get::<_, String>(1))
                        .unwrap()
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(names.contains(&"key_id".to_string()))
                })
                .unwrap_or(false);

            if !has_col {
                conn.execute_batch(
                    "ALTER TABLE proxy_request_logs ADD COLUMN key_id TEXT NOT NULL DEFAULT '';",
                )
                .map_err(|e| format!("迁移 proxy_request_logs 添加 key_id 列失败: {e}"))?;
                log::info!("[cc-proxy] 已迁移 proxy_request_logs 表，添加 key_id 列");
            }
        }

        // 迁移：将旧的 proxy_settings.upstream_base_url 迁移到 upstream_endpoints 表
        {
            let old_url: Option<String> = conn
                .query_row(
                    "SELECT value FROM proxy_settings WHERE key = 'upstream_base_url'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .ok();

            if let Some(url) = old_url.filter(|u| !u.is_empty()) {
                // 检查 endpoints 表是否为空
                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM upstream_endpoints", [], |row| {
                        row.get(0)
                    })
                    .unwrap_or(0);

                if count == 0 {
                    let ep_id = uuid::Uuid::new_v4().to_string();
                    let now = chrono::Utc::now().timestamp();
                    conn.execute(
                        "INSERT INTO upstream_endpoints (id, name, base_url, is_active, created_at) VALUES (?1, ?2, ?3, 1, ?4)",
                        params![ep_id, "默认端点", url, now],
                    ).map_err(|e| format!("迁移上游端点失败: {e}"))?;

                    // 将所有无 endpoint_id 的 key 绑到这个端点
                    conn.execute(
                        "UPDATE api_keys SET endpoint_id = ?1 WHERE endpoint_id = ''",
                        params![ep_id],
                    )
                    .map_err(|e| format!("迁移密钥端点绑定失败: {e}"))?;

                    // 删除旧设置
                    conn.execute(
                        "DELETE FROM proxy_settings WHERE key = 'upstream_base_url'",
                        [],
                    )
                    .ok();

                    log::info!("[cc-proxy] 已迁移旧上游 URL 到 upstream_endpoints 表");
                }
            }
        }

        // 创建代理设置表（KV 存储）
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS proxy_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .map_err(|e| format!("创建 proxy_settings 表失败: {e}"))?;

        // 创建访问密钥表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS access_tokens (
                id TEXT PRIMARY KEY,
                token TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL DEFAULT '',
                is_active INTEGER NOT NULL DEFAULT 1,
                total_requests INTEGER NOT NULL DEFAULT 0,
                failed_requests INTEGER NOT NULL DEFAULT 0,
                last_used_at INTEGER,
                created_at INTEGER NOT NULL
            );",
        )
        .map_err(|e| format!("创建 access_tokens 表失败: {e}"))?;

        // 创建访问密钥-渠道关联表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS access_token_channels (
                access_token_id TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                PRIMARY KEY (access_token_id, channel_id)
            );",
        )
        .map_err(|e| format!("创建 access_token_channels 表失败: {e}"))?;

        // 迁移：将旧的 proxy_settings.auth_token 迁移到 access_tokens 表
        {
            let token_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM access_tokens", [], |row| row.get(0))
                .unwrap_or(0);

            if token_count == 0 {
                let old_token: Option<String> = conn
                    .query_row(
                        "SELECT value FROM proxy_settings WHERE key = 'auth_token'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .ok();

                if let Some(token_val) = old_token.filter(|t| !t.is_empty()) {
                    let at_id = uuid::Uuid::new_v4().to_string();
                    let now = chrono::Utc::now().timestamp();

                    conn.execute(
                        "INSERT INTO access_tokens (id, token, name, is_active, total_requests, failed_requests, last_used_at, created_at) VALUES (?1, ?2, ?3, 1, 0, 0, NULL, ?4)",
                        params![at_id, token_val, "迁移密钥", now],
                    ).map_err(|e| format!("迁移 auth_token 到 access_tokens 失败: {e}"))?;

                    // 绑定到所有现有渠道
                    let mut ep_stmt = conn
                        .prepare("SELECT id FROM upstream_endpoints")
                        .map_err(|e| format!("查询端点列表失败: {e}"))?;
                    let ep_ids: Vec<String> = ep_stmt
                        .query_map([], |row| row.get::<_, String>(0))
                        .map_err(|e| format!("查询端点失败: {e}"))?
                        .filter_map(|r| r.ok())
                        .collect();

                    for ep_id in &ep_ids {
                        conn.execute(
                            "INSERT OR IGNORE INTO access_token_channels (access_token_id, channel_id) VALUES (?1, ?2)",
                            params![at_id, ep_id],
                        ).map_err(|e| format!("插入访问密钥-渠道关联失败: {e}"))?;
                    }

                    // 删除旧设置
                    conn.execute("DELETE FROM proxy_settings WHERE key = 'auth_token'", [])
                        .ok();

                    log::info!(
                        "[cc-proxy] 已迁移旧 auth_token 到 access_tokens 表，绑定 {} 个渠道",
                        ep_ids.len()
                    );
                }
            }
        }

        Ok(Self {
            writer: Arc::new(Mutex::new(conn)),
            reader: Arc::new(ReadPool::new(path, 4)?),
        })
    }

    /// 插入一条请求日志
    pub fn insert_request_log(
        &self,
        request_id: &str,
        model: &str,
        request_model: Option<&str>,
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
        cache_creation_tokens: u32,
        input_cost_usd: &str,
        output_cost_usd: &str,
        cache_read_cost_usd: &str,
        cache_creation_cost_usd: &str,
        total_cost_usd: &str,
        latency_ms: u64,
        first_token_ms: Option<u64>,
        status_code: u16,
        is_streaming: bool,
        error_message: Option<&str>,
        channel_id: &str,
        key_id: &str,
        created_at: i64,
    ) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "INSERT INTO proxy_request_logs (
                request_id, model, request_model, input_tokens, output_tokens,
                cache_read_tokens, cache_creation_tokens,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                latency_ms, first_token_ms, status_code, is_streaming, error_message, channel_id, key_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                request_id, model, request_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                latency_ms as i64, first_token_ms.map(|v| v as i64),
                status_code as i32, is_streaming as i32, error_message, channel_id, key_id, created_at
            ],
        ).map_err(|e| format!("插入请求日志失败: {e}"))?;
        Ok(())
    }

    /// 查询使用统计摘要
    pub fn get_usage_summary(
        &self,
        start_ts: i64,
        end_ts: i64,
        channel_id_filter: Option<&str>,
    ) -> Result<UsageSummary, String> {
        let conn = self.reader.get().lock();

        let mut sql = "SELECT
                COUNT(*) as total_requests,
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0) as total_cost,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(cache_creation_tokens), 0) as total_cache_creation_tokens,
                COALESCE(SUM(cache_read_tokens), 0) as total_cache_read_tokens
            FROM proxy_request_logs
            WHERE created_at >= ?1 AND created_at < ?2"
            .to_string();

        if channel_id_filter.is_some() {
            sql.push_str(" AND channel_id = ?3");
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("准备查询失败: {e}"))?;

        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<UsageSummary> {
            Ok(UsageSummary {
                total_requests: row.get::<_, i64>(0)? as u64,
                total_cost: format!("{:.6}", row.get::<_, f64>(1)?),
                total_input_tokens: row.get::<_, i64>(2)? as u64,
                total_output_tokens: row.get::<_, i64>(3)? as u64,
                total_cache_creation_tokens: row.get::<_, i64>(4)? as u64,
                total_cache_read_tokens: row.get::<_, i64>(5)? as u64,
            })
        };

        let result = if let Some(ch) = channel_id_filter {
            stmt.query_row(params![start_ts, end_ts, ch], map_row)
        } else {
            stmt.query_row(params![start_ts, end_ts], map_row)
        }
        .map_err(|e| format!("查询使用摘要失败: {e}"))?;

        Ok(result)
    }

    /// 查询使用趋势（按时间分桶）
    pub fn get_usage_trends(
        &self,
        start_ts: i64,
        end_ts: i64,
        interval_secs: i64,
        channel_id_filter: Option<&str>,
    ) -> Result<Vec<UsageTrend>, String> {
        let conn = self.reader.get().lock();

        let mut sql = "SELECT
                (created_at / ?3) * ?3 as bucket,
                COUNT(*) as request_count,
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0) as total_cost,
                COALESCE(SUM(input_tokens), 0) as input_tokens,
                COALESCE(SUM(output_tokens), 0) as output_tokens,
                COALESCE(SUM(cache_creation_tokens), 0) as cache_creation_tokens,
                COALESCE(SUM(cache_read_tokens), 0) as cache_read_tokens
            FROM proxy_request_logs
            WHERE created_at >= ?1 AND created_at < ?2"
            .to_string();

        if channel_id_filter.is_some() {
            sql.push_str(" AND channel_id = ?4");
        }

        sql.push_str(" GROUP BY bucket ORDER BY bucket ASC");

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("准备查询失败: {e}"))?;

        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<UsageTrend> {
            let bucket_ts = row.get::<_, i64>(0)?;
            let date = chrono::DateTime::from_timestamp(bucket_ts, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| bucket_ts.to_string());

            Ok(UsageTrend {
                date,
                request_count: row.get::<_, i64>(1)? as u64,
                total_cost: format!("{:.6}", row.get::<_, f64>(2)?),
                total_input_tokens: row.get::<_, i64>(3)? as u64,
                total_output_tokens: row.get::<_, i64>(4)? as u64,
                total_cache_creation_tokens: row.get::<_, i64>(5)? as u64,
                total_cache_read_tokens: row.get::<_, i64>(6)? as u64,
            })
        };

        let rows = if let Some(ch) = channel_id_filter {
            stmt.query_map(params![start_ts, end_ts, interval_secs, ch], map_row)
        } else {
            stmt.query_map(params![start_ts, end_ts, interval_secs], map_row)
        }
        .map_err(|e| format!("查询使用趋势失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取行数据失败: {e}"))?);
        }
        Ok(result)
    }

    /// 查询模型维度统计
    pub fn get_model_stats(&self, start_ts: i64, end_ts: i64) -> Result<Vec<ModelStats>, String> {
        let conn = self.reader.get().lock();
        let mut stmt = conn
            .prepare(
                "SELECT
                model,
                COUNT(*) as request_count,
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0) as total_cost,
                COALESCE(SUM(input_tokens), 0) as input_tokens,
                COALESCE(SUM(output_tokens), 0) as output_tokens,
                COALESCE(SUM(cache_creation_tokens), 0) as cache_creation_tokens,
                COALESCE(SUM(cache_read_tokens), 0) as cache_read_tokens,
                COALESCE(AVG(latency_ms), 0) as avg_latency_ms
            FROM proxy_request_logs
            WHERE created_at >= ?1 AND created_at < ?2
            GROUP BY model
            ORDER BY total_cost DESC",
            )
            .map_err(|e| format!("准备查询失败: {e}"))?;

        let rows = stmt
            .query_map(params![start_ts, end_ts], |row| {
                Ok(ModelStats {
                    model: row.get(0)?,
                    request_count: row.get::<_, i64>(1)? as u64,
                    total_cost: format!("{:.6}", row.get::<_, f64>(2)?),
                    input_tokens: row.get::<_, i64>(3)? as u64,
                    output_tokens: row.get::<_, i64>(4)? as u64,
                    cache_creation_tokens: row.get::<_, i64>(5)? as u64,
                    cache_read_tokens: row.get::<_, i64>(6)? as u64,
                    avg_latency_ms: row.get(7)?,
                })
            })
            .map_err(|e| format!("查询模型统计失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取行数据失败: {e}"))?);
        }
        Ok(result)
    }

    /// 查询分页请求日志
    pub fn get_request_logs(
        &self,
        page: u32,
        page_size: u32,
        status_code_filter: Option<u16>,
        model_filter: Option<&str>,
        channel_id_filter: Option<&str>,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<PaginatedLogs, String> {
        let conn = self.reader.get().lock();

        // 构建动态 WHERE 条件
        let mut conditions = vec!["created_at >= ?1 AND created_at < ?2".to_string()];
        let mut param_index = 3;

        if status_code_filter.is_some() {
            conditions.push(format!("status_code = ?{param_index}"));
            param_index += 1;
        }
        if model_filter.is_some() {
            conditions.push(format!("model LIKE ?{param_index}"));
            param_index += 1;
        }
        if channel_id_filter.is_some() {
            conditions.push(format!("channel_id = ?{param_index}"));
            param_index += 1;
        }

        let where_clause = conditions.join(" AND ");

        // 收集动态参数
        let mut dynamic_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        dynamic_params.push(Box::new(start_ts));
        dynamic_params.push(Box::new(end_ts));
        if let Some(sc) = status_code_filter {
            dynamic_params.push(Box::new(sc as i32));
        }
        if let Some(ref m) = model_filter {
            dynamic_params.push(Box::new(format!("%{m}%")));
        }
        if let Some(ch) = channel_id_filter {
            dynamic_params.push(Box::new(ch.to_string()));
        }

        // 查询总数
        let count_sql = format!("SELECT COUNT(*) FROM proxy_request_logs WHERE {where_clause}");
        let total = {
            let mut stmt = conn
                .prepare(&count_sql)
                .map_err(|e| format!("准备计数查询失败: {e}"))?;

            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                dynamic_params.iter().map(|p| p.as_ref()).collect();
            let total: i64 = stmt
                .query_row(param_refs.as_slice(), |row| row.get(0))
                .map_err(|e| format!("计数查询失败: {e}"))?;
            total as u64
        };

        // 查询分页数据
        let offset = (page.saturating_sub(1)) * page_size;
        let query_sql = format!(
            "SELECT request_id, model, request_model, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens,
                    input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                    latency_ms, first_token_ms, status_code, is_streaming, error_message, channel_id, key_id, created_at
            FROM proxy_request_logs
            WHERE {where_clause}
            ORDER BY created_at DESC
            LIMIT ?{} OFFSET ?{}",
            param_index, param_index + 1
        );

        let mut stmt = conn
            .prepare(&query_sql)
            .map_err(|e| format!("准备日志查询失败: {e}"))?;

        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<RequestLogEntry> {
            Ok(RequestLogEntry {
                request_id: row.get(0)?,
                model: row.get(1)?,
                request_model: row.get(2)?,
                input_tokens: row.get::<_, i32>(3)? as u32,
                output_tokens: row.get::<_, i32>(4)? as u32,
                cache_read_tokens: row.get::<_, i32>(5)? as u32,
                cache_creation_tokens: row.get::<_, i32>(6)? as u32,
                input_cost_usd: row.get(7)?,
                output_cost_usd: row.get(8)?,
                cache_read_cost_usd: row.get(9)?,
                cache_creation_cost_usd: row.get(10)?,
                total_cost_usd: row.get(11)?,
                latency_ms: row.get::<_, i64>(12)? as u64,
                first_token_ms: row.get::<_, Option<i64>>(13)?.map(|v| v as u64),
                status_code: row.get::<_, i32>(14)? as u16,
                is_streaming: row.get::<_, i32>(15)? != 0,
                error_message: row.get(16)?,
                channel_id: row.get(17)?,
                key_id: row.get(18)?,
                created_at: row.get(19)?,
            })
        };

        // 追加分页参数
        let mut query_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        query_params.push(Box::new(start_ts));
        query_params.push(Box::new(end_ts));
        if let Some(sc) = status_code_filter {
            query_params.push(Box::new(sc as i32));
        }
        if let Some(ref m) = model_filter {
            query_params.push(Box::new(format!("%{m}%")));
        }
        if let Some(ch) = channel_id_filter {
            query_params.push(Box::new(ch.to_string()));
        }
        query_params.push(Box::new(page_size));
        query_params.push(Box::new(offset));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            query_params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(param_refs.as_slice(), map_row)
            .map_err(|e| format!("日志查询失败: {e}"))?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row.map_err(|e| format!("读取日志行失败: {e}"))?);
        }

        Ok(PaginatedLogs {
            data: logs,
            total,
            page,
            page_size,
        })
    }

    /// 查询所有模型定价
    pub fn get_model_pricing(&self) -> Result<Vec<ModelPricingRow>, String> {
        let conn = self.reader.get().lock();
        let mut stmt = conn
            .prepare(
                "SELECT model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
            FROM model_pricing
            ORDER BY model_id",
            )
            .map_err(|e| format!("准备定价查询失败: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(ModelPricingRow {
                    model_id: row.get(0)?,
                    display_name: row.get(1)?,
                    input_cost_per_million: row.get(2)?,
                    output_cost_per_million: row.get(3)?,
                    cache_read_cost_per_million: row.get(4)?,
                    cache_creation_cost_per_million: row.get(5)?,
                })
            })
            .map_err(|e| format!("定价查询失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取定价行失败: {e}"))?);
        }
        Ok(result)
    }

    /// 根据模型 ID 查询单个定价信息
    pub fn get_pricing_for_model(&self, model: &str) -> Result<Option<ModelPricingRow>, String> {
        let conn = self.reader.get().lock();
        let mut stmt = conn
            .prepare(
                "SELECT model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
            FROM model_pricing
            WHERE model_id = ?1",
            )
            .map_err(|e| format!("准备定价查询失败: {e}"))?;

        let result = stmt.query_row(params![model], |row| {
            Ok(ModelPricingRow {
                model_id: row.get(0)?,
                display_name: row.get(1)?,
                input_cost_per_million: row.get(2)?,
                output_cost_per_million: row.get(3)?,
                cache_read_cost_per_million: row.get(4)?,
                cache_creation_cost_per_million: row.get(5)?,
            })
        });

        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("查询模型定价失败: {e}")),
        }
    }

    // ===== 上游端点管理 =====

    /// 查询所有上游端点（含每个端点的 key 数量）
    pub fn list_endpoints(&self) -> Result<Vec<EndpointRow>, String> {
        let conn = self.reader.get().lock();
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.name, e.base_url, e.is_active, e.created_at,
                    (SELECT COUNT(*) FROM api_keys k WHERE k.endpoint_id = e.id) as key_count,
                    e.models, e.website_url, e.logo_url, e.proxy_url
            FROM upstream_endpoints e
            ORDER BY e.created_at ASC",
            )
            .map_err(|e| format!("准备端点查询失败: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                let models_json: String =
                    row.get::<_, String>(6).unwrap_or_else(|_| "[]".to_string());
                let models: Vec<String> = serde_json::from_str(&models_json).unwrap_or_default();
                Ok(EndpointRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_url: row.get(2)?,
                    is_active: row.get::<_, i32>(3)? != 0,
                    key_count: row.get::<_, i64>(5)? as u64,
                    models,
                    created_at: row.get(4)?,
                    website_url: row.get::<_, String>(7).unwrap_or_default(),
                    logo_url: row.get::<_, String>(8).unwrap_or_default(),
                    proxy_url: row.get::<_, String>(9).unwrap_or_default(),
                })
            })
            .map_err(|e| format!("查询端点失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取端点行失败: {e}"))?);
        }
        Ok(result)
    }

    /// 添加上游端点
    pub fn add_endpoint(
        &self,
        name: &str,
        base_url: &str,
        website_url: &str,
        logo_url: &str,
        proxy_url: &str,
    ) -> Result<EndpointRow, String> {
        let conn = self.writer.lock();
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT INTO upstream_endpoints (id, name, base_url, website_url, logo_url, proxy_url, is_active, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)",
            params![id, name, base_url, website_url, logo_url, proxy_url, created_at],
        ).map_err(|e| format!("插入端点失败: {e}"))?;

        Ok(EndpointRow {
            id,
            name: name.to_string(),
            base_url: base_url.to_string(),
            website_url: website_url.to_string(),
            logo_url: logo_url.to_string(),
            proxy_url: proxy_url.to_string(),
            is_active: true,
            key_count: 0,
            models: vec![],
            created_at,
        })
    }

    /// 更新上游端点
    pub fn update_endpoint(
        &self,
        id: &str,
        name: &str,
        base_url: &str,
        website_url: &str,
        logo_url: &str,
        proxy_url: &str,
    ) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "UPDATE upstream_endpoints SET name = ?1, base_url = ?2, website_url = ?3, logo_url = ?4, proxy_url = ?5 WHERE id = ?6",
            params![name, base_url, website_url, logo_url, proxy_url, id],
        ).map_err(|e| format!("更新端点失败: {e}"))?;
        Ok(())
    }

    /// 更新上游端点启用状态
    pub fn update_endpoint_status(&self, id: &str, is_active: bool) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "UPDATE upstream_endpoints SET is_active = ?1 WHERE id = ?2",
            params![is_active as i32, id],
        )
        .map_err(|e| format!("更新端点状态失败: {e}"))?;
        Ok(())
    }

    /// 删除上游端点（同时删除关联的所有 key）
    pub fn delete_endpoint(&self, id: &str) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute("DELETE FROM api_keys WHERE endpoint_id = ?1", params![id])
            .map_err(|e| format!("删除端点关联密钥失败: {e}"))?;
        conn.execute("DELETE FROM upstream_endpoints WHERE id = ?1", params![id])
            .map_err(|e| format!("删除端点失败: {e}"))?;
        Ok(())
    }

    /// 获取单个端点的 base_url
    pub fn get_endpoint_url(&self, id: &str) -> Result<Option<String>, String> {
        let conn = self.reader.get().lock();
        let result = conn.query_row(
            "SELECT base_url FROM upstream_endpoints WHERE id = ?1 AND is_active = 1",
            params![id],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(url) => Ok(Some(url)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("查询端点 URL 失败: {e}")),
        }
    }

    /// 更新端点支持的模型列表
    pub fn update_endpoint_models(&self, id: &str, models: &[String]) -> Result<(), String> {
        let conn = self.writer.lock();
        let models_json =
            serde_json::to_string(models).map_err(|e| format!("序列化模型列表失败: {e}"))?;
        conn.execute(
            "UPDATE upstream_endpoints SET models = ?1 WHERE id = ?2",
            params![models_json, id],
        )
        .map_err(|e| format!("更新端点模型列表失败: {e}"))?;
        Ok(())
    }

    // ===== API Key 管理方法 =====

    /// 查询所有 API Key（脱敏），可按端点过滤
    pub fn list_api_keys(&self, endpoint_id: Option<&str>) -> Result<Vec<ApiKeyRow>, String> {
        let conn = self.reader.get().lock();

        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<ApiKeyRow> {
            let raw_key: String = row.get(2)?;
            Ok(ApiKeyRow {
                id: row.get(0)?,
                endpoint_id: row.get(1)?,
                api_key_masked: mask_api_key(&raw_key),
                label: row.get(3)?,
                is_active: row.get::<_, i32>(4)? != 0,
                total_requests: row.get::<_, i64>(5)? as u64,
                failed_requests: row.get::<_, i64>(6)? as u64,
                last_used_at: row.get(7)?,
                created_at: row.get(8)?,
            })
        };

        let mut result = Vec::new();

        if let Some(eid) = endpoint_id {
            let mut stmt = conn.prepare(
                "SELECT id, endpoint_id, api_key, label, is_active, total_requests, failed_requests, last_used_at, created_at
                FROM api_keys WHERE endpoint_id = ?1
                ORDER BY created_at DESC"
            ).map_err(|e| format!("准备 api_keys 查询失败: {e}"))?;

            let rows = stmt
                .query_map(params![eid], map_row)
                .map_err(|e| format!("查询 api_keys 失败: {e}"))?;
            for row in rows {
                result.push(row.map_err(|e| format!("读取 api_key 行失败: {e}"))?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, endpoint_id, api_key, label, is_active, total_requests, failed_requests, last_used_at, created_at
                FROM api_keys
                ORDER BY created_at DESC"
            ).map_err(|e| format!("准备 api_keys 查询失败: {e}"))?;

            let rows = stmt
                .query_map([], map_row)
                .map_err(|e| format!("查询 api_keys 失败: {e}"))?;
            for row in rows {
                result.push(row.map_err(|e| format!("读取 api_key 行失败: {e}"))?);
            }
        }

        Ok(result)
    }

    /// 添加 API Key（绑定到指定端点）
    pub fn add_api_key(
        &self,
        endpoint_id: &str,
        api_key: &str,
        label: &str,
    ) -> Result<ApiKeyRow, String> {
        let conn = self.writer.lock();
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT INTO api_keys (id, endpoint_id, api_key, label, is_active, total_requests, failed_requests, last_used_at, created_at)
            VALUES (?1, ?2, ?3, ?4, 1, 0, 0, NULL, ?5)",
            params![id, endpoint_id, api_key, label, created_at],
        ).map_err(|e| format!("插入 api_key 失败: {e}"))?;

        Ok(ApiKeyRow {
            id,
            endpoint_id: endpoint_id.to_string(),
            api_key_masked: mask_api_key(api_key),
            label: label.to_string(),
            is_active: true,
            total_requests: 0,
            failed_requests: 0,
            last_used_at: None,
            created_at,
        })
    }

    /// 批量添加 API Key（单事务，高性能）
    pub fn add_api_keys_batch(
        &self,
        endpoint_id: &str,
        api_keys: &[String],
    ) -> Result<Vec<ApiKeyRow>, String> {
        let conn = self.writer.lock();
        let created_at = chrono::Utc::now().timestamp();

        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| format!("开始事务失败: {e}"))?;

        let mut rows = Vec::with_capacity(api_keys.len());
        for key in api_keys {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            if let Err(e) = conn.execute(
                "INSERT INTO api_keys (id, endpoint_id, api_key, label, is_active, total_requests, failed_requests, last_used_at, created_at)
                VALUES (?1, ?2, ?3, '', 1, 0, 0, NULL, ?4)",
                params![id, endpoint_id, key, created_at],
            ) {
                conn.execute_batch("ROLLBACK").ok();
                return Err(format!("批量插入 api_key 失败: {e}"));
            }
            rows.push(ApiKeyRow {
                id,
                endpoint_id: endpoint_id.to_string(),
                api_key_masked: mask_api_key(key),
                label: String::new(),
                is_active: true,
                total_requests: 0,
                failed_requests: 0,
                last_used_at: None,
                created_at,
            });
        }

        conn.execute_batch("COMMIT")
            .map_err(|e| format!("提交事务失败: {e}"))?;

        Ok(rows)
    }

    /// 删除 API Key
    pub fn delete_api_key(&self, id: &str) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute("DELETE FROM api_keys WHERE id = ?1", params![id])
            .map_err(|e| format!("删除 api_key 失败: {e}"))?;
        Ok(())
    }

    /// 批量删除端点下的密钥（可按状态过滤）
    pub fn delete_keys_by_endpoint(
        &self,
        endpoint_id: &str,
        status_filter: Option<bool>,
    ) -> Result<u64, String> {
        let conn = self.writer.lock();
        let affected = if let Some(active) = status_filter {
            conn.execute(
                "DELETE FROM api_keys WHERE endpoint_id = ?1 AND is_active = ?2",
                params![endpoint_id, active as i32],
            )
        } else {
            conn.execute(
                "DELETE FROM api_keys WHERE endpoint_id = ?1",
                params![endpoint_id],
            )
        }
        .map_err(|e| format!("批量删除密钥失败: {e}"))?;
        Ok(affected as u64)
    }

    /// 批量恢复端点下的失效密钥
    pub fn restore_invalid_keys(&self, endpoint_id: &str) -> Result<u64, String> {
        let conn = self.writer.lock();
        let affected = conn
            .execute(
                "UPDATE api_keys SET is_active = 1 WHERE endpoint_id = ?1 AND is_active = 0",
                params![endpoint_id],
            )
            .map_err(|e| format!("恢复失效密钥失败: {e}"))?;
        Ok(affected as u64)
    }

    /// 导出端点下的密钥（完整值），可按状态过滤
    pub fn export_keys(
        &self,
        endpoint_id: &str,
        status_filter: Option<bool>,
    ) -> Result<Vec<String>, String> {
        let conn = self.reader.get().lock();
        let mut result = Vec::new();

        if let Some(active) = status_filter {
            let mut stmt = conn
                .prepare("SELECT api_key FROM api_keys WHERE endpoint_id = ?1 AND is_active = ?2 ORDER BY created_at ASC")
                .map_err(|e| format!("准备导出查询失败: {e}"))?;
            let rows = stmt
                .query_map(params![endpoint_id, active as i32], |row| row.get::<_, String>(0))
                .map_err(|e| format!("导出密钥失败: {e}"))?;
            for row in rows {
                result.push(row.map_err(|e| format!("读取密钥失败: {e}"))?);
            }
        } else {
            let mut stmt = conn
                .prepare("SELECT api_key FROM api_keys WHERE endpoint_id = ?1 ORDER BY created_at ASC")
                .map_err(|e| format!("准备导出查询失败: {e}"))?;
            let rows = stmt
                .query_map(params![endpoint_id], |row| row.get::<_, String>(0))
                .map_err(|e| format!("导出密钥失败: {e}"))?;
            for row in rows {
                result.push(row.map_err(|e| format!("读取密钥失败: {e}"))?);
            }
        }

        Ok(result)
    }

    /// 更新 API Key 启用状态
    pub fn update_api_key_status(&self, id: &str, is_active: bool) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "UPDATE api_keys SET is_active = ?1 WHERE id = ?2",
            params![is_active as i32, id],
        )
        .map_err(|e| format!("更新 api_key 状态失败: {e}"))?;
        Ok(())
    }

    /// 更新 API Key 标签
    #[allow(dead_code)]
    pub fn update_api_key_label(&self, id: &str, label: &str) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "UPDATE api_keys SET label = ?1 WHERE id = ?2",
            params![label, id],
        )
        .map_err(|e| format!("更新 api_key 标签失败: {e}"))?;
        Ok(())
    }

    /// 递增 API Key 使用统计
    pub fn increment_key_stats(&self, id: &str, success: bool) -> Result<(), String> {
        let conn = self.writer.lock();
        let now = chrono::Utc::now().timestamp();
        let failed_inc = if success { 0 } else { 1 };

        conn.execute(
            "UPDATE api_keys SET total_requests = total_requests + 1, failed_requests = failed_requests + ?1, last_used_at = ?2 WHERE id = ?3",
            params![failed_inc, now, id],
        ).map_err(|e| format!("更新 api_key 统计失败: {e}"))?;
        Ok(())
    }

    /// 获取所有活跃密钥（完整密钥值 + 所属端点 URL，内部使用）
    ///
    /// 只返回所属端点也处于活跃状态的密钥
    #[allow(dead_code)]
    pub fn get_all_active_keys(&self) -> Result<Vec<ActiveKey>, String> {
        let conn = self.reader.get().lock();
        let mut stmt = conn
            .prepare(
                "SELECT k.id, k.api_key, k.endpoint_id, e.base_url, e.models, e.proxy_url
            FROM api_keys k
            INNER JOIN upstream_endpoints e ON k.endpoint_id = e.id
            WHERE k.is_active = 1 AND e.is_active = 1
            ORDER BY k.created_at ASC",
            )
            .map_err(|e| format!("准备活跃密钥查询失败: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                let models_json: String =
                    row.get::<_, String>(4).unwrap_or_else(|_| "[]".to_string());
                let endpoint_models: Vec<String> =
                    serde_json::from_str(&models_json).unwrap_or_default();
                Ok(ActiveKey {
                    id: row.get(0)?,
                    api_key: row.get(1)?,
                    endpoint_id: row.get(2)?,
                    base_url: row.get(3)?,
                    proxy_url: row.get::<_, String>(5).unwrap_or_default(),
                    endpoint_models,
                })
            })
            .map_err(|e| format!("查询活跃密钥失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取活跃密钥行失败: {e}"))?);
        }
        Ok(result)
    }

    /// 获取完整 API Key 及其端点 URL（用于测试密钥等场景）
    pub fn get_api_key_full(&self, id: &str) -> Result<Option<(String, String)>, String> {
        let conn = self.reader.get().lock();
        let result = conn.query_row(
            "SELECT k.api_key, e.base_url
            FROM api_keys k
            LEFT JOIN upstream_endpoints e ON k.endpoint_id = e.id
            WHERE k.id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1).unwrap_or_default(),
                ))
            },
        );

        match result {
            Ok(pair) => Ok(Some(pair)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("查询密钥失败: {e}")),
        }
    }

    // ===== 代理设置（KV 存储） =====

    /// 获取设置值
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.reader.get().lock();
        let result = conn.query_row(
            "SELECT value FROM proxy_settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("查询设置失败: {e}")),
        }
    }

    /// 设置值（upsert）
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "INSERT INTO proxy_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| format!("保存设置失败: {e}"))?;
        Ok(())
    }

    // ===== 访问密钥管理 =====

    /// 查询所有访问密钥（token 脱敏），附带绑定的渠道 ID 列表
    pub fn list_access_tokens(&self) -> Result<Vec<AccessTokenRow>, String> {
        let conn = self.reader.get().lock();
        let mut stmt = conn.prepare(
            "SELECT id, token, name, is_active, total_requests, failed_requests, last_used_at, created_at
            FROM access_tokens
            ORDER BY created_at ASC"
        ).map_err(|e| format!("准备访问密钥查询失败: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            })
            .map_err(|e| format!("查询访问密钥失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            let (
                id,
                token,
                name,
                is_active,
                total_requests,
                failed_requests,
                last_used_at,
                created_at,
            ) = row.map_err(|e| format!("读取访问密钥行失败: {e}"))?;

            // 查询此 token 绑定的渠道
            let mut ch_stmt = conn
                .prepare("SELECT channel_id FROM access_token_channels WHERE access_token_id = ?1")
                .map_err(|e| format!("准备渠道关联查询失败: {e}"))?;
            let channel_ids: Vec<String> = ch_stmt
                .query_map(params![&id], |r| r.get::<_, String>(0))
                .map_err(|e| format!("查询渠道关联失败: {e}"))?
                .filter_map(|r| r.ok())
                .collect();

            result.push(AccessTokenRow {
                id,
                token_masked: mask_api_key(&token),
                name,
                is_active: is_active != 0,
                total_requests: total_requests as u64,
                failed_requests: failed_requests as u64,
                last_used_at,
                channel_ids,
                created_at,
            });
        }
        Ok(result)
    }

    /// 添加访问密钥，自动生成 token，返回含完整 token 的行（仅此一次展示）
    pub fn add_access_token(
        &self,
        name: &str,
        channel_ids: &[String],
    ) -> Result<AccessTokenRow, String> {
        let conn = self.writer.lock();
        let id = uuid::Uuid::new_v4().to_string();
        let token = format!(
            "sk-proxy-{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        );
        let created_at = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT INTO access_tokens (id, token, name, is_active, total_requests, failed_requests, last_used_at, created_at) VALUES (?1, ?2, ?3, 1, 0, 0, NULL, ?4)",
            params![id, token, name, created_at],
        ).map_err(|e| format!("插入访问密钥失败: {e}"))?;

        for ch_id in channel_ids {
            conn.execute(
                "INSERT OR IGNORE INTO access_token_channels (access_token_id, channel_id) VALUES (?1, ?2)",
                params![id, ch_id],
            ).map_err(|e| format!("插入访问密钥-渠道关联失败: {e}"))?;
        }

        Ok(AccessTokenRow {
            id,
            token_masked: token, // 创建时返回完整 token
            name: name.to_string(),
            is_active: true,
            total_requests: 0,
            failed_requests: 0,
            last_used_at: None,
            channel_ids: channel_ids.to_vec(),
            created_at,
        })
    }

    /// 删除访问密钥及其渠道关联
    pub fn delete_access_token(&self, id: &str) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "DELETE FROM access_token_channels WHERE access_token_id = ?1",
            params![id],
        )
        .map_err(|e| format!("删除访问密钥渠道关联失败: {e}"))?;
        conn.execute("DELETE FROM access_tokens WHERE id = ?1", params![id])
            .map_err(|e| format!("删除访问密钥失败: {e}"))?;
        Ok(())
    }

    /// 更新访问密钥启用状态
    pub fn update_access_token_status(&self, id: &str, is_active: bool) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "UPDATE access_tokens SET is_active = ?1 WHERE id = ?2",
            params![is_active as i32, id],
        )
        .map_err(|e| format!("更新访问密钥状态失败: {e}"))?;
        Ok(())
    }

    /// 替换访问密钥绑定的渠道列表
    pub fn update_access_token_channels(
        &self,
        id: &str,
        channel_ids: &[String],
    ) -> Result<(), String> {
        let conn = self.writer.lock();
        conn.execute(
            "DELETE FROM access_token_channels WHERE access_token_id = ?1",
            params![id],
        )
        .map_err(|e| format!("清除访问密钥渠道关联失败: {e}"))?;

        for ch_id in channel_ids {
            conn.execute(
                "INSERT OR IGNORE INTO access_token_channels (access_token_id, channel_id) VALUES (?1, ?2)",
                params![id, ch_id],
            ).map_err(|e| format!("插入访问密钥-渠道关联失败: {e}"))?;
        }
        Ok(())
    }

    /// 根据 token 原始值查找访问密钥（用于认证）
    pub fn get_access_token_by_value(&self, token: &str) -> Result<Option<AccessTokenRow>, String> {
        let conn = self.reader.get().lock();
        let result = conn.query_row(
            "SELECT id, token, name, is_active, total_requests, failed_requests, last_used_at, created_at
            FROM access_tokens WHERE token = ?1",
            params![token],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        );

        match result {
            Ok((
                id,
                token_val,
                name,
                is_active,
                total_requests,
                failed_requests,
                last_used_at,
                created_at,
            )) => {
                // 查询绑定的渠道
                let mut ch_stmt = conn
                    .prepare(
                        "SELECT channel_id FROM access_token_channels WHERE access_token_id = ?1",
                    )
                    .map_err(|e| format!("准备渠道关联查询失败: {e}"))?;
                let channel_ids: Vec<String> = ch_stmt
                    .query_map(params![&id], |r| r.get::<_, String>(0))
                    .map_err(|e| format!("查询渠道关联失败: {e}"))?
                    .filter_map(|r| r.ok())
                    .collect();

                Ok(Some(AccessTokenRow {
                    id,
                    token_masked: mask_api_key(&token_val),
                    name,
                    is_active: is_active != 0,
                    total_requests: total_requests as u64,
                    failed_requests: failed_requests as u64,
                    last_used_at,
                    channel_ids,
                    created_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("查询访问密钥失败: {e}")),
        }
    }

    /// 根据 token 原始值获取其绑定渠道中的所有活跃密钥
    ///
    /// 只返回活跃渠道 + 活跃密钥的组合
    pub fn get_active_keys_for_token(&self, token: &str) -> Result<Vec<ActiveKey>, String> {
        let conn = self.reader.get().lock();
        let mut stmt = conn
            .prepare(
                "SELECT k.id, k.api_key, k.endpoint_id, e.base_url, e.models, e.proxy_url
            FROM api_keys k
            INNER JOIN upstream_endpoints e ON k.endpoint_id = e.id
            INNER JOIN access_token_channels atc ON atc.channel_id = e.id
            INNER JOIN access_tokens at2 ON at2.id = atc.access_token_id
            WHERE at2.token = ?1 AND at2.is_active = 1 AND k.is_active = 1 AND e.is_active = 1
            ORDER BY k.created_at ASC",
            )
            .map_err(|e| format!("准备 token 关联活跃密钥查询失败: {e}"))?;

        let rows = stmt
            .query_map(params![token], |row| {
                let models_json: String =
                    row.get::<_, String>(4).unwrap_or_else(|_| "[]".to_string());
                let endpoint_models: Vec<String> =
                    serde_json::from_str(&models_json).unwrap_or_default();
                Ok(ActiveKey {
                    id: row.get(0)?,
                    api_key: row.get(1)?,
                    endpoint_id: row.get(2)?,
                    base_url: row.get(3)?,
                    proxy_url: row.get::<_, String>(5).unwrap_or_default(),
                    endpoint_models,
                })
            })
            .map_err(|e| format!("查询 token 关联活跃密钥失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取活跃密钥行失败: {e}"))?);
        }
        Ok(result)
    }

    /// 递增访问密钥使用统计
    pub fn increment_access_token_stats(&self, id: &str, success: bool) -> Result<(), String> {
        let conn = self.writer.lock();
        let now = chrono::Utc::now().timestamp();
        let failed_inc = if success { 0 } else { 1 };

        conn.execute(
            "UPDATE access_tokens SET total_requests = total_requests + 1, failed_requests = failed_requests + ?1, last_used_at = ?2 WHERE id = ?3",
            params![failed_inc, now, id],
        ).map_err(|e| format!("更新访问密钥统计失败: {e}"))?;
        Ok(())
    }

    /// 查询 access_tokens 表中的记录数
    pub fn count_access_tokens(&self) -> Result<u64, String> {
        let conn = self.reader.get().lock();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM access_tokens", [], |row| row.get(0))
            .map_err(|e| format!("查询访问密钥数量失败: {e}"))?;
        Ok(count as u64)
    }
}
