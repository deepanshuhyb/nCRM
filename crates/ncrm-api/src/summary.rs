//! `/summary` — the counts on the home screen. Aggregates only, so every
//! role may see them.

use axum::extract::State;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::db::TenantTx;
use ncrm_core::students;
use serde::Serialize;

use crate::error::ApiError;
use crate::extract::Staff;
use crate::AppState;

#[derive(Serialize)]
pub struct Summary {
    pub branches: i64,
    pub open_branches: i64,
    pub active_students: i64,
    pub active_team: i64,
}

/// `GET /summary`
pub async fn get(State(state): State<AppState>, who: Staff) -> Result<Json<Summary>, ApiError> {
    who.require(Capability::ViewStudio)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let (branches, open_branches): (i64, i64) =
        sqlx::query_as("SELECT count(*), count(*) FILTER (WHERE active) FROM branches")
            .fetch_one(tx.conn())
            .await
            .map_err(ncrm_core::Error::from)?;
    let active_team: i64 = sqlx::query_scalar("SELECT count(*) FROM staff WHERE active")
        .fetch_one(tx.conn())
        .await
        .map_err(ncrm_core::Error::from)?;
    let active_students = students::count_active(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(Summary {
        branches,
        open_branches,
        active_students,
        active_team,
    }))
}
