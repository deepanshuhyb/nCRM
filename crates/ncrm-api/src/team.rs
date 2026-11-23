//! `/staff` — the studio's team. Owners and managers only; the rules on who
//! may change whom live in `ncrm_core::staff`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::db::TenantTx;
use ncrm_core::staff::{self, NewStaff, Staff, StaffEdit, StaffRole};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::{parse_id, Body, Staff as Acting};
use crate::AppState;

#[derive(Deserialize)]
pub struct Hire {
    pub name: String,
    pub phone: String,
    pub role: StaffRole,
    pub branch_id: Option<Uuid>,
}

/// `GET /staff`
pub async fn list(
    State(state): State<AppState>,
    who: Acting,
) -> Result<Json<Vec<Staff>>, ApiError> {
    who.require(Capability::ManageStaff)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let team = staff::list(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(team))
}

/// `POST /staff`
pub async fn hire(
    State(state): State<AppState>,
    who: Acting,
    Body(body): Body<Hire>,
) -> Result<(StatusCode, Json<Staff>), ApiError> {
    who.require(Capability::ManageStaff)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let hired = staff::hire(
        &mut tx,
        who.actor(),
        NewStaff {
            name: body.name,
            phone: body.phone,
            role: body.role,
            branch_id: body.branch_id,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(hired)))
}

/// `PUT /staff/{id}`
pub async fn update(
    State(state): State<AppState>,
    who: Acting,
    Path(id): Path<String>,
    Body(body): Body<StaffEdit>,
) -> Result<Json<Staff>, ApiError> {
    who.require(Capability::ManageStaff)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let updated = staff::update(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(updated))
}
