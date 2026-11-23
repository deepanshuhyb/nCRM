//! `/students/import/preview` and `/students/import`: bring a student list
//! in from a spreadsheet. The file is the raw request body (not JSON, not
//! a multipart form), so these two routes get a 2 MB body limit instead of
//! the usual 64 KB.
//!
//! Preview writes nothing. Import reads the same file again and adds the
//! ready rows in one transaction (see `ncrm_core::import`).

use axum::body::Bytes;
use axum::extract::{FromRequest, Request, State};
use axum::http::{header, StatusCode};
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::db::TenantTx;
use ncrm_core::import::{self, ImportOptions, ImportResult, Preview, Sheet};

use crate::error::ApiError;
use crate::extract::{Params, Staff};
use crate::AppState;

/// The body limit for these routes: the file plus a little slack.
pub const IMPORT_BODY_LIMIT: usize = import::MAX_FILE_BYTES + 4 * 1024;

/// Types a browser sends for .xlsx and .csv files. Deliberately excludes
/// the three types a plain HTML form can send cross-site
/// (`application/x-www-form-urlencoded`, `multipart/form-data`,
/// `text/plain`): one more wall against cross-site request forgery, on top
/// of the origin check and the SameSite cookie.
const ACCEPTED_TYPES: [&str; 4] = [
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "text/csv",
    // What Windows browsers often call a .csv file.
    "application/vnd.ms-excel",
    "application/octet-stream",
];

/// A spreadsheet as the request body.
pub struct Upload(pub Bytes);

impl<S> FromRequest<S> for Upload
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(|v| v.trim().to_ascii_lowercase());
        if !content_type.is_some_and(|t| ACCEPTED_TYPES.contains(&t.as_str())) {
            return Err(ApiError::Invalid("Upload a .xlsx or .csv file.".into()));
        }
        match Bytes::from_request(req, state).await {
            Ok(bytes) => Ok(Upload(bytes)),
            Err(r) if r.status() == StatusCode::PAYLOAD_TOO_LARGE => Err(ApiError::TooLarge),
            Err(_) => Err(ApiError::Invalid(
                "The file didn't arrive in one piece. Try again.".into(),
            )),
        }
    }
}

/// Reading a spreadsheet is CPU work; keep it off the async threads.
async fn read(bytes: Bytes) -> Result<Sheet, ApiError> {
    tokio::task::spawn_blocking(move || import::read_sheet(&bytes))
        .await
        .map_err(|e| ApiError::Internal(format!("spreadsheet reader stopped: {e}")))?
        .map_err(ApiError::from)
}

/// `POST /students/import/preview?branch_id=`
pub async fn preview(
    State(state): State<AppState>,
    who: Staff,
    Params(options): Params<ImportOptions>,
    Upload(bytes): Upload,
) -> Result<Json<Preview>, ApiError> {
    who.require(Capability::ManageStudents)?;
    let sheet = read(bytes).await?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let preview = import::preview(&mut tx, who.actor(), &sheet, &options).await?;
    tx.rollback().await?;
    Ok(Json(preview))
}

/// `POST /students/import?branch_id=`
pub async fn commit(
    State(state): State<AppState>,
    who: Staff,
    Params(options): Params<ImportOptions>,
    Upload(bytes): Upload,
) -> Result<(StatusCode, Json<ImportResult>), ApiError> {
    who.require(Capability::ManageStudents)?;
    let sheet = read(bytes).await?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let result = import::commit(&mut tx, who.actor(), &sheet, &options).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}
