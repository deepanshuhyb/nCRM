use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// The row asked for does not exist, or belongs to another studio.
    /// The two are deliberately indistinguishable.
    #[error("not found")]
    NotFound,

    /// A uniqueness rule was broken, for example a duplicate phone number,
    /// or the change collides with something (a room already booked, a full
    /// class). The text is a sentence for the person, never a constraint name.
    #[error("conflict: {0}")]
    Conflict(String),

    /// The input failed validation before or at the database. The text is a
    /// sentence for the person.
    #[error("invalid input: {0}")]
    Invalid(String),

    /// The acting staff member's role does not allow this.
    #[error("not allowed")]
    Forbidden,

    /// The sign-in code is wrong, expired or already used. Deliberately one
    /// error for all three, so guesses learn nothing.
    #[error("that code is wrong or has expired")]
    InvalidCode,

    /// Too many wrong guesses on the current code; a new one is needed.
    #[error("too many wrong attempts; ask for a new code")]
    CodeLocked,

    /// Too many requests in a short time.
    #[error("too many requests; try again later")]
    RateLimited,

    #[error(transparent)]
    Database(sqlx::Error),
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        if matches!(err, sqlx::Error::RowNotFound) {
            return Error::NotFound;
        }
        if let Some(db) = err.as_database_error() {
            let constraint = db.constraint().unwrap_or("");
            if db.is_unique_violation() {
                return Error::Conflict(unique_message(constraint).into());
            }
            if db.is_check_violation() {
                return Error::Invalid(check_message(constraint).into());
            }
            if db.is_foreign_key_violation() {
                return Error::Invalid(
                    "Something that refers to isn't in your studio any more.".into(),
                );
            }
        }
        Error::Database(err)
    }
}

/// What a broken uniqueness rule means, in words. The application checks
/// most of these first with a more specific message; these are the backstop
/// when two people do the same thing at the same moment.
fn unique_message(constraint: &str) -> &'static str {
    match constraint {
        "branches_org_id_name_key" => "There's already a branch with that name.",
        "staff_org_id_phone_key" => "Someone on the team already has that number.",
        "student_contacts_pkey" => "That number is already a contact for this student.",
        "rooms_org_id_branch_id_name_key" => "That branch already has a room with that name.",
        "plans_org_id_name_key" => "There's already a plan with that name.",
        "enrollments_one_open" => "They're already in that class.",
        "organizations_slug_key" => "That studio address is taken.",
        _ => "That already exists.",
    }
}

/// What a failed CHECK means, in words. Most input is validated with a
/// specific message before it reaches the database.
fn check_message(constraint: &str) -> &'static str {
    match constraint {
        "credit_moves_balance" => "That's more class credits than they have.",
        "plans_shape" => "That plan's classes and validity don't fit its kind.",
        _ => "Some of that isn't allowed. Check the values and try again.",
    }
}
