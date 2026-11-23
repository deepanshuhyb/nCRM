//! `/plans`: prices. Front desk can read them (they sell packs); owners
//! and managers set them.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::db::TenantTx;
use ncrm_core::plans::{self, NewPlan, Plan, PlanEdit};

use crate::error::ApiError;
use crate::extract::{parse_id, Body, Staff};
use crate::AppState;

/// `GET /plans`
pub async fn list(State(state): State<AppState>, who: Staff) -> Result<Json<Vec<Plan>>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let all = plans::list(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(all))
}

/// `POST /plans`
pub async fn create(
    State(state): State<AppState>,
    who: Staff,
    Body(body): Body<NewPlan>,
) -> Result<(StatusCode, Json<Plan>), ApiError> {
    who.require(Capability::ManageClasses)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let plan = plans::create(&mut tx, who.actor(), body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(plan)))
}

/// `PUT /plans/{id}`
pub async fn update(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<PlanEdit>,
) -> Result<Json<Plan>, ApiError> {
    who.require(Capability::ManageClasses)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let plan = plans::update(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(plan))
}
