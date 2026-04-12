use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// ============================================================
// Shared / pagination
// ============================================================

#[derive(Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

impl ListQuery {
    pub fn offset(&self) -> i64 {
        let page = self.page.unwrap_or(1).max(1);
        let per_page = self.per_page.unwrap_or(20).clamp(1, 100);
        (page - 1) * per_page
    }
    pub fn limit(&self) -> i64 {
        self.per_page.unwrap_or(20).clamp(1, 100)
    }
}

#[derive(Serialize)]
pub struct ListResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Serialize)]
pub struct OkResponse {
    pub message: &'static str,
}

// ============================================================
// Routes
// ============================================================

#[derive(Deserialize)]
pub struct CreateRouteRequest {
    #[serde(rename = "route_code", alias = "code")]
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub effective_from: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct UpdateRouteRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub effective_from: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct ScheduleRequest {
    pub effective_from: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct RouteResponse {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub effective_from: Option<DateTime<Utc>>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct RouteDetailResponse {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub effective_from: Option<DateTime<Utc>>,
    pub version: i32,
    pub stops: Vec<StopResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================
// Stops
// ============================================================

#[derive(Deserialize)]
pub struct CreateStopRequest {
    pub code: String,
    pub name: String,
    pub sequence_order: i16,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Deserialize)]
pub struct UpdateStopRequest {
    pub name: Option<String>,
    pub sequence_order: Option<i16>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Serialize)]
pub struct StopResponse {
    pub id: Uuid,
    pub route_id: Uuid,
    pub code: String,
    pub name: String,
    pub sequence_order: i16,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================
// Trips
// ============================================================

#[derive(Deserialize)]
pub struct CreateTripRequest {
    pub route_id: Uuid,
    pub trip_code: String,
    pub scheduled_departure: DateTime<Utc>,
    pub scheduled_arrival: DateTime<Utc>,
    pub assigned_driver_id: Option<Uuid>,
    pub calendar_id: Option<Uuid>,
    pub effective_from: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct UpdateTripRequest {
    pub scheduled_departure: Option<DateTime<Utc>>,
    pub scheduled_arrival: Option<DateTime<Utc>>,
    pub assigned_driver_id: Option<Uuid>,
    pub calendar_id: Option<Uuid>,
    pub effective_from: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct TripResponse {
    pub id: Uuid,
    pub route_id: Uuid,
    pub trip_code: String,
    pub status: String,
    pub scheduled_departure: DateTime<Utc>,
    pub scheduled_arrival: DateTime<Utc>,
    pub actual_departure: Option<DateTime<Utc>>,
    pub actual_arrival: Option<DateTime<Utc>>,
    pub assigned_driver_id: Option<Uuid>,
    pub calendar_id: Option<Uuid>,
    pub effective_from: Option<DateTime<Utc>>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================
// Trip Calendars
// ============================================================

#[derive(Deserialize)]
pub struct CreateCalendarRequest {
    pub name: String,
    pub description: Option<String>,
    pub days_of_week: Vec<i16>, // 0=Sun…6=Sat
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
    pub exception_dates: Option<CalendarExceptions>,
}

#[derive(Deserialize)]
pub struct UpdateCalendarRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub days_of_week: Option<Vec<i16>>,
    pub valid_from: Option<NaiveDate>,
    pub valid_to: Option<NaiveDate>,
    pub exception_dates: Option<CalendarExceptions>,
}

#[derive(Deserialize, Serialize)]
pub struct CalendarExceptions {
    pub included: Vec<NaiveDate>, // extra service dates
    pub excluded: Vec<NaiveDate>, // no-service dates
}

#[derive(Serialize)]
pub struct CalendarResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub days_of_week: Vec<i16>,
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
    pub exception_dates: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================
// Config versions
// ============================================================

#[derive(Deserialize)]
pub struct CreateConfigVersionRequest {
    pub payload: JsonValue,
    pub based_on_version: Option<Uuid>, // fork from an existing version
}

#[derive(Deserialize)]
pub struct UpdateConfigVersionRequest {
    pub payload: JsonValue,
}

#[derive(Deserialize)]
pub struct ScheduleConfigRequest {
    pub effective_from: DateTime<Utc>,
    pub effective_to: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct DiffQuery {
    pub v1: Uuid,
    pub v2: Uuid,
}

#[derive(Serialize)]
pub struct ConfigVersionResponse {
    pub id: Uuid,
    pub template_id: Uuid,
    pub template_key: String,
    pub version_number: i32,
    pub status: String,
    pub payload: JsonValue,
    pub effective_from: Option<DateTime<Utc>>,
    pub effective_to: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================
// Rollout
// ============================================================

#[derive(Deserialize)]
pub struct CreateRolloutRequest {
    pub stages: Vec<RolloutStageSpec>,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct RolloutStageSpec {
    /// Informational percentage label (10, 50, 100).
    pub target_percentage: i16,
    pub depot_ids: Vec<Uuid>,
    /// If set, stage auto-activates at this time (requires a scheduler job).
    pub scheduled_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct RolloutPlanResponse {
    pub id: Uuid,
    pub config_version_id: Uuid,
    pub status: String,
    pub total_depots: i32,
    pub current_stage: i32,
    pub stages: Vec<RolloutStageResponse>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct RolloutStageResponse {
    pub id: Uuid,
    pub stage_number: i16,
    pub target_percentage: i16,
    pub depot_count: usize,
    pub status: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
}

// ============================================================
// DB row types
// ============================================================

#[derive(sqlx::FromRow)]
pub struct RouteRow {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub effective_from: Option<DateTime<Utc>>,
    pub entity_version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct StopRow {
    pub id: Uuid,
    pub route_id: Uuid,
    pub code: String,
    pub name: String,
    pub sequence_order: i16,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct TripRow {
    pub id: Uuid,
    pub route_id: Uuid,
    pub trip_code: String,
    pub status: String,
    pub scheduled_departure: DateTime<Utc>,
    pub scheduled_arrival: DateTime<Utc>,
    pub actual_departure: Option<DateTime<Utc>>,
    pub actual_arrival: Option<DateTime<Utc>>,
    pub assigned_driver_id: Option<Uuid>,
    pub calendar_id: Option<Uuid>,
    pub effective_from: Option<DateTime<Utc>>,
    pub entity_version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct CalendarRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub days_of_week: Vec<i16>,
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
    pub exception_dates: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct ConfigVersionRow {
    pub id: Uuid,
    pub template_id: Uuid,
    pub template_key: String,
    pub version_number: i32,
    pub status: String,
    pub payload: JsonValue,
    pub effective_from: Option<DateTime<Utc>>,
    pub effective_to: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct RolloutStageRow {
    pub id: Uuid,
    pub stage_number: i16,
    pub target_percentage: i16,
    pub depot_ids: Vec<Uuid>,
    pub status: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
}

// ============================================================
// Conversion helpers
// ============================================================

impl From<RouteRow> for RouteResponse {
    fn from(r: RouteRow) -> Self {
        RouteResponse {
            id: r.id,
            code: r.code,
            name: r.name,
            description: r.description,
            status: r.status,
            effective_from: r.effective_from,
            version: r.entity_version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

impl From<StopRow> for StopResponse {
    fn from(s: StopRow) -> Self {
        StopResponse {
            id: s.id,
            route_id: s.route_id,
            code: s.code,
            name: s.name,
            sequence_order: s.sequence_order,
            latitude: s.latitude,
            longitude: s.longitude,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

impl From<TripRow> for TripResponse {
    fn from(t: TripRow) -> Self {
        TripResponse {
            id: t.id,
            route_id: t.route_id,
            trip_code: t.trip_code,
            status: t.status,
            scheduled_departure: t.scheduled_departure,
            scheduled_arrival: t.scheduled_arrival,
            actual_departure: t.actual_departure,
            actual_arrival: t.actual_arrival,
            assigned_driver_id: t.assigned_driver_id,
            calendar_id: t.calendar_id,
            effective_from: t.effective_from,
            version: t.entity_version,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

impl From<CalendarRow> for CalendarResponse {
    fn from(c: CalendarRow) -> Self {
        CalendarResponse {
            id: c.id,
            name: c.name,
            description: c.description,
            days_of_week: c.days_of_week,
            valid_from: c.valid_from,
            valid_to: c.valid_to,
            exception_dates: c.exception_dates,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

impl From<ConfigVersionRow> for ConfigVersionResponse {
    fn from(c: ConfigVersionRow) -> Self {
        ConfigVersionResponse {
            id: c.id,
            template_id: c.template_id,
            template_key: c.template_key,
            version_number: c.version_number,
            status: c.status,
            payload: c.payload,
            effective_from: c.effective_from,
            effective_to: c.effective_to,
            published_at: c.published_at,
            scheduled_at: c.scheduled_at,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}
