/// WeCom (企业微信) channel adapter.
///
/// Delivers notifications via a WeCom Bot webhook.  In on-prem / government
/// WeCom deployments the webhook endpoint is reachable from the corporate LAN
/// without external internet access.
///
/// ## Configuration
///
/// | Env var              | Description                                       |
/// |----------------------|---------------------------------------------------|
/// | `WECOM_WEBHOOK_URL`  | Full WeCom Bot webhook URL, e.g.                  |
/// |                      | `https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=XXX` |
/// |                      | or an on-prem proxy: `http://wecom-proxy.internal/send` |
///
/// The `channel_address` stored per-user is the **WeCom `@` mention ID**
/// (the user's WeCom internal account ID, used in `mentioned_mobile_list` or
/// `mentioned_list`).  If absent, the message is sent to the group without
/// `@` mention.
///
/// ## Wire format
///
/// TransitOps sends **Markdown messages** for rich formatting:
///
/// ```json
/// POST {WECOM_WEBHOOK_URL}
/// Content-Type: application/json
///
/// {
///   "msgtype": "markdown",
///   "markdown": {
///     "content": "### [WARNING] ops.trip.conflict_detected\nTrip T-42 conflict…\n<@user_id>"
///   }
/// }
/// ```
///
/// Severity is colour-coded via WeCom Markdown font-colour tags:
/// - `info`     → default
/// - `warning`  → `<font color=\"warning\">…</font>`
/// - `critical` → `<font color=\"info\">…</font>` (WeCom uses "info" for red)
use super::{AdapterError, NotificationAdapter, OutboundNotification};

pub struct WeComAdapter {
    webhook_url: Option<String>,
    client:      reqwest::Client,
}

impl WeComAdapter {
    pub fn new(webhook_url: Option<String>) -> Self {
        Self {
            webhook_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("HTTP client build should not fail"),
        }
    }
}

#[async_trait::async_trait]
impl NotificationAdapter for WeComAdapter {
    fn channel(&self) -> &'static str { "wecom" }

    fn is_available(&self) -> bool { self.webhook_url.is_some() }

    async fn send(&self, notif: &OutboundNotification) -> Result<(), AdapterError> {
        let url = self.webhook_url.as_deref().ok_or(AdapterError::NotConfigured)?;

        let content = build_markdown(notif);

        let payload = serde_json::json!({
            "msgtype": "markdown",
            "markdown": {
                "content": content,
            }
        });

        let resp = self.client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AdapterError::DeliveryFailed(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AdapterError::DeliveryFailed(format!(
                "WeCom webhook returned HTTP {}",
                resp.status()
            )));
        }

        // WeCom returns { "errcode": 0, "errmsg": "ok" } on success.
        // A non-zero errcode with HTTP 200 is still a failure.
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if body.get("errcode").and_then(|v| v.as_i64()).unwrap_or(0) != 0 {
                let errmsg = body.get("errmsg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                return Err(AdapterError::DeliveryFailed(format!(
                    "WeCom API error: {}",
                    errmsg
                )));
            }
        }

        Ok(())
    }
}

/// Build a WeCom Markdown string from the notification.
///
/// Format:
/// ```markdown
/// ### [SEVERITY] Event Type
/// Body text
/// <@wecom_user_id>    ← only when channel_address is non-empty
/// ```
fn build_markdown(notif: &OutboundNotification) -> String {
    let severity_tag = match notif.severity.as_str() {
        "warning"  => format!("<font color=\"warning\">[WARNING]</font> "),
        "critical" => format!("<font color=\"info\">[CRITICAL]</font> "),
        _          => String::new(),
    };

    let mention = if notif.channel_address.is_empty() {
        String::new()
    } else {
        format!("\n<@{}>", notif.channel_address)
    };

    format!(
        "### {}{}\n{}{}",
        severity_tag,
        notif.title,
        notif.body,
        mention
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_notif(severity: &str, address: &str) -> OutboundNotification {
        OutboundNotification {
            event_type:      "ops.trip.conflict_detected".into(),
            severity:        severity.into(),
            title:           "Trip conflict".into(),
            body:            "Trip T-42 conflicts with T-55.".into(),
            payload:         serde_json::Value::Object(Default::default()),
            recipient_id:    uuid::Uuid::nil(),
            channel_address: address.into(),
        }
    }

    #[test]
    fn critical_uses_colour_tag() {
        let md = build_markdown(&make_notif("critical", ""));
        assert!(md.contains("<font color=\"info\">"));
    }

    #[test]
    fn warning_uses_warning_tag() {
        let md = build_markdown(&make_notif("warning", ""));
        assert!(md.contains("<font color=\"warning\">"));
    }

    #[test]
    fn mention_appended_when_address_set() {
        let md = build_markdown(&make_notif("info", "wangfang"));
        assert!(md.contains("<@wangfang>"));
    }

    #[test]
    fn no_mention_when_address_empty() {
        let md = build_markdown(&make_notif("info", ""));
        assert!(!md.contains("<@"));
    }
}
