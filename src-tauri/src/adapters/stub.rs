//! Scripted adapters for the gate: the stub matrix drives every adapter outcome
//! without touching a real platform. The dispatch/queue path sees the same
//! AdapterError taxonomy a live adapter produces.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::adapters::{
    BoxFuture, Capabilities, ConnectionState, ConnState, ErrorKind, AdapterError, MediaKind,
    PlatformAdapter, PublishOk, PublishPayload,
};

/// One scripted outcome, consumed in order; the last one repeats forever.
#[derive(Debug, Clone)]
pub enum Scripted {
    Ok,
    Transient,
    RateLimited(u64),
    Permanent,
    Auth,
    Truncate(usize),
}

pub struct StubAdapter {
    pub capabilities: Capabilities,
    outcomes: Mutex<VecDeque<Scripted>>,
}

impl StubAdapter {
    pub fn new(platform: &str, outcomes: Vec<Scripted>) -> Self {
        Self {
            capabilities: Capabilities {
                platform: platform.to_string(),
                max_text_length: 4096,
                media_kinds: vec![MediaKind::None, MediaKind::Image],
                honors_retry_after: true,
                supports_edit: false,
                supports_delete: false,
                supports_metrics: false,
                needs_public_image_url: false,
            },
            outcomes: Mutex::new(outcomes.into()),
        }
    }

    fn next(&self) -> Scripted {
        let mut queue = self.outcomes.lock().expect("stub outcomes");
        if queue.len() > 1 {
            queue.pop_front().unwrap_or(Scripted::Ok)
        } else {
            queue.front().cloned().unwrap_or(Scripted::Ok)
        }
    }
}

impl PlatformAdapter for StubAdapter {
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    fn connection_state(&self) -> BoxFuture<'_, Result<ConnectionState, AdapterError>> {
        Box::pin(async {
            Ok(ConnectionState {
                state: ConnState::Ready,
                reason: Some("scripted".into()),
            })
        })
    }

    fn publish(&self, payload: PublishPayload) -> BoxFuture<'_, Result<PublishOk, AdapterError>> {
        let scripted = self.next();
        let max = self.capabilities.max_text_length;
        Box::pin(async move {
            let (_, truncated_from) = crate::adapters::truncate_text(&payload.text, max);
            match scripted {
                Scripted::Ok => Ok(PublishOk {
                    ok: true,
                    remote_id: Some("stub-1".into()),
                    remote_url: None,
                    truncated_from,
                }),
                Scripted::Truncate(limit) => {
                    let (short, from) = crate::adapters::truncate_text(&payload.text, limit);
                    Ok(PublishOk {
                        ok: true,
                        remote_id: Some(format!("stub-{}", short.len())),
                        remote_url: None,
                        truncated_from: from,
                    })
                }
                Scripted::RateLimited(secs) => Err(AdapterError {
                    kind: ErrorKind::RateLimited,
                    message: format!("rate limited for {secs}s"),
                    retry_after_seconds: Some(secs),
                }),
                Scripted::Transient => Err(AdapterError {
                    kind: ErrorKind::PlatformOutage,
                    message: "platform outage".into(),
                    retry_after_seconds: None,
                }),
                Scripted::Permanent => Err(AdapterError {
                    kind: ErrorKind::ContentInvalid,
                    message: "content refused".into(),
                    retry_after_seconds: None,
                }),
                Scripted::Auth => Err(AdapterError {
                    kind: ErrorKind::AuthInvalid,
                    message: "token revoked".into(),
                    retry_after_seconds: None,
                }),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{classify, FailureClass};

    #[tokio::test]
    async fn matrix_covers_every_outcome_with_the_plan_taxonomy() {
        let payload = || PublishPayload {
            target: "stub".into(),
            text: "hello".into(),
            media: None,
            dry_run: false,
        };

        let ok = StubAdapter::new("telegram", vec![Scripted::Ok]);
        assert!(ok.publish(payload()).await.is_ok());

        let limited = StubAdapter::new("telegram", vec![Scripted::RateLimited(42)]);
        let err = limited.publish(payload()).await.unwrap_err();
        assert_eq!(err.retry_after_seconds, Some(42));
        assert_eq!(classify(&err), FailureClass::Transient);

        let outage = StubAdapter::new("telegram", vec![Scripted::Transient]);
        assert_eq!(classify(&outage.publish(payload()).await.unwrap_err()), FailureClass::Transient);

        let bad = StubAdapter::new("telegram", vec![Scripted::Permanent]);
        assert_eq!(classify(&bad.publish(payload()).await.unwrap_err()), FailureClass::Permanent);

        let auth = StubAdapter::new("telegram", vec![Scripted::Auth]);
        assert_eq!(classify(&auth.publish(payload()).await.unwrap_err()), FailureClass::AuthPermission);
    }

    #[tokio::test]
    async fn truncation_reports_the_original_length() {
        let adapter = StubAdapter::new("telegram", vec![Scripted::Truncate(5)]);
        let result = adapter
            .publish(PublishPayload {
                target: "stub".into(),
                text: "one two three".into(),
                media: None,
                dry_run: false,
            })
            .await
            .unwrap();
        assert_eq!(result.truncated_from, Some(13));
    }

    #[tokio::test]
    async fn the_script_consumes_in_order_and_repeats_the_last() {
        let adapter = StubAdapter::new(
            "telegram",
            vec![Scripted::Permanent, Scripted::Ok],
        );
        assert!(adapter
            .publish(PublishPayload { target: "t".into(), text: "x".into(), media: None, dry_run: false })
            .await
            .is_err());
        for _ in 0..3 {
            assert!(adapter
                .publish(PublishPayload { target: "t".into(), text: "x".into(), media: None, dry_run: false })
                .await
                .is_ok());
        }
    }
}
