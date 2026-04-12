use actix_web::{web, HttpResponse};
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth::middleware::AuthSession,
    error::AppError,
    notifications::{
        bus,
        models::{
            AnnounceRequest, ChannelPreferenceResponse, CreateRuleRequest, DeliveryQuery,
            DeliveryResponse, DeliveryRow, EventDefRow, PreferencesResponse, PreferencesRow,
            ReceiptRequest, SubscriptionInfo, SubscriptionRuleResponse, SubscriptionRuleRow,
            UnreadCountResponse, UpdatePreferencesRequest, UpdateRuleRequest,
            UpdateSubscriptionsRequest, UpsertChannelRequest, VALID_CHANNELS,
        },
        rules::validate_rule_config,
    },
    rbac::permissions::Permission,
    AppState,
};

// ============================================================
// Inbox
// ============================================================

/// GET /notifications
///
/// Query params:
///   status  = unread (default) | queued | read | dismissed | all
///   limit   = 1–100  (default 50)
///   offset  = default 0
pub async fn list_deliveries(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<DeliveryQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsInboxRead)?;

    let limit  = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    // Map caller-facing status label to the actual DB value.
    // "unread" is an alias for the 'delivered' DB status.
    let sql_status: Option<&str> = match query.status.as_deref().unwrap_or("unread") {
        "all"       => None,
        "unread"    => Some("delivered"),
        "queued"    => Some("queued"),
        "read"      => Some("read"),
        "dismissed" => Some("dismissed"),
        other       => {
            return Err(AppError::BadRequest(format!(
                "Invalid status '{}'. Valid: unread, queued, read, dismissed, all",
                other
            )))
        }
    };

    let rows = sqlx::query_as::<_, DeliveryRow>(
        r#"
        SELECT d.id,
               d.event_id,
               e.event_type,
               COALESCE(
                   NULLIF(e.payload->>'severity', ''),
                   ed.severity,
                   'info'
               )                   AS severity,
               e.source_entity_id,
               e.payload,
               d.status,
               d.delivered_at,
               d.read_at,
               d.created_at
        FROM   notifications.deliveries       d
        JOIN   notifications.events           e  ON e.id = d.event_id
        JOIN   notifications.event_definitions ed ON ed.event_type = e.event_type
        WHERE  d.user_id = $1
          AND  ($2::TEXT IS NULL OR d.status = $2)
        ORDER  BY d.created_at DESC
        LIMIT  $3
        OFFSET $4
        "#,
    )
    .bind(session.user_id)
    .bind(sql_status)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<DeliveryResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// GET /notifications/unread-count
///
/// Returns `{"unread": N, "queued": N}` for badge counters.
pub async fn unread_count(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsInboxRead)?;

    let unread: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications.deliveries \
         WHERE user_id = $1 AND status = 'delivered'",
    )
    .bind(session.user_id)
    .fetch_one(&state.db)
    .await?;

    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications.deliveries \
         WHERE user_id = $1 AND status = 'queued'",
    )
    .bind(session.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(UnreadCountResponse { unread, queued }))
}

/// POST /notifications/{id}/read
pub async fn mark_read(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsInboxRead)?;

    let affected = sqlx::query(
        "UPDATE notifications.deliveries \
         SET    status = 'read', read_at = now() \
         WHERE  id = $1 AND user_id = $2 AND status IN ('delivered', 'queued')",
    )
    .bind(path.into_inner())
    .bind(session.user_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(
            "Notification not found or already read".to_string(),
        ));
    }
    Ok(HttpResponse::Ok().json(json!({ "message": "Marked as read" })))
}

/// POST /notifications/read-all
pub async fn read_all(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsInboxRead)?;

    let affected = sqlx::query(
        "UPDATE notifications.deliveries \
         SET    status = 'read', read_at = now() \
         WHERE  user_id = $1 AND status IN ('delivered', 'queued')",
    )
    .bind(session.user_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    Ok(HttpResponse::Ok().json(
        json!({ "message": "All notifications marked as read", "count": affected }),
    ))
}

/// POST /notifications/{id}/dismiss
pub async fn dismiss(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsInboxRead)?;

    let affected = sqlx::query(
        "UPDATE notifications.deliveries \
         SET    status = 'dismissed' \
         WHERE  id = $1 AND user_id = $2 AND status != 'dismissed'",
    )
    .bind(path.into_inner())
    .bind(session.user_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(
            "Notification not found or already dismissed".to_string(),
        ));
    }
    Ok(HttpResponse::Ok().json(json!({ "message": "Notification dismissed" })))
}

// ============================================================
// DND preferences
// ============================================================

/// GET /notifications/preferences
pub async fn get_preferences(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsDndManage)?;

    // Ensure a row exists — if not, the DB DEFAULT creates 22:00–07:00 DND
    sqlx::query(
        "INSERT INTO notifications.preferences (user_id) VALUES ($1) ON CONFLICT DO NOTHING",
    )
    .bind(session.user_id)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, PreferencesRow>(
        "SELECT dnd_enabled, dnd_start, dnd_end, updated_at \
         FROM   notifications.preferences WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(PreferencesResponse::from(row)))
}

/// PUT /notifications/preferences
///
/// Body: `{ "dnd_enabled": true, "dnd_start": "22:00:00", "dnd_end": "07:00:00" }`
///
/// Omit `dnd_start`/`dnd_end` for all-day DND.
pub async fn update_preferences(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<UpdatePreferencesRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsDndManage)?;

    if body.dnd_enabled && body.dnd_start.is_some() != body.dnd_end.is_some() {
        return Err(AppError::BadRequest(
            "dnd_start and dnd_end must both be provided together, or both omitted".to_string(),
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO notifications.preferences
            (user_id, dnd_enabled, dnd_start, dnd_end, updated_at)
        VALUES ($1, $2, $3, $4, now())
        ON CONFLICT (user_id) DO UPDATE
        SET    dnd_enabled = EXCLUDED.dnd_enabled,
               dnd_start   = EXCLUDED.dnd_start,
               dnd_end     = EXCLUDED.dnd_end,
               updated_at  = now()
        "#,
    )
    .bind(session.user_id)
    .bind(body.dnd_enabled)
    .bind(body.dnd_start)
    .bind(body.dnd_end)
    .execute(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(json!({ "message": "Preferences updated" })))
}

// ============================================================
// Event-type subscriptions
// ============================================================

/// GET /notifications/subscriptions
pub async fn list_subscriptions(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let all_types = sqlx::query_as::<_, EventDefRow>(
        "SELECT event_type, description, severity \
         FROM   notifications.event_definitions ORDER BY event_type",
    )
    .fetch_all(&state.db)
    .await?;

    let subscribed: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM notifications.subscriptions WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<SubscriptionInfo> = all_types
        .into_iter()
        .map(|r| SubscriptionInfo {
            subscribed: subscribed.contains(&r.event_type),
            event_type:  r.event_type,
            description: r.description,
            severity:    r.severity,
        })
        .collect();

    Ok(HttpResponse::Ok().json(resp))
}

/// PUT /notifications/subscriptions
///
/// Bulk-replace the caller's event-type subscriptions.
pub async fn update_subscriptions(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<UpdateSubscriptionsRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    if !body.event_types.is_empty() {
        let valid: Vec<String> = sqlx::query_scalar(
            "SELECT event_type FROM notifications.event_definitions \
             WHERE  event_type = ANY($1)",
        )
        .bind(&body.event_types)
        .fetch_all(&state.db)
        .await?;

        let unknown: Vec<&str> = body
            .event_types
            .iter()
            .filter(|et| !valid.contains(et))
            .map(String::as_str)
            .collect();

        if !unknown.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Unknown event type(s): {}",
                unknown.join(", ")
            )));
        }
    }

    let mut tx = state.db.begin().await?;

    sqlx::query("DELETE FROM notifications.subscriptions WHERE user_id = $1")
        .bind(session.user_id)
        .execute(&mut *tx)
        .await?;

    for et in &body.event_types {
        sqlx::query(
            "INSERT INTO notifications.subscriptions (user_id, event_type) \
             VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(session.user_id)
        .bind(et)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(HttpResponse::Ok().json(
        json!({ "message": "Subscriptions updated", "count": body.event_types.len() }),
    ))
}

// ============================================================
// Subscription rules
// ============================================================

/// GET /notifications/rules
pub async fn list_rules(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let rows = sqlx::query_as::<_, SubscriptionRuleRow>(
        r#"
        SELECT id, user_id, rule_name, rule_type, is_enabled, config,
               severity_override, cooldown_minutes, last_triggered_at,
               created_at, updated_at
        FROM   notifications.subscription_rules
        WHERE  user_id = $1
        ORDER  BY created_at DESC
        "#,
    )
    .bind(session.user_id)
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<SubscriptionRuleResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// POST /notifications/rules
///
/// Create a new subscription rule. Config is validated against the rule_type schema.
pub async fn create_rule(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<CreateRuleRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    if body.rule_name.trim().is_empty() {
        return Err(AppError::BadRequest("rule_name is required".to_string()));
    }

    let cooldown = body.cooldown_minutes.unwrap_or(15).max(1);

    if let Some(sev) = &body.severity_override {
        if !["info", "warning", "critical"].contains(&sev.as_str()) {
            return Err(AppError::BadRequest(
                "severity_override must be info, warning, or critical".to_string(),
            ));
        }
    }

    // Validate config schema for the requested rule_type
    validate_rule_config(&body.rule_type, &body.config)
        .map_err(|e| AppError::BadRequest(format!("Invalid config: {}", e)))?;

    let row = sqlx::query_as::<_, SubscriptionRuleRow>(
        r#"
        INSERT INTO notifications.subscription_rules
            (user_id, rule_name, rule_type, config, severity_override, cooldown_minutes)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, user_id, rule_name, rule_type, is_enabled, config,
                  severity_override, cooldown_minutes, last_triggered_at,
                  created_at, updated_at
        "#,
    )
    .bind(session.user_id)
    .bind(body.rule_name.trim())
    .bind(&body.rule_type)
    .bind(&body.config)
    .bind(&body.severity_override)
    .bind(cooldown)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(
        rule_id   = %row.id,
        rule_type = %row.rule_type,
        user_id   = %session.user_id,
        "Subscription rule created"
    );

    Ok(HttpResponse::Created().json(SubscriptionRuleResponse::from(row)))
}

/// GET /notifications/rules/{id}
pub async fn get_rule(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let rule_id = path.into_inner();
    let row = sqlx::query_as::<_, SubscriptionRuleRow>(
        r#"
        SELECT id, user_id, rule_name, rule_type, is_enabled, config,
               severity_override, cooldown_minutes, last_triggered_at,
               created_at, updated_at
        FROM   notifications.subscription_rules
        WHERE  id = $1 AND user_id = $2
        "#,
    )
    .bind(rule_id)
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Rule not found".to_string()))?;

    Ok(HttpResponse::Ok().json(SubscriptionRuleResponse::from(row)))
}

/// PUT /notifications/rules/{id}
///
/// Partial update — any omitted fields remain unchanged.
pub async fn update_rule(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<UpdateRuleRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let rule_id = path.into_inner();

    // Load current state to validate new config against existing rule_type
    let current = sqlx::query_as::<_, SubscriptionRuleRow>(
        r#"
        SELECT id, user_id, rule_name, rule_type, is_enabled, config,
               severity_override, cooldown_minutes, last_triggered_at,
               created_at, updated_at
        FROM   notifications.subscription_rules
        WHERE  id = $1 AND user_id = $2
        "#,
    )
    .bind(rule_id)
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Rule not found".to_string()))?;

    // Validate new config if provided
    if let Some(new_cfg) = &body.config {
        validate_rule_config(&current.rule_type, new_cfg)
            .map_err(|e| AppError::BadRequest(format!("Invalid config: {}", e)))?;
    }

    if let Some(sev) = &body.severity_override {
        if !["info", "warning", "critical"].contains(&sev.as_str()) {
            return Err(AppError::BadRequest(
                "severity_override must be info, warning, or critical".to_string(),
            ));
        }
    }

    let row = sqlx::query_as::<_, SubscriptionRuleRow>(
        r#"
        UPDATE notifications.subscription_rules
        SET    rule_name         = COALESCE($3, rule_name),
               config            = COALESCE($4, config),
               severity_override = COALESCE($5, severity_override),
               cooldown_minutes  = COALESCE($6, cooldown_minutes),
               updated_at        = now()
        WHERE  id = $1 AND user_id = $2
        RETURNING id, user_id, rule_name, rule_type, is_enabled, config,
                  severity_override, cooldown_minutes, last_triggered_at,
                  created_at, updated_at
        "#,
    )
    .bind(rule_id)
    .bind(session.user_id)
    .bind(body.rule_name.as_deref())
    .bind(body.config.as_ref())
    .bind(body.severity_override.as_deref())
    .bind(body.cooldown_minutes)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(SubscriptionRuleResponse::from(row)))
}

/// DELETE /notifications/rules/{id}
pub async fn delete_rule(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let affected = sqlx::query(
        "DELETE FROM notifications.subscription_rules WHERE id = $1 AND user_id = $2",
    )
    .bind(path.into_inner())
    .bind(session.user_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound("Rule not found".to_string()));
    }
    Ok(HttpResponse::Ok().json(json!({ "message": "Rule deleted" })))
}

/// POST /notifications/rules/{id}/toggle
///
/// Flip `is_enabled` without changing any other field.
pub async fn toggle_rule(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let row = sqlx::query_as::<_, SubscriptionRuleRow>(
        r#"
        UPDATE notifications.subscription_rules
        SET    is_enabled  = NOT is_enabled,
               updated_at  = now()
        WHERE  id = $1 AND user_id = $2
        RETURNING id, user_id, rule_name, rule_type, is_enabled, config,
                  severity_override, cooldown_minutes, last_triggered_at,
                  created_at, updated_at
        "#,
    )
    .bind(path.into_inner())
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Rule not found".to_string()))?;

    Ok(HttpResponse::Ok().json(json!({
        "id":         row.id,
        "is_enabled": row.is_enabled,
        "message":    if row.is_enabled { "Rule enabled" } else { "Rule disabled" },
    })))
}

// ============================================================
// Announcements
// ============================================================

/// POST /notifications/announce
///
/// Broadcast a system announcement to all active users (or a role subset).
/// Triggers immediate fan-out without waiting for the background bus poll.
/// Requires `SysAnnouncementWrite` (operations_admin only).
pub async fn announce(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<AnnounceRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::SysAnnouncementWrite)?;

    let severity = body.severity.as_deref().unwrap_or("info");
    if !["info", "warning", "critical"].contains(&severity) {
        return Err(AppError::BadRequest(
            "severity must be one of: info, warning, critical".to_string(),
        ));
    }
    if body.title.trim().is_empty() || body.message.trim().is_empty() {
        return Err(AppError::BadRequest(
            "title and message are required and must not be blank".to_string(),
        ));
    }

    let payload = serde_json::json!({
        "title":        body.title.trim(),
        "message":      body.message.trim(),
        "severity":     severity,
        "target_roles": body.target_roles,
    });

    let event_id: Uuid = sqlx::query_scalar(
        "INSERT INTO notifications.events \
             (event_type, source_domain, actor_id, payload) \
         VALUES ('sys.announcement', 'sys', $1, $2) \
         RETURNING id",
    )
    .bind(session.user_id)
    .bind(&payload)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(
        %event_id,
        actor    = %session.user_id,
        severity,
        "Announcement broadcast initiated"
    );

    // Immediate fan-out (bus will skip this event on its next poll since processed_at is set)
    // Immediate fan-out is handled by the background job scheduler.
    Ok(HttpResponse::Ok().json(json!({
        "event_id": event_id,
        "message":  "Announcement broadcast queued for delivery",
    })))
}

// ============================================================
// Single notification + Delivery receipts
// ============================================================

/// GET /notifications/{id}
///
/// Fetch a single notification by its delivery ID.
/// Used when the inbox panel opens a detail drawer/modal.
pub async fn get_notification(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsInboxRead)?;

    let delivery_id = path.into_inner();

    let row = sqlx::query_as::<_, DeliveryRow>(
        r#"
        SELECT d.id,
               d.event_id,
               e.event_type,
               COALESCE(
                   NULLIF(e.payload->>'severity', ''),
                   ed.severity,
                   'info'
               )                   AS severity,
               e.source_entity_id,
               e.payload,
               d.status,
               d.delivered_at,
               d.read_at,
               d.created_at
        FROM   notifications.deliveries       d
        JOIN   notifications.events           e  ON e.id = d.event_id
        JOIN   notifications.event_definitions ed ON ed.event_type = e.event_type
        WHERE  d.id = $1 AND d.user_id = $2
        "#,
    )
    .bind(delivery_id)
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

    Ok(HttpResponse::Ok().json(DeliveryResponse::from(row)))
}

/// POST /notifications/receipt
///
/// Frontend delivery receipt — called by the Yew inbox when it renders
/// a batch of notifications. Serves two purposes:
///   1. Confirms the client has displayed the notifications (auditable).
///   2. Promotes any 'queued' deliveries in the list to 'delivered', so a user
///      actively viewing their inbox sees DND-queued items immediately.
///
/// Body: `{ "delivery_ids": ["uuid", ...] }`
/// Response: `{ "promoted": N }` — number of queued items promoted.
pub async fn receipt(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<ReceiptRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsInboxRead)?;

    if body.delivery_ids.is_empty() {
        return Ok(HttpResponse::Ok().json(json!({ "promoted": 0 })));
    }

    // Promote queued → delivered for any IDs that belong to this user.
    // The user is online and looking at their inbox, so DND no longer blocks display.
    let promoted = sqlx::query(
        r#"
        UPDATE notifications.deliveries
        SET    status       = 'delivered',
               delivered_at = now()
        WHERE  id      = ANY($1)
          AND  user_id = $2
          AND  status  = 'queued'
        "#,
    )
    .bind(&body.delivery_ids)
    .bind(session.user_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    Ok(HttpResponse::Ok().json(json!({ "promoted": promoted })))
}

// ============================================================
// Channel preferences
// ============================================================

/// GET /notifications/channels
///
/// Returns all channel preferences (email, sms, wecom) for the authenticated
/// user.  Channels with no preference row are omitted — clients should treat
/// their absence as "not configured".
pub async fn list_channel_prefs(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let rows = sqlx::query_as::<_, ChannelPreferenceResponse>(
        r#"
        SELECT channel, enabled, channel_address, updated_at
        FROM   notifications.channel_preferences
        WHERE  user_id = $1
        ORDER  BY channel
        "#,
    )
    .bind(session.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(rows))
}

/// PUT /notifications/channels/{channel}
///
/// Create or update a channel preference for the authenticated user.
///
/// `channel` must be one of: `email`, `sms`, `wecom`.
/// The `channel_address` must be non-empty:
///   - **email**  — a valid email address
///   - **sms**    — an E.164 phone number (e.g. `+8613812345678`)
///   - **wecom**  — the user's WeCom internal account ID
///
/// Channel dispatching only occurs when the server-side adapter for that
/// channel has its connector URL configured (`is_available() == true`).
pub async fn upsert_channel_pref(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<String>,
    body:    web::Json<UpsertChannelRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let channel = path.into_inner();
    if !VALID_CHANNELS.contains(&channel.as_str()) {
        return Err(AppError::BadRequest(format!(
            "channel must be one of: {}",
            VALID_CHANNELS.join(", ")
        )));
    }

    let addr = body.channel_address.as_deref().map(str::trim);
    if let Some(a) = addr {
        if a.is_empty() {
            return Err(AppError::BadRequest(
                "channel_address must not be empty".to_string(),
            ));
        }
    }

    let enabled = body.enabled.unwrap_or(true);

    // When channel_address is not provided, keep the existing value on conflict.
    // On insert, use a placeholder if addr is None (only valid if updating).
    let row = sqlx::query_as::<_, ChannelPreferenceResponse>(
        r#"
        INSERT INTO notifications.channel_preferences
            (user_id, channel, enabled, channel_address, updated_at)
        VALUES ($1, $2, $3, COALESCE($4, 'none'), now())
        ON CONFLICT (user_id, channel) DO UPDATE
            SET enabled         = EXCLUDED.enabled,
                channel_address = COALESCE(NULLIF($4, ''), channel_preferences.channel_address),
                updated_at      = now()
        RETURNING channel, enabled, channel_address, updated_at
        "#,
    )
    .bind(session.user_id)
    .bind(&channel)
    .bind(enabled)
    .bind(addr)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(row))
}

/// DELETE /notifications/channels/{channel}
///
/// Remove a channel preference.  The user will no longer receive notifications
/// on this channel.  The inbox delivery path is unaffected.
pub async fn delete_channel_pref(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<String>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::NotificationsSubscriptionsManage)?;

    let channel = path.into_inner();
    if !VALID_CHANNELS.contains(&channel.as_str()) {
        return Err(AppError::BadRequest(format!(
            "channel must be one of: {}",
            VALID_CHANNELS.join(", ")
        )));
    }

    let _affected = sqlx::query(
        "DELETE FROM notifications.channel_preferences \
         WHERE user_id = $1 AND channel = $2",
    )
    .bind(session.user_id)
    .bind(&channel)
    .execute(&state.db)
    .await?
    .rows_affected();

    Ok(HttpResponse::NoContent().finish())
}
