/// SMS channel adapter.
///
/// Delivers short-text notifications via an on-prem SMS gateway appliance.
/// All traffic remains on the corporate LAN — no internet required.
///
/// ## Configuration
///
/// | Env var           | Description                                         |
/// |-------------------|-----------------------------------------------------|
/// | `SMS_GATEWAY_URL` | HTTP endpoint of the on-prem gateway, e.g.          |
/// |                   | `http://sms-gw.internal:8081/send`                  |
///
/// ## Wire format
///
/// ```json
/// POST {SMS_GATEWAY_URL}
/// Content-Type: application/json
///
/// {
///   "to":      "+8613812345678",
///   "message": "[CRITICAL] Driver unassigned: Trip T-42 departs in 10 min."
/// }
/// ```
///
/// The `to` field must be an E.164 phone number stored in
/// `channel_preferences.channel_address`.
///
/// Message length is capped at 160 characters to fit a single SMS PDU;
/// longer messages are truncated with "…".
use super::{AdapterError, NotificationAdapter, OutboundNotification};

/// Maximum SMS body length in characters (single PDU).
const SMS_MAX_LEN: usize = 160;

pub struct SmsAdapter {
    gateway_url: Option<String>,
    client:      reqwest::Client,
}

impl SmsAdapter {
    pub fn new(gateway_url: Option<String>) -> Self {
        Self {
            gateway_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("HTTP client build should not fail"),
        }
    }
}

#[async_trait::async_trait]
impl NotificationAdapter for SmsAdapter {
    fn channel(&self) -> &'static str { "sms" }

    fn is_available(&self) -> bool { self.gateway_url.is_some() }

    async fn send(&self, notif: &OutboundNotification) -> Result<(), AdapterError> {
        let url = self.gateway_url.as_deref().ok_or(AdapterError::NotConfigured)?;

        let raw = format!(
            "[{}] {}",
            notif.severity.to_uppercase(),
            notif.body
        );
        let message = truncate_sms(&raw);

        let payload = serde_json::json!({
            "to":      notif.channel_address,
            "message": message,
        });

        let resp = self.client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AdapterError::DeliveryFailed(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AdapterError::DeliveryFailed(format!(
                "gateway returned HTTP {}",
                resp.status()
            )));
        }

        Ok(())
    }
}

/// Truncate a string to `SMS_MAX_LEN` characters, appending "…" if cut.
fn truncate_sms(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= SMS_MAX_LEN {
        s.to_string()
    } else {
        // Reserve one char for the ellipsis
        chars[..SMS_MAX_LEN - 1].iter().collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_message_unchanged() {
        let short = "Hello";
        assert_eq!(truncate_sms(short), short);
    }

    #[test]
    fn truncate_long_message_appends_ellipsis() {
        let long = "A".repeat(200);
        let result = truncate_sms(&long);
        let count: usize = result.chars().count();
        assert_eq!(count, SMS_MAX_LEN);
        assert!(result.ends_with('…'));
    }
}
