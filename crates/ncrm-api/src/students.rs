//! `/students` — students and their contacts. Owners, managers and front
//! desk. (Instructors will see the students of their own classes once
//! classes exist.)

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::db::TenantTx;
use ncrm_core::students::{
    self, ListQuery, NewContact, NewStudent, Page, StudentDetail, StudentEdit, StudentListItem,
};

use crate::error::ApiError;
use crate::extract::{parse_id, Body, Params, Staff};
use crate::AppState;

/// `GET /students?q=&status=&branch_id=&limit=&offset=`
pub async fn list(
    State(state): State<AppState>,
    who: Staff,
    Params(query): Params<ListQuery>,
) -> Result<Json<Page<StudentListItem>>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let page = students::list(&mut tx, query).await?;
    tx.commit().await?;
    Ok(Json(page))
}

/// `POST /students`
pub async fn create(
    State(state): State<AppState>,
    who: Staff,
    Body(body): Body<NewStudent>,
) -> Result<(StatusCode, Json<StudentDetail>), ApiError> {
    who.require(Capability::ManageStudents)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let created = students::create(&mut tx, who.actor(), body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// `GET /students/{id}`
pub async fn get(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<StudentDetail>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = students::get(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `PUT /students/{id}`
pub async fn update(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<StudentEdit>,
) -> Result<Json<StudentDetail>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = students::update(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// `POST /students/{id}/contacts`
pub async fn add_contact(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<NewContact>,
) -> Result<(StatusCode, Json<StudentDetail>), ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = students::add_contact(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

/// `DELETE /students/{id}/contacts/{contact_id}`
pub async fn remove_contact(
    State(state): State<AppState>,
    who: Staff,
    Path((id, contact_id)): Path<(String, String)>,
) -> Result<Json<StudentDetail>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let id = parse_id(&id)?;
    let contact_id = parse_id(&contact_id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let detail = students::remove_contact(&mut tx, who.actor(), id, contact_id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}
