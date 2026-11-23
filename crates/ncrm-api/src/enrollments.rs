//! A student's classes and class credits:
//! `/students/{id}/enrollments`, `/enrollments/{id}/end` and
//! `/students/{id}/credits/...`.
//!
//! Front desk puts students in classes and sells packs. Giving classes away
//! or taking credits back is for owners and managers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::credits::{self, Credits, GiveCredits, SellPack, TakeBack};
use ncrm_core::db::TenantTx;
use ncrm_core::enrollments::{self, NewEnrollment, StudentEnrollment};

use crate::error::ApiError;
use crate::extract::{parse_id, Body, Staff};
use crate::AppState;

/// `GET /students/{id}/enrollments`
pub async fn list(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<Vec<StudentEnrollment>>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    // A student from another studio is "not found", not an empty list.
    ncrm_core::students::get(&mut tx, id).await?;
    let all = enrollments::for_student(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(all))
}

/// `POST /students/{id}/enrollments`
pub async fn enroll(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<NewEnrollment>,
) -> Result<(StatusCode, Json<Vec<StudentEnrollment>>), ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let all = enrollments::enroll(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(all)))
}

/// `POST /enrollments/{id}/end`
pub async fn end(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<Vec<StudentEnrollment>>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let all = enrollments::end(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(all))
}

/// `GET /students/{id}/credits`
pub async fn credits(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<Credits>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let c = credits::summary(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(c))
}

/// `POST /students/{id}/credits/sell`
pub async fn sell(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<SellPack>,
) -> Result<(StatusCode, Json<Credits>), ApiError> {
    who.require(Capability::ManageFees)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let c = credits::sell(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(c)))
}

/// `POST /students/{id}/credits/give`
pub async fn give(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<GiveCredits>,
) -> Result<(StatusCode, Json<Credits>), ApiError> {
    who.require(Capability::ManageClasses)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let c = credits::give(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(c)))
}

/// `POST /students/{id}/credits/take-back`
pub async fn take_back(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<TakeBack>,
) -> Result<Json<Credits>, ApiError> {
    who.require(Capability::ManageClasses)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let c = credits::take_back(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(c))
}
