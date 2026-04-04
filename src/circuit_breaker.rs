//! 简易熔断器
//!
//! 每个 API Key 独立跟踪，连续失败超过阈值后熔断，
//! 超时后进入半开状态允许单次探测。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Closed,
    Open { since: Instant },
    HalfOpen,
}

struct KeyHealth {
    state: State,
    consecutive_failures: u32,
    consecutive_successes: u32,
}

pub struct CircuitBreaker {
    keys: Mutex<HashMap<String, KeyHealth>>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, success_threshold: u32, timeout_secs: u64) -> Self {
        Self {
            keys: Mutex::new(HashMap::new()),
            failure_threshold,
            success_threshold,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// 检查密钥是否可用
    pub fn is_available(&self, key_id: &str) -> bool {
        let mut keys = self.keys.lock().unwrap();
        let health = keys.entry(key_id.to_string()).or_insert(KeyHealth {
            state: State::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
        });

        match health.state {
            State::Closed => true,
            State::Open { since } => {
                if since.elapsed() >= self.timeout {
                    health.state = State::HalfOpen;
                    health.consecutive_successes = 0;
                    log::info!("[cc-proxy] 密钥 {} 熔断超时，进入半开状态", &key_id[..key_id.len().min(8)]);
                    true
                } else {
                    false
                }
            }
            State::HalfOpen => true,
        }
    }

    /// 记录密钥请求成功
    pub fn record_success(&self, key_id: &str) {
        let mut keys = self.keys.lock().unwrap();
        let health = keys.entry(key_id.to_string()).or_insert(KeyHealth {
            state: State::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
        });

        health.consecutive_failures = 0;
        health.consecutive_successes += 1;

        if health.state == State::HalfOpen && health.consecutive_successes >= self.success_threshold {
            health.state = State::Closed;
            log::info!("[cc-proxy] 密钥 {} 恢复正常", &key_id[..key_id.len().min(8)]);
        }
    }

    /// 记录密钥请求失败
    pub fn record_failure(&self, key_id: &str) {
        let mut keys = self.keys.lock().unwrap();
        let health = keys.entry(key_id.to_string()).or_insert(KeyHealth {
            state: State::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
        });

        health.consecutive_failures += 1;
        health.consecutive_successes = 0;

        match health.state {
            State::Closed => {
                if health.consecutive_failures >= self.failure_threshold {
                    health.state = State::Open { since: Instant::now() };
                    log::warn!("[cc-proxy] 密钥 {} 连续失败 {} 次，触发熔断", &key_id[..key_id.len().min(8)], health.consecutive_failures);
                }
            }
            State::HalfOpen => {
                health.state = State::Open { since: Instant::now() };
                log::warn!("[cc-proxy] 密钥 {} 半开探测失败，重新熔断", &key_id[..key_id.len().min(8)]);
            }
            State::Open { .. } => {}
        }
    }
}
