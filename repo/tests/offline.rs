//! Offline behaviour and channel adapter failure tests.
//!
//! Core invariant: inbox delivery MUST succeed even when all external channel
//! adapters are unavailable or return errors.  The bus is designed so that
//! external channel failures are non-fatal and never abort inbox delivery.
//!
//! Pure tests in this file verify adapter availability detection when no
//! connector URL is configured.  Full fan-out integration tests require a
//! database and are documented as stubs at the bottom.
//!
//! Run: `cargo test --test offline`

use transitops_backend::notifications::adapters::{
    email::EmailAdapter,
    sms::SmsAdapter,
    wecom::WeComAdapter,
    AdapterRegistry, NotificationAdapter,
};

// ── Adapter availability (no connector URL) ───────────────────────────────────

/// Email adapter is inert when no relay URL is configured.
#[test]
fn email_adapter_unavailable_when_relay_url_is_none() {
    let adapter = EmailAdapter::new(None, "noreply@test.local".to_string());
    assert!(!adapter.is_available());
}

/// SMS adapter is inert when no gateway URL is configured.
#[test]
fn sms_adapter_unavailable_when_gateway_url_is_none() {
    let adapter = SmsAdapter::new(None);
    assert!(!adapter.is_available());
}

/// WeCom adapter is inert when no webhook URL is configured.
#[test]
fn wecom_adapter_unavailable_when_webhook_url_is_none() {
    let adapter = WeComAdapter::new(None);
    assert!(!adapter.is_available());
}

// ── Adapter channel names ─────────────────────────────────────────────────────

#[test]
fn email_adapter_channel_name_is_email() {
    assert_eq!(EmailAdapter::new(None, String::new()).channel(), "email");
}

#[test]
fn sms_adapter_channel_name_is_sms() {
    assert_eq!(SmsAdapter::new(None).channel(), "sms");
}

#[test]
fn wecom_adapter_channel_name_is_wecom() {
    assert_eq!(WeComAdapter::new(None).channel(), "wecom");
}

// ── Registry availability ─────────────────────────────────────────────────────

/// When all adapters are unconfigured, `any_available()` returns false.
/// This is the default out-of-the-box behaviour — no env vars → no dispatch.
#[test]
fn registry_any_available_false_when_all_unconfigured() {
    let registry = AdapterRegistry::new(vec![
        Box::new(EmailAdapter::new(None, "noreply@test.local".to_string())),
        Box::new(SmsAdapter::new(None)),
        Box::new(WeComAdapter::new(None)),
    ]);
    assert!(!registry.any_available());
}

#[test]
fn registry_all_returns_all_adapters() {
    let registry = AdapterRegistry::new(vec![
        Box::new(EmailAdapter::new(None, String::new())),
        Box::new(SmsAdapter::new(None)),
        Box::new(WeComAdapter::new(None)),
    ]);
    assert_eq!(registry.all().len(), 3);
}

/// When all adapters are unavailable, the bus skips dispatch entirely.
/// This is documented by the `dispatch_channels` guard in bus.rs:
///   `if adapters.any_available() { dispatch_channels(...).await; }`
#[test]
fn bus_dispatch_guard_skips_when_no_available_adapters() {
    let registry = AdapterRegistry::new(vec![
        Box::new(EmailAdapter::new(None, String::new())),
    ]);
    // Verify the guard condition the bus evaluates before dispatching.
    let would_dispatch = registry.any_available();
    assert!(!would_dispatch);
}

// ── Integration test stubs (require database + channel mock) ─────────────────

// #[tokio::test]
// #[ignore = "requires database"]
// async fn inbox_delivery_succeeds_when_all_adapters_unconfigured() {
//     // Setup: AdapterRegistry with all adapters having None URLs (is_available=false)
//     //        pending event row with one subscriber
//     // Call tick_once(&pool, &registry)
//     // Assert: notifications.deliveries row with status='delivered'
//     // Assert: notifications.channel_deliveries is empty (no dispatch attempted)
// }

// #[tokio::test]
// #[ignore = "requires database + mock adapter"]
// async fn adapter_send_failure_does_not_abort_inbox_delivery() {
//     // Setup: mock adapter that always returns Err; user opted into that channel
//     //        pending event row
//     // Call fan_out_event
//     // Assert: notifications.deliveries has status='delivered' (inbox unaffected)
//     // Assert: notifications.channel_deliveries has status='failed', error_msg set
// }

// #[tokio::test]
// #[ignore = "requires database"]
// async fn unavailable_adapter_never_calls_send() {
//     // Setup: adapter with is_available()=false; user has channel preference row
//     // Call dispatch_channels
//     // Assert: send() is never invoked (validate via call counter or absence of channel_delivery row)
// }

// #[tokio::test]
// #[ignore = "requires database"]
// async fn dnd_user_gets_no_channel_dispatch() {
//     // Setup: user with DND 22:00-06:00, current mock time 23:30
//     //        event severity='info', at least one adapter configured
//     // Call fan_out_event
//     // Assert: notifications.deliveries status='queued'
//     // Assert: notifications.channel_deliveries is empty for this user
// }

// #[tokio::test]
// #[ignore = "requires database"]
// async fn critical_event_dispatched_to_channels_despite_dnd() {
//     // Setup: user with all-day DND; adapter configured
//     //        pending event with severity='critical'
//     // Call fan_out_event
//     // Assert: notifications.deliveries status='delivered' (not queued)
//     // Assert: notifications.channel_deliveries has a row (dispatch was attempted)
// }
