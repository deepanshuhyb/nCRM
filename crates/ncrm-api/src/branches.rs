//! `/branches` — the first studio-scoped resource. The pattern here is the
//! one every studio-scoped handler follows:
//!
//! 1. Take `Staff` (signed in, studio picked, still active).
//! 2. `require` the capability.
//! 3. Open `TenantTx` with the staff member's org id.
//! 4. Do the work and write the audit entry in the same transaction.
//! 5. Commit.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::audit::{self, NewAuditEntry};
use ncrm_core::branches::{self, Branch, NewBranch};
use ncrm_core::db::TenantTx;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::extract::{Body, Staff};
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateBranch {
    pub name: String,
    pub address: Option<String>,
}

/// `GET /branches`
pub async fn list(
    State(state): State<AppState>,
    staff: Staff,
) -> Result<Json<Vec<Branch>>, ApiError> {
    staff.require(Capability::ViewStudio)?;
    let mut tx = TenantTx::begin(&state.pool, staff.0.org_id).await?;
    let all = branches::list(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(all))
}

/// `POST /branches`
pub async fn create(
    State(state): State<AppState>,
    staff: Staff,
    Body(body): Body<CreateBranch>,
) -> Result<(StatusCode, Json<Branch>), ApiError> {
    staff.require(Capability::ManageBranches)?;
    if body.name.trim().is_empty() {
        return Err(ApiError::Invalid("Give the branch a name.".into()));
    }

    let mut tx = TenantTx::begin(&state.pool, staff.0.org_id).await?;
    let branch = branches::create(
        &mut tx,
        NewBranch {
            name: body.name,
            address: body.address,
            timezone: None,
        },
    )
    .await?;
    audit::record(
        &mut tx,
        NewAuditEntry {
            actor_staff_id: Some(staff.0.staff_id),
            action: "branch.created".into(),
            entity: "branch".into(),
            entity_id: Some(branch.id),
            detail: json!({ "name": branch.name, "address": branch.address }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(branch)))
}
