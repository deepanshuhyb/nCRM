//! `/rooms` and `/batches`: the spaces and the weekly timetable.
//!
//! Everyone can see the timetable. Owners and managers shape it. A class's
//! roster is shown to staff who manage students and to that class's own
//! instructor (decided in `ncrm_core::batches::get`).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::batches::{self, BatchDetail, BatchEdit, BatchSummary, ListQuery, NewBatch};
use ncrm_core::db::TenantTx;
use ncrm_core::rooms::{self, NewRoom, Room, RoomEdit};

use crate::error::ApiError;
use crate::extract::{parse_id, Body, Params, Staff};
use crate::AppState;

/// `GET /rooms`
pub async fn list_rooms(
    State(state): State<AppState>,
    who: Staff,
) -> Result<Json<Vec<Room>>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let all = rooms::list(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(all))
}

/// `POST /branches/{id}/rooms`
pub async fn create_room(
    State(state): State<AppState>,
    who: Staff,
    Path(branch_id): Path<String>,
    Body(body): Body<NewRoom>,
) -> Result<(StatusCode, Json<Room>), ApiError> {
    who.require(Capability::ManageBranches)?;
    let branch_id = parse_id(&branch_id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let room = rooms::create(&mut tx, who.actor(), branch_id, body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(room)))
}

/// `PUT /rooms/{id}`
pub async fn update_room(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<RoomEdit>,
) -> Result<Json<Room>, ApiError> {
    who.require(Capability::ManageBranches)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let room = rooms::update(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(room))
}

/// `GET /batches?branch_id=&archived=`
pub async fn list(
    State(state): State<AppState>,
    who: Staff,
    Params(query): Params<ListQuery>,
) -> Result<Json<Vec<BatchSummary>>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let all = batches::list(&mut tx, query).await?;
    tx.commit().await?;
    Ok(Json(all))
}

/// `POST /batches`
pub async fn create(
    State(state): State<AppState>,
    who: Staff,
    Body(body): Body<NewBatch>,
) -> Result<(StatusCode, Json<BatchDetail>), ApiError> {
    who.require(Capability::ManageClasses)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let created = batches::create(&mut tx, who.actor(), body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// `GET /batches/{id}`
pub async fn get(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<BatchDetail>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = batches::get(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `PUT /batches/{id}`
pub async fn update(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<BatchEdit>,
) -> Result<Json<BatchDetail>, ApiError> {
    who.require(Capability::ManageClasses)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = batches::update(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(detail))
}
