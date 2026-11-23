//! `/sessions` and `/attendance`: today's classes, marking who came,
//! teacher check-in, drop-ins, cancelling, and alerts.
//!
//! Everyone can see the day's sessions. Marking needs "mark own
//! attendance" here (every role has it), and the core then checks the finer
//! rule: the front desk and up mark anything, an instructor only a session
//! they teach.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::attendance::{
    self, Alerts, Cancel, DayQuery, DropIn, Marks, SessionDetail, SessionSummary,
    StudentAttendance, TeacherMark,
};
use ncrm_core::db::TenantTx;

use crate::error::ApiError;
use crate::extract::{parse_id, Body, Params, Staff};
use crate::AppState;

/// `GET /sessions?date=&branch_id=`
pub async fn day(
    State(state): State<AppState>,
    who: Staff,
    Params(query): Params<DayQuery>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let sessions = attendance::day(&mut tx, who.actor(), query).await?;
    // Committed: looking at a day creates its sessions from the timetable.
    tx.commit().await?;
    Ok(Json(sessions))
}

/// `GET /sessions/{id}`
pub async fn get(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<SessionDetail>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = attendance::get(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `PUT /sessions/{id}/attendance`
pub async fn mark(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<Marks>,
) -> Result<Json<SessionDetail>, ApiError> {
    who.require(Capability::MarkOwnAttendance)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = attendance::mark(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `POST /sessions/{id}/drop-ins`
pub async fn add_drop_in(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<DropIn>,
) -> Result<(StatusCode, Json<SessionDetail>), ApiError> {
    who.require(Capability::MarkOwnAttendance)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = attendance::add_drop_in(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

/// `DELETE /sessions/{id}/drop-ins/{student_id}`
pub async fn remove_drop_in(
    State(state): State<AppState>,
    who: Staff,
    Path((id, student_id)): Path<(String, String)>,
) -> Result<Json<SessionDetail>, ApiError> {
    who.require(Capability::MarkOwnAttendance)?;
    let id = parse_id(&id)?;
    let student_id = parse_id(&student_id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = attendance::remove_drop_in(&mut tx, who.actor(), id, student_id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `PUT /sessions/{id}/teacher`
pub async fn set_teacher(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<TeacherMark>,
) -> Result<Json<SessionDetail>, ApiError> {
    who.require(Capability::MarkOwnAttendance)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = attendance::set_teacher(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `POST /sessions/{id}/cancel`
pub async fn cancel(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<Cancel>,
) -> Result<Json<SessionDetail>, ApiError> {
    who.require(Capability::MarkAnyAttendance)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = attendance::cancel(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `POST /sessions/{id}/restore`
pub async fn restore(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<SessionDetail>, ApiError> {
    who.require(Capability::MarkAnyAttendance)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = attendance::restore(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `GET /attendance/alerts`
pub async fn alerts(State(state): State<AppState>, who: Staff) -> Result<Json<Alerts>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let alerts = attendance::alerts(&mut tx, who.actor()).await?;
    tx.commit().await?;
    Ok(Json(alerts))
}

/// `GET /students/{id}/attendance`
pub async fn for_student(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<StudentAttendance>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let history = attendance::for_student(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(history))
}
