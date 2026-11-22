//! Queue data types and display states.

use serde::{Deserialize, Serialize};

pub const PENDING: &str = "pending";
pub const SENDING: &str = "sending";
pub const SENT: &str = "sent";
pub const FAILED: &str = "failed";
pub const ATTENTION: &str = "attention";
pub const CANCELLED: &str = "cancelled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    Transient,
    Permanent,
    AuthPermission,
}

impl Failure {
    /// Adapter error kinds are free-form; anything unknown is retryable.
    pub fn classify(kind: &str) -> Failure {
        let k = kind.trim().to_ascii_lowercase().replace('-', "_").replace(' ', "_");
        if k.contains("auth") || k.contains("permission") || k.contains("401") || k.contains("403") {
            Failure::AuthPermission
        } else if k.contains("permanent") || k.contains("invalid") || k.contains("400")
            || k.contains("404") || k.contains("422")
        {
            Failure::Permanent
        } else {
            Failure::Transient
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub delivery_id: String,
    pub post_id: String,
    pub target: String,
    pub text: String,
    pub preset: Option<String>,
    /// The Media product this delivery carries, and the price shown when it was composed.
    pub product_id: Option<String>,
    pub price_shown: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveryRow {
    pub id: String,
    pub post_id: String,
    /// The post text, so the queue can show what is being sent without a second call.
    pub text: String,
    pub target: String,
    pub preset: Option<String>,
    pub state: String,
    pub shown_state: String,
    pub attempts: i64,
    pub next_attempt_at: String,
    pub last_error: Option<String>,
    pub attention: bool,
    pub remote_id: Option<String>,
    pub remote_url: Option<String>,
    pub sent_late_minutes: Option<i64>,
    pub product_id: Option<String>,
    pub price_shown: Option<f64>,
    pub price_sent: Option<f64>,
    pub price_as_of: Option<String>,
}
