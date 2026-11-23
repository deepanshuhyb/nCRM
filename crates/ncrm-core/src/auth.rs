//! Signing in with a one-time code.
//!
//! The flow, end to end (see docs/ARCHITECTURE.md, "Signing in"):
//!
//! 1. `request_code`: a phone number asks for a code. A 6-digit code is
//!    generated, its HMAC is stored, and the code is handed back to the caller
//!    for delivery only if the phone belongs to active staff somewhere.
//! 2. `verify_code`: the person types the code. On success a session is
//!    created and a random token is returned for the browser's cookie.
//! 3. If the phone works at several studios, `select_studio` picks one.
//! 4. Every later request turns the cookie back into a session with
//!    `session_from_token`, then into a studio context with `staff_context`.
//!
//! Everything here runs as the database owner, outside any studio's scope,
//! because it happens before (or while) we learn which studio the person is
//! in. These are the only functions allowed to look across studios, and they
//! only ever look up the caller's own phone number.

use std::sync::{Arc, Mutex};

use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::staff::StaffRole;
use crate::{phone, Error, Result};

type HmacSha256 = Hmac<Sha256>;

/// Settings for sign-in. Built once at startup.
#[derive(Clone)]
pub struct AuthConfig {
    secret: Vec<u8>,
    /// How long a code stays valid.
    pub code_ttl_secs: i64,
    /// How long a session lasts from sign-in.
    pub session_ttl_secs: i64,
    /// Codes one phone may request per rolling hour.
    pub max_codes_per_hour: i64,
    /// Wrong guesses allowed on one code before it is locked.
    pub max_attempts: i32,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field("secret", &"<hidden>")
            .field("code_ttl_secs", &self.code_ttl_secs)
            .field("session_ttl_secs", &self.session_ttl_secs)
            .field("max_codes_per_hour", &self.max_codes_per_hour)
            .field("max_attempts", &self.max_attempts)
            .finish()
    }
}

impl AuthConfig {
    /// Minimum length of the HMAC secret, in bytes.
    pub const MIN_SECRET_LEN: usize = 32;

    /// Default settings with the given secret. The secret must be at least
    /// 32 bytes; generate one with `openssl rand -base64 48`.
    pub fn new(secret: impl Into<Vec<u8>>) -> Result<Self> {
        let secret = secret.into();
        if secret.len() < Self::MIN_SECRET_LEN {
            return Err(Error::Invalid(format!(
                "AUTH_SECRET must be at least {} bytes",
                Self::MIN_SECRET_LEN
            )));
        }
        Ok(Self {
            secret,
            code_ttl_secs: 10 * 60,
            session_ttl_secs: 30 * 24 * 60 * 60,
            max_codes_per_hour: 5,
            max_attempts: 5,
        })
    }

    fn code_mac(&self, phone: &str, code: &str) -> HmacSha256 {
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC accepts any key length");
        // Binding the phone into the MAC means a stored hash is useless for
        // any other number.
        mac.update(phone.as_bytes());
        mac.update(b":");
        mac.update(code.as_bytes());
        mac
    }
}

/// A code waiting to be delivered to a phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCode {
    pub phone: String,
    pub code: String,
}

/// How codes reach people.
///
/// An enum rather than a trait object keeps async simple.
#[derive(Clone, Debug)]
pub enum CodeSender {
    /// Write the code to the log. Development only.
    Log,
    /// Send it over WhatsApp. The code goes to Meta and nowhere else: it
    /// is not logged, and a failure reports only a status and error number.
    WhatsApp(crate::whatsapp::WhatsApp),
    /// Keep codes in memory so tests can read them.
    Capture(Arc<Mutex<Vec<PendingCode>>>),
}

impl CodeSender {
    pub fn capture() -> (Self, Arc<Mutex<Vec<PendingCode>>>) {
        let store = Arc::new(Mutex::new(Vec::new()));
        (CodeSender::Capture(store.clone()), store)
    }

    pub async fn send(&self, pending: PendingCode) -> Result<()> {
        match self {
            CodeSender::Log => {
                eprintln!(
                    "[ncrm dev] sign-in code for {}: {}",
                    pending.phone, pending.code
                );
                Ok(())
            }
            CodeSender::WhatsApp(client) => client
                .send_sign_in_code(&pending.phone, &pending.code)
                .await
                .map_err(|e| Error::Unavailable(e.to_string())),
            CodeSender::Capture(store) => {
                store.lock().expect("capture store poisoned").push(pending);
                Ok(())
            }
        }
    }
}

/// One studio a phone number works at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Membership {
    pub org_id: Uuid,
    pub org_name: String,
    pub staff_id: Uuid,
    pub staff_name: String,
    pub role: StaffRole,
}

/// A signed-in session, as stored.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub phone: String,
    pub org_id: Option<Uuid>,
    pub staff_id: Option<Uuid>,
    pub last_seen_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

/// The result of a successful `verify_code`.
#[derive(Debug, Clone)]
pub struct SignedIn {
    /// Goes in the browser's cookie. Never stored or logged.
    pub token: String,
    pub session: Session,
    pub memberships: Vec<Membership>,
}

/// Who is acting, inside which studio. Built for every studio-scoped request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct StaffContext {
    pub session_id: Uuid,
    pub org_id: Uuid,
    pub org_name: String,
    pub staff_id: Uuid,
    pub staff_name: String,
    pub phone: String,
    pub role: StaffRole,
}

/// Ask for a code.
///
/// Returns the code to deliver when the phone belongs to active staff, and
/// `None` when it does not. Callers should respond the same way in both
/// cases and deliver in the background, so the endpoint does not reveal who
/// is staff.
pub async fn request_code(
    pool: &PgPool,
    cfg: &AuthConfig,
    phone_as_typed: &str,
) -> Result<Option<PendingCode>> {
    let phone = phone::normalize(phone_as_typed)?;
    let mut tx = pool.begin().await?;

    // Serialize requests for the same phone so two parallel requests cannot
    // both slip under the hourly limit.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&phone)
        .execute(&mut *tx)
        .await?;

    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.login_codes
          WHERE phone = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(&phone)
    .fetch_one(&mut *tx)
    .await?;
    if recent >= cfg.max_codes_per_hour {
        return Err(Error::RateLimited);
    }

    let code = format!("{:06}", rand::random_range(0..1_000_000u32));
    let code_hash = cfg.code_mac(&phone, &code).finalize().into_bytes().to_vec();

    sqlx::query(
        "INSERT INTO platform.login_codes (phone, code_hash, expires_at)
         VALUES ($1, $2, now() + make_interval(secs => $3))",
    )
    .bind(&phone)
    .bind(code_hash)
    .bind(cfg.code_ttl_secs as f64)
    .execute(&mut *tx)
    .await?;

    let is_staff: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM staff s JOIN organizations o ON o.id = s.org_id
             WHERE s.phone = $1 AND s.active)",
    )
    .bind(&phone)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(is_staff.then_some(PendingCode { phone, code }))
}

/// Check a code and, if it is right, start a session.
pub async fn verify_code(
    pool: &PgPool,
    cfg: &AuthConfig,
    phone_as_typed: &str,
    code: &str,
) -> Result<SignedIn> {
    let phone = phone::normalize(phone_as_typed).map_err(|_| Error::InvalidCode)?;
    let code = code.trim();
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::InvalidCode);
    }

    let mut tx = pool.begin().await?;

    // Only the newest unused code counts. FOR UPDATE stops two parallel
    // guesses from both reading the same attempt count.
    let latest: Option<(Uuid, Vec<u8>, i32, bool)> = sqlx::query_as(
        "SELECT id, code_hash, attempts, expires_at > now()
           FROM platform.login_codes
          WHERE phone = $1 AND consumed_at IS NULL
          ORDER BY created_at DESC
          LIMIT 1
          FOR UPDATE",
    )
    .bind(&phone)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((code_id, stored, attempts, live)) = latest else {
        return Err(Error::InvalidCode);
    };
    if !live {
        return Err(Error::InvalidCode);
    }
    if attempts >= cfg.max_attempts {
        return Err(Error::CodeLocked);
    }

    // verify_slice compares in constant time.
    if cfg.code_mac(&phone, code).verify_slice(&stored).is_err() {
        sqlx::query("UPDATE platform.login_codes SET attempts = attempts + 1 WHERE id = $1")
            .bind(code_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(Error::InvalidCode);
    }

    // Right code. Use it up, along with any older unused ones.
    sqlx::query(
        "UPDATE platform.login_codes SET consumed_at = now()
          WHERE phone = $1 AND consumed_at IS NULL",
    )
    .bind(&phone)
    .execute(&mut *tx)
    .await?;

    let memberships = memberships_on(&mut tx, &phone).await?;
    if memberships.is_empty() {
        // A code was issued for a number that is not staff (or no longer is).
        tx.commit().await?;
        return Err(Error::InvalidCode);
    }
    let picked = (memberships.len() == 1).then(|| &memberships[0]);

    let token = new_token();
    let session = sqlx::query_as::<_, Session>(
        "INSERT INTO platform.sessions (token_hash, phone, org_id, staff_id, expires_at)
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5))
         RETURNING id, phone, org_id, staff_id, last_seen_at, expires_at",
    )
    .bind(token_hash(&token))
    .bind(&phone)
    .bind(picked.map(|m| m.org_id))
    .bind(picked.map(|m| m.staff_id))
    .bind(cfg.session_ttl_secs as f64)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(SignedIn {
        token,
        session,
        memberships,
    })
}

/// Studios this phone number works at, by studio name.
pub async fn memberships(pool: &PgPool, phone_e164: &str) -> Result<Vec<Membership>> {
    let mut conn = pool.acquire().await?;
    memberships_on(&mut conn, phone_e164).await
}

async fn memberships_on(
    conn: &mut sqlx::PgConnection,
    phone_e164: &str,
) -> Result<Vec<Membership>> {
    let rows = sqlx::query_as::<_, Membership>(
        "SELECT o.id AS org_id, o.name AS org_name, s.id AS staff_id,
                s.name AS staff_name, s.role
           FROM staff s JOIN organizations o ON o.id = s.org_id
          WHERE s.phone = $1 AND s.active
          ORDER BY o.name, o.id",
    )
    .bind(phone_e164)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// Turn a cookie token into a live session, or `None` if it is unknown,
/// expired or signed out.
pub async fn session_from_token(pool: &PgPool, token: &str) -> Result<Option<Session>> {
    let session = sqlx::query_as::<_, Session>(
        "SELECT id, phone, org_id, staff_id, last_seen_at, expires_at
           FROM platform.sessions
          WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(token_hash(token))
    .fetch_optional(pool)
    .await?;

    // Record activity at most every five minutes, so reads do not turn into
    // a write on every request.
    if let Some(s) = &session {
        if OffsetDateTime::now_utc() - s.last_seen_at > time::Duration::minutes(5) {
            sqlx::query("UPDATE platform.sessions SET last_seen_at = now() WHERE id = $1")
                .bind(s.id)
                .execute(pool)
                .await?;
        }
    }
    Ok(session)
}

/// The studio context for a session, or `None` if no studio is picked or the
/// person no longer works there (deactivated, or their number changed).
pub async fn staff_context(pool: &PgPool, session: &Session) -> Result<Option<StaffContext>> {
    let (Some(org_id), Some(staff_id)) = (session.org_id, session.staff_id) else {
        return Ok(None);
    };
    let ctx = sqlx::query_as::<_, StaffContext>(
        "SELECT $1::uuid AS session_id, o.id AS org_id, o.name AS org_name,
                s.id AS staff_id, s.name AS staff_name, s.phone, s.role
           FROM staff s JOIN organizations o ON o.id = s.org_id
          WHERE s.org_id = $2 AND s.id = $3 AND s.active AND s.phone = $4",
    )
    .bind(session.id)
    .bind(org_id)
    .bind(staff_id)
    .bind(&session.phone)
    .fetch_optional(pool)
    .await?;
    Ok(ctx)
}

/// Switch a session to one of the studios its phone works at.
pub async fn select_studio(pool: &PgPool, session: &Session, org_id: Uuid) -> Result<Session> {
    let staff_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM staff WHERE org_id = $1 AND phone = $2 AND active")
            .bind(org_id)
            .bind(&session.phone)
            .fetch_optional(pool)
            .await?;
    // Not working there looks the same as the studio not existing.
    let staff_id = staff_id.ok_or(Error::NotFound)?;

    let updated = sqlx::query_as::<_, Session>(
        "UPDATE platform.sessions SET org_id = $2, staff_id = $3
          WHERE id = $1 AND revoked_at IS NULL
          RETURNING id, phone, org_id, staff_id, last_seen_at, expires_at",
    )
    .bind(session.id)
    .bind(org_id)
    .bind(staff_id)
    .fetch_one(pool)
    .await?;
    Ok(updated)
}

/// End a session. Unknown tokens are ignored.
pub async fn sign_out(pool: &PgPool, token: &str) -> Result<()> {
    sqlx::query(
        "UPDATE platform.sessions SET revoked_at = now()
          WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(token_hash(token))
    .execute(pool)
    .await?;
    Ok(())
}

fn new_token() -> String {
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_secrets_are_refused() {
        assert!(AuthConfig::new(vec![0u8; 31]).is_err());
        assert!(AuthConfig::new(vec![0u8; 32]).is_ok());
    }

    #[test]
    fn the_secret_never_shows_in_debug_output() {
        let cfg = AuthConfig::new(b"super-secret-value-that-is-32-bytes!".to_vec()).unwrap();
        assert!(!format!("{cfg:?}").contains("super-secret"));
    }

    #[test]
    fn a_code_hash_only_matches_its_own_phone() {
        let cfg = AuthConfig::new(vec![7u8; 32]).unwrap();
        let tag = cfg
            .code_mac("+919876543210", "123456")
            .finalize()
            .into_bytes();
        assert!(cfg
            .code_mac("+919876543210", "123456")
            .verify_slice(&tag)
            .is_ok());
        assert!(cfg
            .code_mac("+919876543211", "123456")
            .verify_slice(&tag)
            .is_err());
        assert!(cfg
            .code_mac("+919876543210", "123457")
            .verify_slice(&tag)
            .is_err());
    }

    #[test]
    fn tokens_are_long_random_and_url_safe() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43); // 32 bytes, base64 without padding
        assert!(a
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
        assert_eq!(token_hash(&a).len(), 32);
    }
}

/// What `cleanup` removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cleanup {
    pub codes: u64,
    pub sessions: u64,
}

/// Delete sign-in records nobody needs any more: codes older than a day
/// (they stop working after ten minutes; a day keeps the "5 codes an hour"
/// limit honest), and sessions that expired or were signed out more than a
/// week ago.
///
/// Runs outside any studio, like the rest of this module: these tables
/// belong to no studio. Called by the worker (`jobs::housekeeping`).
pub async fn cleanup(pool: &PgPool) -> Result<Cleanup> {
    let codes =
        sqlx::query("DELETE FROM platform.login_codes WHERE created_at < now() - interval '1 day'")
            .execute(pool)
            .await?
            .rows_affected();
    let sessions = sqlx::query(
        "DELETE FROM platform.sessions
          WHERE expires_at < now() - interval '7 days'
             OR revoked_at < now() - interval '7 days'",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(Cleanup { codes, sessions })
}
