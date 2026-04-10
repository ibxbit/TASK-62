/// Email channel adapter.
///
/// Delivers notifications via an on-prem SMTP relay exposed as an HTTP API.
/// Requires no internet access — all traffic stays on the corporate LAN.
///
/// ## Configuration
///
/// | Env var             | Description                                       |
/// |---------------------|---------------------------------------------------|
/// | `EMAIL_RELAY_URL`   | HTTP endpoint of the on-prem relay, e.g.          |
/// |                     | `http://mailrelay.internal:8025/send`              |
/// | `EMAIL_FROM_ADDRESS`| Sender address (default: `noreply@transitops.local`) |
///
/// ## Wire format
///
/// ```json
/// POST {EMAIL_RELAY_URL}
/// Content-Type: application/json
///
/// {
///   "from":    "noreply@transitops.local",
///   "to":      "user@example.com",
///   "subject": "[WARNING] ops.trip.conflict_detected",
///   "body":    "Trip T-42 has a scheduling conflict with T-55."
/// }
/// ```
///
/// The relay is responsible for SMTP authentication, TLS, and queuing.
/// If `EMAIL_RELAY_URL` is not set, `is_available()` returns `false` and
/// no connections are attempted.
use super::{AdapterError, NotificationAdapter, OutboundNotification};

pub struct EmailAdapter {
    relay_url: Option<String>,
    from_addr: String,
    client:    reqwest::Client,
}

impl EmailAdapter {
    pub fn new(relay_url: Option<String>, from_addr: String) -> Self {
        Self {
            relay_url,
            from_addr,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("HTTP client build should not fail"),
        }
    }
}

#[async_trait::async_trait]
impl NotificationAdapter for EmailAdapter {
    fn channel(&self) -> &'static str { "email" }

    fn is_available(&self) -> bool { self.relay_url.is_some() }

    async fn send(&self, notif: &OutboundNotification) -> Result<(), AdapterError> {
        let url = self.relay_url.as_deref().ok_or(AdapterError::NotConfigured)?;

        let subject = format!(
            "[{}] {}",
            notif.severity.to_uppercase(),
            notif.title
        );

        let payload = serde_json::json!({
            "from":    self.from_addr,
            "to":      notif.channel_address,
            "subject": subject,
            "body":    notif.body,
            "event_type": notif.event_type,
        });

        let resp = self.client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AdapterError::DeliveryFailed(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AdapterError::DeliveryFailed(format!(
                "relay returned HTTP {}",
                resp.status()
            )));
        }

        Ok(())
    }
}
