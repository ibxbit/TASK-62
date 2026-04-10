use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// Request bodies
// ============================================================

#[derive(Deserialize)]
pub struct PatchTripRequest {
    pub scheduled_departure:  Option<DateTime<Utc>>,
    pub scheduled_arrival:    Option<DateTime<Utc>>,
    pub assigned_driver_id:   Option<Uuid>,
    pub note:                 Option<String>,
}

#[derive(Deserialize)]
pub struct AssignDriverRequest {
    pub driver_id: Uuid,
    pub note:      Option<String>,
}

#[derive(Deserialize)]
pub struct StartTripRequest {
    /// Defaults to now() if omitted.
    pub actual_departure: Option<DateTime<Utc>>,
    pub note:             Option<String>,
}

#[derive(Deserialize)]
pub struct CompleteTripRequest {
    /// Defaults to now() if omitted.
    pub actual_arrival: Option<DateTime<Utc>>,
    pub note:           Option<String>,
}

#[derive(Deserialize)]
pub struct CancelTripRequest {
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct ResolveConflictRequest {
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct UpcomingQuery {
    /// Look-ahead window in minutes (default 120, max 480).
    pub window_minutes: Option<i64>,
}

// ============================================================
// Response types
// ============================================================

#[derive(Serialize)]
pub struct TripSummary {
    pub id:                   Uuid,
    pub trip_code:            String,
    pub route_name:           String,
    pub status:               String,
    pub scheduled_departure:  DateTime<Utc>,
    pub scheduled_arrival:    DateTime<Utc>,
    pub actual_departure:     Option<DateTime<Utc>>,
    pub actual_arrival:       Option<DateTime<Utc>>,
    pub assigned_driver_id:   Option<Uuid>,
    pub driver_username:      Option<String>,
    pub minutes_until_start:  i64,
    pub has_open_conflicts:   bool,
}

#[derive(Serialize)]
pub struct ConflictResponse {
    pub id:              Uuid,
    pub conflict_type:   String,
    pub trip_id_1:       Uuid,
    pub trip_code_1:     String,
    pub trip_id_2:       Option<Uuid>,
    pub trip_code_2:     Option<String>,
    pub description:     String,
    pub severity:        String,
    pub status:          String,
    pub detected_at:     DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub resolved_at:     Option<DateTime<Utc>>,
    pub notes:           Option<String>,
}

#[derive(Serialize)]
pub struct ConflictCheckResult {
    pub trip_id:       Uuid,
    pub new_conflicts: usize,
    pub conflicts:     Vec<ConflictResponse>,
}

#[derive(Serialize)]
pub struct DashboardResponse {
    pub active_trips_count:         i64,
    pub upcoming_2h_count:          i64,
    pub open_conflicts_count:       i64,
    pub unassigned_within_30min:    i64,
    pub active_trips:               Vec<TripSummary>,
    pub upcoming_trips:             Vec<TripSummary>,
    pub recent_conflicts:           Vec<ConflictResponse>,
}

#[derive(Serialize)]
pub struct ApproachingCheckResult {
    pub events_emitted:         usize,
    pub conflicts_created:      usize,
    pub approaching_trip_ids:   Vec<Uuid>,
}

// ============================================================
// DB row types
// ============================================================

#[derive(sqlx::FromRow)]
pub struct TripSummaryRow {
    pub id:                   Uuid,
    pub trip_code:            String,
    pub route_name:           String,
    pub status:               String,
    pub scheduled_departure:  DateTime<Utc>,
    pub scheduled_arrival:    DateTime<Utc>,
    pub actual_departure:     Option<DateTime<Utc>>,
    pub actual_arrival:       Option<DateTime<Utc>>,
    pub assigned_driver_id:   Option<Uuid>,
    pub driver_username:      Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct ConflictRow {
    pub id:              Uuid,
    pub conflict_type:   String,
    pub trip_id_1:       Uuid,
    pub trip_code_1:     String,
    pub trip_id_2:       Option<Uuid>,
    pub trip_code_2:     Option<String>,
    pub description:     String,
    pub severity:        String,
    pub status:          String,
    pub detected_at:     DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub resolved_at:     Option<DateTime<Utc>>,
    pub notes:           Option<String>,
}

// ============================================================
// Conversions
// ============================================================

impl From<ConflictRow> for ConflictResponse {
    fn from(r: ConflictRow) -> Self {
        ConflictResponse {
            id:              r.id,
            conflict_type:   r.conflict_type,
            trip_id_1:       r.trip_id_1,
            trip_code_1:     r.trip_code_1,
            trip_id_2:       r.trip_id_2,
            trip_code_2:     r.trip_code_2,
            description:     r.description,
            severity:        r.severity,
            status:          r.status,
            detected_at:     r.detected_at,
            acknowledged_at: r.acknowledged_at,
            resolved_at:     r.resolved_at,
            notes:           r.notes,
        }
    }
}
