/// Event fan-out bus.
///
/// Poll loop (every 5 s):
///   1. process_pending_events  — fan-out to subscribers + evaluate keyword/topic rules
///   2. evaluate_periodic_rules — threshold and spike rules (poll-based)
///   3. flush_dnd_queue         — promote 'queued' deliveries when DND ends
///
/// DND handling
/// ─────────────
///   Non-critical deliveries for users in a DND window are inserted with
///   status = 'queued'. `flush_dnd_queue` promotes them to 'delivered' when
///   the window closes. Critical-severity events (and critical rule alerts)
///   bypass DND and are delivered immediately.
///
/// Deduplication
/// ─────────────
///   Before inserting a delivery the bus checks whether the same
///   (user, event_type, source_entity_id) was delivered or queued within the
///   last 15 minutes. If so, the delivery is skipped. The UNIQUE constraint
///   on (event_id, user_id) provides a safety net against concurrent inserts.
use chrono::{NaiveTime, Utc};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::notifications::{
    adapters::{AdapterRegistry, OutboundNotification},
    models::PendingEventRow,
    rules,
};

// ── Public API ────────────────────────────────────────────────────────────────

/// Execute one full bus cycle (fan-out → periodic rules → DND flush).
///
/// Called by the scheduler framework's `NotificationBusJob` every 5 seconds.
/// Returns `(events_processed, deliveries_flushed)`.
///
/// Errors in `evaluate_periodic_rules` and `flush_dnd_queue` are logged but
/// do not propagate — fan-out failures *do* propagate so the scheduler can
/// record a failed run.
pub async fn tick_once(
    pool:     &PgPool,
    adapters: &AdapterRegistry,
) -> Result<(usize, u64), sqlx::Error> {
    // 1. Fan-out pending events
    let processed = match process_pending_events(pool, adapters).await {
        Ok(n) => {
            if n > 0 { tracing::info!(count = n, "Event bus: fan-out complete"); }
            n
        }
        Err(e) => {
            tracing::error!("Event bus [process]: {}", e);
            return Err(e);
        }
    };

    // 2. Threshold / spike rules (non-fatal)
    if let Err(e) = rules::evaluate_periodic_rules(pool).await {
        tracing::error!("Event bus [periodic rules]: {}", e);
    }

    // 3. Promote queued deliveries for users whose DND window ended (non-fatal)
    let flushed = match flush_dnd_queue(pool).await {
        Ok(n)  => n,
        Err(e) => { tracing::error!("Event bus [dnd flush]: {}", e); 0 }
    };

    Ok((processed, flushed))
}

/// Spawn once in `main()`. Runs forever.
/// Kept for backwards compatibility; the scheduler uses `tick_once` directly.
pub async fn run_event_bus(pool: PgPool, adapters: AdapterRegistry) {
    let interval = Duration::from_secs(5);
    loop {
        // 1. Fan-out pending events
        match process_pending_events(&pool, &adapters).await {
            Ok(n) if n > 0 => tracing::info!(count = n, "Event bus: fan-out complete"),
            Ok(_)          => {}
            Err(e)         => tracing::error!("Event bus [process]: {}", e),
        }

        // 2. Threshold / spike rules (poll-based)
        if let Err(e) = rules::evaluate_periodic_rules(&pool).await {
            tracing::error!("Event bus [periodic rules]: {}", e);
        }

        // 3. Promote queued deliveries for users whose DND window ended
        if let Err(e) = flush_dnd_queue(&pool).await {
            tracing::error!("Event bus [dnd flush]: {}", e);
        }

        tokio::time::sleep(interval).await;
    }
}

/// Fan-out one batch (≤ 50) of unprocessed events.
/// Also evaluates keyword/topic rules across the batch.
/// Returns the number of events processed.
pub async fn process_pending_events(pool: &PgPool, adapters: &AdapterRegistry) -> Result<usize, sqlx::Error> {
    let events = sqlx::query_as::<_, PendingEventRow>(
        r#"
        SELECT e.id,
               e.event_type,
               e.source_entity_id,
               e.actor_id,
               e.payload,
               COALESCE(
                   NULLIF(e.payload->>'severity', ''),
                   ed.severity,
                   'info'
               ) AS effective_severity
        FROM   notifications.events e
        JOIN   notifications.event_definitions ed ON ed.event_type = e.event_type
        WHERE  e.processed_at IS NULL
        ORDER  BY e.created_at ASC
        LIMIT  50
        "#,
    )
    .fetch_all(pool)
    .await?;

    let count = events.len();

    for event in &events {
        if let Err(e) = fan_out_event(pool, event, adapters).await {
            tracing::error!(
                event_id   = %event.id,
                event_type = %event.event_type,
                "Fan-out failed — will retry: {}",
                e
            );
            continue; // leave processed_at NULL so it retries next poll
        }

        sqlx::query(
            "UPDATE notifications.events SET processed_at = now() WHERE id = $1",
        )
        .bind(event.id)
        .execute(pool)
        .await?;
    }

    // Keyword/topic rule evaluation — once for the whole batch (one rule load)
    if let Err(e) = rules::evaluate_event_rules(pool, &events).await {
        tracing::error!("Event rule evaluation failed: {}", e);
    }

    Ok(count)
}

// ── DND check — pub(crate) so rules.rs can use it ────────────────────────────

/// Pure time-window check extracted for unit testing.
///
/// | `dnd_enabled` | `dnd_start/end`    | Result                       |
/// |---------------|--------------------|------------------------------|
/// | false         | any                | false (DND off)              |
/// | true          | both None          | true  (all-day DND)          |
/// | true          | start ≤ end        | true when now ∈ [start, end] |
/// | true          | start >  end       | true when now ≥ start ∨ ≤ end (crosses midnight) |
pub fn is_in_dnd_window(
    dnd_enabled: bool,
    dnd_start:   Option<chrono::NaiveTime>,
    dnd_end:     Option<chrono::NaiveTime>,
    now:         chrono::NaiveTime,
) -> bool {
    if !dnd_enabled { return false; }
    match (dnd_start, dnd_end) {
        (Some(start), Some(end)) => {
            if start <= end {
                now >= start && now <= end
            } else {
                now >= start || now <= end
            }
        }
        _ => true, // all-day DND — no time window set
    }
}

/// Returns `true` if the user's DND window is currently active (UTC).
/// Returns `false` for unknown users (no preferences row).
pub(crate) async fn check_dnd(pool: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct PrefRow {
        dnd_enabled: bool,
        dnd_start:   Option<NaiveTime>,
        dnd_end:     Option<NaiveTime>,
    }

    let pref = sqlx::query_as::<_, PrefRow>(
        "SELECT dnd_enabled, dnd_start, dnd_end \
         FROM   notifications.preferences \
         WHERE  user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(match pref {
        None                          => false,
        Some(p) if !p.dnd_enabled     => false,
        Some(PrefRow { dnd_start: Some(start), dnd_end: Some(end), .. }) => {
            let now = Utc::now().naive_utc().time();
            if start <= end {
                now >= start && now <= end   // e.g. 08:00–18:00
            } else {
                now >= start || now <= end   // crosses midnight e.g. 22:00–07:00
            }
        }
        _ => true, // dnd_enabled with no time window → all-day DND
    })
}

// ── Fan-out ───────────────────────────────────────────────────────────────────

async fn fan_out_event(
    pool:     &PgPool,
    event:    &PendingEventRow,
    adapters: &AdapterRegistry,
) -> Result<(), sqlx::Error> {
    let severity   = event.effective_severity.as_str();
    let recipients = get_recipients(pool, event).await?;

    for user_id in recipients {
        // Never notify the user who triggered the event
        if event.actor_id == Some(user_id) {
            continue;
        }

        // Deduplication: skip if same (user, event_type, entity) delivered/queued within 15 min
        if check_duplicate(pool, user_id, &event.event_type, event.source_entity_id).await? {
            tracing::debug!(
                %user_id,
                event_type = %event.event_type,
                "Delivery suppressed: 15-min dedup window"
            );
            continue;
        }

        if severity != "critical" && check_dnd(pool, user_id).await? {
            // Queue for inbox delivery when DND ends.
            // External channels are not dispatched during DND — consistent with user preference.
            sqlx::query(
                "INSERT INTO notifications.deliveries (event_id, user_id, status) \
                 VALUES ($1, $2, 'queued') \
                 ON CONFLICT (event_id, user_id) DO NOTHING",
            )
            .bind(event.id)
            .bind(user_id)
            .execute(pool)
            .await?;
        } else {
            // Deliver immediately to inbox
            sqlx::query(
                "INSERT INTO notifications.deliveries \
                     (event_id, user_id, status, delivered_at) \
                 VALUES ($1, $2, 'delivered', now()) \
                 ON CONFLICT (event_id, user_id) DO NOTHING",
            )
            .bind(event.id)
            .bind(user_id)
            .execute(pool)
            .await?;

            // Dispatch to enabled external channel adapters (fire-and-forget).
            // Failures are logged and recorded but never abort inbox delivery.
            if adapters.any_available() {
                dispatch_channels(pool, event, user_id, adapters).await;
            }
        }
    }

    Ok(())
}

// ── External channel dispatch ─────────────────────────────────────────────────

/// Dispatch a notification to all active channel adapters for which the
/// recipient has a preference row with `enabled = true`.
///
/// This function never returns an error — all failures are logged and written
/// to `channel_deliveries` so they're visible for support/debugging.
async fn dispatch_channels(
    pool:     &PgPool,
    event:    &PendingEventRow,
    user_id:  Uuid,
    adapters: &AdapterRegistry,
) {
    #[derive(sqlx::FromRow)]
    struct ChanPref {
        channel:         String,
        channel_address: String,
    }

    // Load all channels the user has opted into.
    let prefs: Vec<ChanPref> = match sqlx::query_as::<_, ChanPref>(
        "SELECT channel, channel_address \
         FROM   notifications.channel_preferences \
         WHERE  user_id = $1 AND enabled = TRUE",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    {
        Ok(p)  => p,
        Err(e) => {
            tracing::warn!(%user_id, "Failed to load channel preferences: {}", e);
            return;
        }
    };

    if prefs.is_empty() {
        return;
    }

    for adapter in adapters.all() {
        if !adapter.is_available() {
            continue;
        }

        let channel = adapter.channel();

        let Some(pref) = prefs.iter().find(|p| p.channel == channel) else {
            continue;  // user has not opted into this channel
        };

        let notif = OutboundNotification::from_event(
            event,
            user_id,
            pref.channel_address.clone(),
        );

        let (status, error_msg) = match adapter.send(&notif).await {
            Ok(())  => {
                tracing::debug!(
                    channel,
                    %user_id,
                    event_type = %event.event_type,
                    "Channel dispatch sent"
                );
                ("sent", None)
            }
            Err(e) => {
                tracing::warn!(
                    channel,
                    %user_id,
                    event_type = %event.event_type,
                    "Channel dispatch failed: {}",
                    e
                );
                ("failed", Some(e.to_string()))
            }
        };

        // Record outcome — ON CONFLICT DO NOTHING guards against bus retries.
        if let Err(e) = sqlx::query(
            "INSERT INTO notifications.channel_deliveries \
                 (event_id, user_id, channel, status, error_msg) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (event_id, user_id, channel) DO NOTHING",
        )
        .bind(event.id)
        .bind(user_id)
        .bind(channel)
        .bind(status)
        .bind(error_msg)
        .execute(pool)
        .await
        {
            tracing::warn!("Failed to record channel delivery: {}", e);
        }
    }
}

async fn get_recipients(pool: &PgPool, event: &PendingEventRow) -> Result<Vec<Uuid>, sqlx::Error> {
    if event.event_type == "sys.announcement" {
        let target_roles: Option<Vec<String>> = event
            .payload
            .get("target_roles")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let is_all = target_roles
            .as_deref()
            .map_or(true, |r| r.is_empty() || r == ["all"]);

        if is_all {
            sqlx::query_scalar(
                "SELECT id FROM auth.users WHERE is_active = TRUE AND deleted_at IS NULL",
            )
            .fetch_all(pool)
            .await
        } else {
            let roles = target_roles.unwrap();
            sqlx::query_scalar(
                "SELECT u.id FROM auth.users u \
                 JOIN   auth.roles r ON r.id = u.role_id \
                 WHERE  u.is_active = TRUE AND u.deleted_at IS NULL \
                   AND  r.name = ANY($1)",
            )
            .bind(&roles)
            .fetch_all(pool)
            .await
        }
    } else {
        sqlx::query_scalar(
            "SELECT user_id FROM notifications.subscriptions WHERE event_type = $1",
        )
        .bind(&event.event_type)
        .fetch_all(pool)
        .await
    }
}

/// Returns `true` if a non-dismissed delivery for the same
/// (user, event_type, source_entity_id) exists within the last 15 minutes.
pub async fn check_duplicate(
    pool:             &PgPool,
    user_id:          Uuid,
    event_type:       &str,
    source_entity_id: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM   notifications.deliveries d
            JOIN   notifications.events     e ON e.id = d.event_id
            WHERE  d.user_id         = $1
              AND  e.event_type      = $2
              AND  (
                    ($3::UUID IS NULL AND e.source_entity_id IS NULL)
                 OR  e.source_entity_id = $3
              )
              AND  d.created_at      > now() - interval '15 minutes'
              AND  d.status         != 'dismissed'
        )
        "#,
    )
    .bind(user_id)
    .bind(event_type)
    .bind(source_entity_id)
    .fetch_one(pool)
    .await
}

// ── DND queue flush ───────────────────────────────────────────────────────────

/// Promote 'queued' deliveries to 'delivered' for every user whose DND window
/// is not currently active. Uses a single UPDATE to minimise round-trips.
///
/// DND-inactive conditions:
///   • No preferences row (no DND configured)
///   • dnd_enabled = FALSE
///   • dnd_enabled = TRUE with a time window, but that window is not currently open
///
/// All-day DND (dnd_enabled = TRUE, both times NULL) keeps deliveries queued.
pub async fn flush_dnd_queue(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        UPDATE notifications.deliveries
        SET    status       = 'delivered',
               delivered_at = now()
        WHERE  status = 'queued'
          AND  user_id IN (
              SELECT u.id
              FROM   auth.users u
              LEFT   JOIN notifications.preferences p ON p.user_id = u.id
              WHERE  u.is_active   = TRUE
                AND  u.deleted_at  IS NULL
                AND  (
                    -- No preferences row → no DND configured
                    p.user_id IS NULL

                    -- DND explicitly disabled
                    OR p.dnd_enabled = FALSE

                    -- DND with a time window that is NOT currently active
                    OR (
                        p.dnd_enabled = TRUE
                        AND p.dnd_start IS NOT NULL
                        AND p.dnd_end   IS NOT NULL
                        AND NOT (
                            CASE
                                WHEN p.dnd_start <= p.dnd_end THEN
                                    now()::TIME BETWEEN p.dnd_start AND p.dnd_end
                                ELSE
                                    -- crosses midnight
                                    now()::TIME >= p.dnd_start
                                    OR now()::TIME <= p.dnd_end
                            END
                        )
                    )
                )
          )
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();

    if rows > 0 {
        tracing::info!(count = rows, "DND flush: promoted queued deliveries");
    }

    Ok(rows)
}
