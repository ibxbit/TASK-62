use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url:     String,
    pub server_host:      String,
    pub server_port:      u16,
    pub encryption_key:          String,
    /// Optional previous key for in-flight rotation.  When set, decryption
    /// falls back to this key if the current key fails.  Clear once all rows
    /// have been re-encrypted with the new key.
    pub encryption_key_previous: Option<String>,

    // ---- External channel adapters (on-prem connectors) ----
    /// HTTP endpoint of the on-prem SMTP relay.
    /// Absent → email adapter inactive; no connections attempted.
    /// To enable/disable the email adapter, set/unset EMAIL_RELAY_URL in env.
    pub email_relay_url:   Option<String>,
    /// Sender address used in outbound email.
    pub email_from_addr:   String,
    /// HTTP endpoint of the on-prem SMS gateway.
    /// Absent → SMS adapter inactive. To enable/disable, set/unset SMS_GATEWAY_URL.
    pub sms_gateway_url:   Option<String>,
    /// WeCom Bot webhook URL (on-prem proxy or direct WeCom endpoint).
    /// Absent → WeCom adapter inactive. To enable/disable, set/unset WECOM_WEBHOOK_URL.
    pub wecom_webhook_url: Option<String>,

    // ---- Auth constants (tunable via env; sensible defaults hardcoded) ----
    /// Failed attempts before an account is locked.
    pub lockout_threshold: i32,
    /// How long (minutes) a locked account stays locked.
    pub lockout_duration_minutes: i64,
    /// Idle session is revoked after this many minutes of inactivity.
    pub inactivity_timeout_minutes: i64,
    /// Re-authentication must have occurred within this window for admin actions.
    pub reauth_window_minutes: i64,
    /// Absolute maximum session age in hours regardless of activity.
    pub session_max_age_hours: i64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url:   env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            server_host:    env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            server_port:    env::var("SERVER_PORT")
                                .unwrap_or_else(|_| "8081".into())
                                .parse()
                                .expect("SERVER_PORT must be a valid port number"),
            encryption_key:          env::var("ENCRYPTION_KEY")
                                         .expect("ENCRYPTION_KEY must be set"),
            encryption_key_previous: env::var("ENCRYPTION_KEY_PREVIOUS").ok(),

            email_relay_url:   env::var("EMAIL_RELAY_URL").ok(),
            email_from_addr:   env::var("EMAIL_FROM_ADDRESS")
                                   .unwrap_or_else(|_| "noreply@transitops.local".into()),
            sms_gateway_url:   env::var("SMS_GATEWAY_URL").ok(),
            wecom_webhook_url: env::var("WECOM_WEBHOOK_URL").ok(),

            lockout_threshold:          5,
            lockout_duration_minutes:   15,
            inactivity_timeout_minutes: 30,
            reauth_window_minutes:      10,
            session_max_age_hours:      8,
        }
    }
}
