//! Retry policy: 5 attempts total, 1/2/4/8/16 minute base delays with +/-10% jitter.

use chrono::Duration;
use rand::Rng;

pub const MAX_ATTEMPTS: i64 = 5;

pub fn backoff_base_secs(attempts: i64) -> i64 {
    60 * 2_i64.pow((attempts - 1).clamp(0, 4) as u32)
}

pub fn backoff_delay(attempts: i64) -> Duration {
    let base_ms = backoff_base_secs(attempts) as f64 * 1000.0;
    let jitter = rand::thread_rng().gen_range(0.9_f64..=1.1);
    Duration::milliseconds((base_ms * jitter).round() as i64)
}
