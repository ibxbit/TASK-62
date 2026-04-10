use actix_web::{web, App, HttpServer};
use dotenvy::dotenv;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use transitops_backend::{
    alerting, audit, auth, config, crypto, db,
    dispatcher, notifications, ops, payments,
    reconciliation, reporting, scheduler, AppState,
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Load .env (ignored if absent — production uses real env vars)
    dotenv().ok();

    // Structured logging; set RUST_LOG=info or RUST_LOG=debug to control verbosity
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cfg  = config::Config::from_env();
    let pool = db::create_pool(&cfg.database_url)
        .await
        .expect("Failed to connect to PostgreSQL");

    // Build the field encryptor — validates the key length at startup so a
    // misconfigured ENCRYPTION_KEY fails fast rather than at first DB write.
    let encryptor = crypto::FieldEncryptor::from_hex_key(&cfg.encryption_key)
        .expect("ENCRYPTION_KEY invalid: must be 64 hex characters (32-byte AES-256 key)");
    let encryptor = match &cfg.encryption_key_previous {
        Some(prev) => encryptor
            .with_previous_key(prev)
            .expect("ENCRYPTION_KEY_PREVIOUS invalid: must be 64 hex characters"),
        None => encryptor,
    };

    let bind_addr = format!("{}:{}", cfg.server_host, cfg.server_port);
    tracing::info!("TransitOps backend listening on http://{}", bind_addr);

    let state = web::Data::new(AppState { db: pool.clone(), config: cfg, crypto: encryptor });

    // Build pluggable notification channel adapters.
    // Each adapter is inert when its env var connector URL is absent.
    let adapter_registry = {
        use notifications::adapters::{
            AdapterRegistry,
            email::EmailAdapter,
            sms::SmsAdapter,
            wecom::WeComAdapter,
        };
        AdapterRegistry::new(vec![
            Box::new(EmailAdapter::new(
                state.config.email_relay_url.clone(),
                state.config.email_from_addr.clone(),
            )),
            Box::new(SmsAdapter::new(state.config.sms_gateway_url.clone())),
            Box::new(WeComAdapter::new(state.config.wecom_webhook_url.clone())),
        ])
    };

    // ── Background job scheduler ──────────────────────────────────────────────
    // Creates a pool-backed scheduler, repairs any stale `running` records from
    // a previous crash, then registers all jobs as independent tokio tasks.
    //
    // Graceful shutdown: when the HTTP server stops (SIGTERM / Ctrl-C), main()
    // returns, the tokio runtime drops, and all tasks are cancelled.  Jobs that
    // are currently sleeping exit within ~500 ms.  In-flight jobs complete first.
    {
        use scheduler::{Scheduler, jobs::{
            notification_bus::NotificationBusJob,
            payment_compensation::PaymentCompensationJob,
            kpi_anomaly::KpiAnomalyJob,
            system_maintenance::SystemMaintenanceJob,
            dedup_cleanup::DedupCleanupJob,
        }};

        let sched = Scheduler::new(pool.clone());

        if let Err(e) = sched.recover_stale_runs().await {
            tracing::warn!(error = %e, "Could not recover stale job_runs records");
        }

        // 5 s — event fan-out, DND flush, keyword/spike rule evaluation
        sched.spawn(NotificationBusJob { adapters: adapter_registry });

        // 15 min — stuck transactions, stuck refunds, unprocessed callbacks
        sched.spawn(PaymentCompensationJob);

        // 60 s — fires scheduled reports, auto-publishes configs, and activates rollout stages
        sched.spawn(SystemMaintenanceJob);

        // 30 min — compare KPI snapshots against rolling average, raise alerts
        sched.spawn(KpiAnomalyJob);

        // 1 h — prune job_runs (7 d), channel_deliveries (30 d), dismissed
        //        inbox entries (90 d), orphaned notification events (90 d)
        sched.spawn(DedupCleanupJob);
    }

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            // Reject payloads larger than 64 KiB for auth endpoints
            .app_data(web::JsonConfig::default().limit(65_536))
            .configure(auth::configure_routes)
            .configure(ops::configure_routes)
            .configure(dispatcher::configure_routes)
            .configure(notifications::configure_routes)
            .configure(payments::configure_routes)
            .configure(reconciliation::configure_routes)
            .configure(reporting::configure_routes)
            .configure(alerting::configure_routes)
            .configure(audit::configure_routes)
    })
    .bind(&bind_addr)?
    .run()
    .await
}
