use async_trait::async_trait;
use serde_json::{json, Value};

use super::{QueuedEvent, TelemetrySink};

pub struct HttpSink {
    pub http: reqwest::Client,
    pub url: String,
    pub bearer: Option<String>,
}

#[async_trait]
impl TelemetrySink for HttpSink {
    async fn send(&self, events: &[QueuedEvent]) -> anyhow::Result<()> {
        let body = json!({
            "source": "travis",
            "events": events.iter().map(|e| {
                let payload: Value = serde_json::from_str(&e.payload_json).unwrap_or(Value::Null);
                json!({
                    "kind": e.kind,
                    "ts": e.created_at,
                    "payload": payload,
                })
            }).collect::<Vec<_>>(),
        });
        let mut req = self.http.post(&self.url).json(&body);
        if let Some(b) = self.bearer.as_deref() {
            req = req.bearer_auth(b);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("telemetry sink {}: {}", status.as_u16(), body);
        }
        Ok(())
    }
}
