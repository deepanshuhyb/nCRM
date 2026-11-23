//! Fees: `/fees/...`, `/payments`, `/charges`, and a student's
//! `/students/{id}/fees`, `/charges`, `/payments`.
//!
//! The front desk adds charges and takes payments. Cancelling either is
//! for owners and managers ("adjust fees"), with a reason.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::db::TenantTx;
use ncrm_core::fees::{
    self, Discount, Dues, DuesQuery, FeeSettings, FeesSummary, NewCharge, NewPayment, Payments,
    PaymentsQuery, Receipt, StudentFees, Void,
};

use crate::error::ApiError;
use crate::extract::{parse_id, Body, Params, Staff};
use crate::AppState;

/// `GET /fees/summary`
pub async fn summary(
    State(state): State<AppState>,
    who: Staff,
) -> Result<Json<FeesSummary>, ApiError> {
    who.require(Capability::ManageFees)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let summary = fees::summary(&mut tx, who.actor()).await?;
    // Committed: the first look in a month creates that month's fees.
    tx.commit().await?;
    Ok(Json(summary))
}

/// `PUT /fees/settings`
pub async fn set_settings(
    State(state): State<AppState>,
    who: Staff,
    Body(body): Body<FeeSettings>,
) -> Result<Json<FeesSummary>, ApiError> {
    who.require(Capability::ManageClasses)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let summary = fees::set_settings(&mut tx, who.actor(), body).await?;
    tx.commit().await?;
    Ok(Json(summary))
}

/// `GET /fees/dues?branch_id=&limit=&offset=`
pub async fn dues(
    State(state): State<AppState>,
    who: Staff,
    Params(query): Params<DuesQuery>,
) -> Result<Json<Dues>, ApiError> {
    who.require(Capability::ManageFees)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let dues = fees::dues(&mut tx, who.actor(), query).await?;
    tx.commit().await?;
    Ok(Json(dues))
}

/// `GET /payments?from=&to=&limit=&offset=`
pub async fn payments(
    State(state): State<AppState>,
    who: Staff,
    Params(query): Params<PaymentsQuery>,
) -> Result<Json<Payments>, ApiError> {
    who.require(Capability::ManageFees)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let list = fees::payments(&mut tx, who.actor(), query).await?;
    tx.commit().await?;
    Ok(Json(list))
}

/// `GET /payments/{id}`: the receipt.
pub async fn receipt(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<Receipt>, ApiError> {
    who.require(Capability::ManageFees)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let receipt = fees::receipt(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(receipt))
}

/// `POST /payments/{id}/void`
pub async fn void_payment(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<Void>,
) -> Result<Json<StudentFees>, ApiError> {
    who.require(Capability::AdjustFees)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let fees = fees::void_payment(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(fees))
}

/// `POST /charges/{id}/void`
pub async fn void_charge(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<Void>,
) -> Result<Json<StudentFees>, ApiError> {
    who.require(Capability::AdjustFees)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let fees = fees::void_charge(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(fees))
}

/// `GET /students/{id}/fees`
pub async fn for_student(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<StudentFees>, ApiError> {
    who.require(Capability::ManageFees)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let fees = fees::for_student(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(fees))
}

/// `POST /students/{id}/charges`
pub async fn add_charge(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<NewCharge>,
) -> Result<(StatusCode, Json<StudentFees>), ApiError> {
    who.require(Capability::ManageFees)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let fees = fees::add_charge(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(fees)))
}

/// `POST /charges/{id}/discount`: `{off_paise, reason}`.
pub async fn discount_charge(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<Discount>,
) -> Result<Json<StudentFees>, ApiError> {
    who.require(Capability::AdjustFees)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let fees = fees::discount_charge(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok(Json(fees))
}

/// `POST /students/{id}/payments`: returns the receipt.
pub async fn record_payment(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<NewPayment>,
) -> Result<(StatusCode, Json<Receipt>), ApiError> {
    who.require(Capability::ManageFees)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let receipt = fees::record_payment(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(receipt)))
}
