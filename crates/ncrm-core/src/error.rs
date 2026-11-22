use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// The row asked for does not exist, or belongs to another studio.
    /// The two are deliberately indistinguishable.
    #[error("not found")]
    NotFound,

    /// A uniqueness rule was broken, for example a duplicate phone number.
    #[error("conflict: {0}")]
    Conflict(String),

    /// The input failed validation before or at the database.
    #[error("invalid input: {0}")]
    Invalid(String),

    #[error(transparent)]
    Database(sqlx::Error),
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        if matches!(err, sqlx::Error::RowNotFound) {
            return Error::NotFound;
        }
        if let Some(db) = err.as_database_error() {
            if db.is_unique_violation() {
                let what = db.constraint().unwrap_or("unique constraint").to_string();
                return Error::Conflict(what);
            }
            if db.is_check_violation() || db.is_foreign_key_violation() {
                let what = db.constraint().unwrap_or("constraint").to_string();
                return Error::Invalid(what);
            }
        }
        Error::Database(err)
    }
}
