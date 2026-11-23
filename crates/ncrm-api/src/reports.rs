//! `/reports/...` and `/batches/{id}/history`: the aggregates the
//! dashboard draws as graphs. Counts and sums, never names.

use axum::extract::{Path, State};
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::db::TenantTx;
use ncrm_core::reports::{
    self, AttendanceQuery, DayAttendance, FeesReport, MeetingCount, MonthStudents,
};

use crate::error::ApiError;
use crate::extract::{parse_id, Params, Staff};
use crate::AppState;

/// `GET /reports/attendance?days=`
pub async fn attendance(
    State(state): State<AppState>,
    who: Staff,
    Params(query): Params<AttendanceQuery>,
) -> Result<Json<Vec<DayAttendance>>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let days = reports::attendance_by_day(&mut tx, who.actor(), query).await?;
    tx.commit().await?;
    Ok(Json(days))
}

/// `GET /reports/students`
pub async fn students(
    State(state): State<AppState>,
    who: Staff,
) -> Result<Json<Vec<MonthStudents>>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let months = reports::students_by_month(&mut tx, who.actor()).await?;
    tx.commit().await?;
    Ok(Json(months))
}

/// `GET /reports/fees`
pub async fn fees(State(state): State<AppState>, who: Staff) -> Result<Json<FeesReport>, ApiError> {
    who.require(Capability::ManageFees)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let report = reports::fees(&mut tx, who.actor()).await?;
    // Committed: like the fees pages, the first look in a month creates
    // that month's fees.
    tx.commit().await?;
    Ok(Json(report))
}

/// `GET /batches/{id}/history`
pub async fn class_history(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<Vec<MeetingCount>>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let meetings = reports::class_history(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(meetings))
}
