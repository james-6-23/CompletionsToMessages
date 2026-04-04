//! SQLite 数据库模块
//!
//! 管理使用统计数据的存储与查询

use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::{Arc, Mutex};

/// 线程安全的数据库连接封装
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
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
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
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

/// API Key 行（对外展示，密钥脱敏）
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyRow {
    pub id: String,
    pub api_key_masked: String,
    pub label: String,
    pub is_active: bool,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
}

/// 活跃密钥（内部使用，含完整密钥值）
#[derive(Debug, Clone)]
pub struct ActiveKey {
    pub id: String,
    pub api_key: String,
}

/// 将 API Key 脱敏，保留前 4 位和后 4 位
fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "****".to_string();
    }
    let prefix = &key[..4];
    let suffix = &key[key.len()-4..];
    format!("{}...{}", prefix, suffix)
}

impl Database {
    /// 创建数据库连接并初始化表结构
    pub fn new(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path)
            .map_err(|e| format!("打开数据库失败: {e}"))?;

        // 启用 WAL 模式提升并发性能
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("设置 WAL 模式失败: {e}"))?;

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
            CREATE INDEX IF NOT EXISTS idx_logs_status ON proxy_request_logs(status_code);"
        ).map_err(|e| format!("创建 proxy_request_logs 表失败: {e}"))?;

        // 创建模型定价表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS model_pricing (
                model_id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                input_cost_per_million TEXT NOT NULL DEFAULT '0',
                output_cost_per_million TEXT NOT NULL DEFAULT '0',
                cache_read_cost_per_million TEXT NOT NULL DEFAULT '0',
                cache_creation_cost_per_million TEXT NOT NULL DEFAULT '0'
            );"
        ).map_err(|e| format!("创建 model_pricing 表失败: {e}"))?;

        // 预填充常见模型定价（使用 INSERT OR IGNORE 避免重复）
        let pricing_data = vec![
            ("gpt-4o", "GPT-4o", "2.50", "10.00", "1.25", "3.75"),
            ("gpt-4o-mini", "GPT-4o Mini", "0.15", "0.60", "0.075", "0.225"),
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

        // 创建 API 密钥管理表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                api_key TEXT NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                is_active INTEGER NOT NULL DEFAULT 1,
                total_requests INTEGER NOT NULL DEFAULT 0,
                failed_requests INTEGER NOT NULL DEFAULT 0,
                last_used_at INTEGER,
                created_at INTEGER NOT NULL
            );"
        ).map_err(|e| format!("创建 api_keys 表失败: {e}"))?;

        // 创建代理设置表（KV 存储）
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS proxy_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );"
        ).map_err(|e| format!("创建 proxy_settings 表失败: {e}"))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
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
        created_at: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        conn.execute(
            "INSERT INTO proxy_request_logs (
                request_id, model, request_model, input_tokens, output_tokens,
                cache_read_tokens, cache_creation_tokens,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                latency_ms, first_token_ms, status_code, is_streaming, error_message, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                request_id, model, request_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                latency_ms as i64, first_token_ms.map(|v| v as i64),
                status_code as i32, is_streaming as i32, error_message, created_at
            ],
        ).map_err(|e| format!("插入请求日志失败: {e}"))?;
        Ok(())
    }

    /// 查询使用统计摘要
    pub fn get_usage_summary(&self, start_ts: i64, end_ts: i64) -> Result<UsageSummary, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT
                COUNT(*) as total_requests,
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0) as total_cost,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(cache_creation_tokens), 0) as total_cache_creation_tokens,
                COALESCE(SUM(cache_read_tokens), 0) as total_cache_read_tokens
            FROM proxy_request_logs
            WHERE created_at >= ?1 AND created_at < ?2"
        ).map_err(|e| format!("准备查询失败: {e}"))?;

        let result = stmt.query_row(params![start_ts, end_ts], |row| {
            Ok(UsageSummary {
                total_requests: row.get::<_, i64>(0)? as u64,
                total_cost: format!("{:.6}", row.get::<_, f64>(1)?),
                total_input_tokens: row.get::<_, i64>(2)? as u64,
                total_output_tokens: row.get::<_, i64>(3)? as u64,
                total_cache_creation_tokens: row.get::<_, i64>(4)? as u64,
                total_cache_read_tokens: row.get::<_, i64>(5)? as u64,
            })
        }).map_err(|e| format!("查询使用摘要失败: {e}"))?;

        Ok(result)
    }

    /// 查询使用趋势（按时间分桶）
    pub fn get_usage_trends(
        &self,
        start_ts: i64,
        end_ts: i64,
        interval_secs: i64,
    ) -> Result<Vec<UsageTrend>, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT
                (created_at / ?3) * ?3 as bucket,
                COUNT(*) as request_count,
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0) as total_cost,
                COALESCE(SUM(input_tokens), 0) as input_tokens,
                COALESCE(SUM(output_tokens), 0) as output_tokens,
                COALESCE(SUM(cache_creation_tokens), 0) as cache_creation_tokens,
                COALESCE(SUM(cache_read_tokens), 0) as cache_read_tokens
            FROM proxy_request_logs
            WHERE created_at >= ?1 AND created_at < ?2
            GROUP BY bucket
            ORDER BY bucket ASC"
        ).map_err(|e| format!("准备查询失败: {e}"))?;

        let rows = stmt.query_map(params![start_ts, end_ts, interval_secs], |row| {
            let bucket_ts = row.get::<_, i64>(0)?;
            let date = chrono::DateTime::from_timestamp(bucket_ts, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| bucket_ts.to_string());

            Ok(UsageTrend {
                date,
                request_count: row.get::<_, i64>(1)? as u64,
                total_cost: format!("{:.6}", row.get::<_, f64>(2)?),
                input_tokens: row.get::<_, i64>(3)? as u64,
                output_tokens: row.get::<_, i64>(4)? as u64,
                cache_creation_tokens: row.get::<_, i64>(5)? as u64,
                cache_read_tokens: row.get::<_, i64>(6)? as u64,
            })
        }).map_err(|e| format!("查询使用趋势失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取行数据失败: {e}"))?);
        }
        Ok(result)
    }

    /// 查询模型维度统计
    pub fn get_model_stats(&self, start_ts: i64, end_ts: i64) -> Result<Vec<ModelStats>, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let mut stmt = conn.prepare(
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
            ORDER BY total_cost DESC"
        ).map_err(|e| format!("准备查询失败: {e}"))?;

        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
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
        }).map_err(|e| format!("查询模型统计失败: {e}"))?;

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
        start_ts: i64,
        end_ts: i64,
    ) -> Result<PaginatedLogs, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;

        // 构建动态 WHERE 条件
        let mut conditions = vec!["created_at >= ?1 AND created_at < ?2".to_string()];
        let mut param_index = 3;

        if status_code_filter.is_some() {
            conditions.push(format!("status_code = ?{param_index}"));
            param_index += 1;
        }
        if model_filter.is_some() {
            conditions.push(format!("model LIKE ?{param_index}"));
        }

        let where_clause = conditions.join(" AND ");

        // 查询总数
        let count_sql = format!("SELECT COUNT(*) FROM proxy_request_logs WHERE {where_clause}");
        let total = {
            let mut stmt = conn.prepare(&count_sql)
                .map_err(|e| format!("准备计数查询失败: {e}"))?;

            let total: i64 = match (status_code_filter, model_filter) {
                (Some(sc), Some(m)) => stmt.query_row(
                    params![start_ts, end_ts, sc as i32, format!("%{m}%")], |row| row.get(0)
                ),
                (Some(sc), None) => stmt.query_row(
                    params![start_ts, end_ts, sc as i32], |row| row.get(0)
                ),
                (None, Some(m)) => stmt.query_row(
                    params![start_ts, end_ts, format!("%{m}%")], |row| row.get(0)
                ),
                (None, None) => stmt.query_row(
                    params![start_ts, end_ts], |row| row.get(0)
                ),
            }.map_err(|e| format!("计数查询失败: {e}"))?;
            total as u64
        };

        // 查询分页数据
        let offset = (page.saturating_sub(1)) * page_size;
        let query_sql = format!(
            "SELECT request_id, model, request_model, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens,
                    input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                    latency_ms, first_token_ms, status_code, is_streaming, error_message, created_at
            FROM proxy_request_logs
            WHERE {where_clause}
            ORDER BY created_at DESC
            LIMIT ?{} OFFSET ?{}",
            param_index, param_index + 1
        );

        let mut stmt = conn.prepare(&query_sql)
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
                created_at: row.get(17)?,
            })
        };

        let rows = match (status_code_filter, model_filter) {
            (Some(sc), Some(m)) => stmt.query_map(
                params![start_ts, end_ts, sc as i32, format!("%{m}%"), page_size, offset],
                map_row,
            ),
            (Some(sc), None) => stmt.query_map(
                params![start_ts, end_ts, sc as i32, page_size, offset],
                map_row,
            ),
            (None, Some(m)) => stmt.query_map(
                params![start_ts, end_ts, format!("%{m}%"), page_size, offset],
                map_row,
            ),
            (None, None) => stmt.query_map(
                params![start_ts, end_ts, page_size, offset],
                map_row,
            ),
        }.map_err(|e| format!("日志查询失败: {e}"))?;

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
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
            FROM model_pricing
            ORDER BY model_id"
        ).map_err(|e| format!("准备定价查询失败: {e}"))?;

        let rows = stmt.query_map([], |row| {
            Ok(ModelPricingRow {
                model_id: row.get(0)?,
                display_name: row.get(1)?,
                input_cost_per_million: row.get(2)?,
                output_cost_per_million: row.get(3)?,
                cache_read_cost_per_million: row.get(4)?,
                cache_creation_cost_per_million: row.get(5)?,
            })
        }).map_err(|e| format!("定价查询失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取定价行失败: {e}"))?);
        }
        Ok(result)
    }

    /// 根据模型 ID 查询单个定价信息
    pub fn get_pricing_for_model(&self, model: &str) -> Result<Option<ModelPricingRow>, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
            FROM model_pricing
            WHERE model_id = ?1"
        ).map_err(|e| format!("准备定价查询失败: {e}"))?;

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

    // ===== API Key 管理方法 =====

    /// 查询所有 API Key（脱敏）
    pub fn list_api_keys(&self) -> Result<Vec<ApiKeyRow>, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT id, api_key, label, is_active, total_requests, failed_requests, last_used_at, created_at
            FROM api_keys
            ORDER BY created_at DESC"
        ).map_err(|e| format!("准备 api_keys 查询失败: {e}"))?;

        let rows = stmt.query_map([], |row| {
            let raw_key: String = row.get(1)?;
            Ok(ApiKeyRow {
                id: row.get(0)?,
                api_key_masked: mask_api_key(&raw_key),
                label: row.get(2)?,
                is_active: row.get::<_, i32>(3)? != 0,
                total_requests: row.get::<_, i64>(4)? as u64,
                failed_requests: row.get::<_, i64>(5)? as u64,
                last_used_at: row.get(6)?,
                created_at: row.get(7)?,
            })
        }).map_err(|e| format!("查询 api_keys 失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取 api_key 行失败: {e}"))?);
        }
        Ok(result)
    }

    /// 添加 API Key
    pub fn add_api_key(&self, api_key: &str, label: &str) -> Result<ApiKeyRow, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT INTO api_keys (id, api_key, label, is_active, total_requests, failed_requests, last_used_at, created_at)
            VALUES (?1, ?2, ?3, 1, 0, 0, NULL, ?4)",
            params![id, api_key, label, created_at],
        ).map_err(|e| format!("插入 api_key 失败: {e}"))?;

        Ok(ApiKeyRow {
            id,
            api_key_masked: mask_api_key(api_key),
            label: label.to_string(),
            is_active: true,
            total_requests: 0,
            failed_requests: 0,
            last_used_at: None,
            created_at,
        })
    }

    /// 删除 API Key
    pub fn delete_api_key(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        conn.execute("DELETE FROM api_keys WHERE id = ?1", params![id])
            .map_err(|e| format!("删除 api_key 失败: {e}"))?;
        Ok(())
    }

    /// 更新 API Key 启用状态
    pub fn update_api_key_status(&self, id: &str, is_active: bool) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        conn.execute(
            "UPDATE api_keys SET is_active = ?1 WHERE id = ?2",
            params![is_active as i32, id],
        ).map_err(|e| format!("更新 api_key 状态失败: {e}"))?;
        Ok(())
    }

    /// 更新 API Key 标签
    pub fn update_api_key_label(&self, id: &str, label: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        conn.execute(
            "UPDATE api_keys SET label = ?1 WHERE id = ?2",
            params![label, id],
        ).map_err(|e| format!("更新 api_key 标签失败: {e}"))?;
        Ok(())
    }

    /// 递增 API Key 使用统计
    pub fn increment_key_stats(&self, id: &str, success: bool) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let now = chrono::Utc::now().timestamp();
        let failed_inc = if success { 0 } else { 1 };

        conn.execute(
            "UPDATE api_keys SET total_requests = total_requests + 1, failed_requests = failed_requests + ?1, last_used_at = ?2 WHERE id = ?3",
            params![failed_inc, now, id],
        ).map_err(|e| format!("更新 api_key 统计失败: {e}"))?;
        Ok(())
    }

    /// 获取所有活跃密钥（完整密钥值，内部使用）
    pub fn get_all_active_keys(&self) -> Result<Vec<ActiveKey>, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT id, api_key FROM api_keys WHERE is_active = 1 ORDER BY created_at ASC"
        ).map_err(|e| format!("准备活跃密钥查询失败: {e}"))?;

        let rows = stmt.query_map([], |row| {
            Ok(ActiveKey {
                id: row.get(0)?,
                api_key: row.get(1)?,
            })
        }).map_err(|e| format!("查询活跃密钥失败: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("读取活跃密钥行失败: {e}"))?);
        }
        Ok(result)
    }

    /// 获取完整 API Key（用于测试密钥等场景）
    pub fn get_api_key_full(&self, id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        let mut stmt = conn.prepare("SELECT api_key FROM api_keys WHERE id = ?1")
            .map_err(|e| format!("准备密钥查询失败: {e}"))?;

        let result = stmt.query_row(params![id], |row| row.get::<_, String>(0));

        match result {
            Ok(key) => Ok(Some(key)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("查询密钥失败: {e}")),
        }
    }

    // ===== 代理设置（KV 存储） =====

    /// 获取设置值
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
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
        let conn = self.conn.lock().map_err(|e| format!("获取数据库锁失败: {e}"))?;
        conn.execute(
            "INSERT INTO proxy_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        ).map_err(|e| format!("保存设置失败: {e}"))?;
        Ok(())
    }

    /// 获取上游 base_url（优先数据库，fallback 到 config）
    pub fn get_upstream_url(&self) -> Result<Option<String>, String> {
        self.get_setting("upstream_base_url")
    }

    /// 设置上游 base_url
    pub fn set_upstream_url(&self, url: &str) -> Result<(), String> {
        self.set_setting("upstream_base_url", url)
    }
}
