//! Database connection, migrations and studio-scoped transactions.

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::Result;

/// All SQL migrations, embedded in the binary at compile time.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// Open a connection pool.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Apply any migrations that have not run yet.
pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .map_err(|e| crate::Error::Invalid(format!("migration failed: {e}")))?;
    Ok(())
}

/// A transaction locked to one studio.
///
/// Opening it switches the connection to the restricted `ncrm_app` role and
/// sets `app.org_id`, both for this transaction only. Row-level security then
/// hides every other studio's rows, whatever SQL runs inside.
///
/// Functions that read or write studio data take `&mut TenantTx`, so they
/// cannot be called with a bare pool by mistake.
pub struct TenantTx {
    tx: Transaction<'static, Postgres>,
    org_id: Uuid,
}

impl TenantTx {
    /// Begin a transaction scoped to `org_id`.
    pub async fn begin(pool: &PgPool, org_id: Uuid) -> Result<Self> {
        let mut tx = pool.begin().await?;
        // SET LOCAL and set_config(..., true) both end with the transaction,
        // so nothing leaks to the next user of this pooled connection.
        sqlx::query("SET LOCAL ROLE ncrm_app")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT set_config('app.org_id', $1, true)")
            .bind(org_id.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(Self { tx, org_id })
    }

    /// The studio this transaction is locked to.
    pub fn org_id(&self) -> Uuid {
        self.org_id
    }

    /// Today's date where the studio is (its `timezone`, India by default).
    /// Use this, not the server's clock or `CURRENT_DATE`: the server runs on
    /// UTC, where it's still "yesterday" until 5:30 am in India.
    pub async fn studio_today(&mut self) -> Result<time::Date> {
        let today: time::Date = sqlx::query_scalar(
            "SELECT (now() AT TIME ZONE timezone)::date FROM organizations WHERE id = $1",
        )
        .bind(self.org_id)
        .fetch_one(&mut *self.tx)
        .await?;
        Ok(today)
    }

    /// Today's date and the time of day where the studio is.
    pub async fn studio_now(&mut self) -> Result<(time::Date, time::Time)> {
        let now: (time::Date, time::Time) = sqlx::query_as(
            "SELECT (now() AT TIME ZONE timezone)::date, (now() AT TIME ZONE timezone)::time
               FROM organizations WHERE id = $1",
        )
        .bind(self.org_id)
        .fetch_one(&mut *self.tx)
        .await?;
        Ok(now)
    }

    /// Wait for, then hold until this transaction ends, a lock named
    /// `name` within this studio. Serializes rare, check-then-write work
    /// (schedule clash checks) that row locks can't cover.
    pub async fn lock_studio(&mut self, name: &str) -> Result<()> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("ncrm:{name}:{}", self.org_id))
            .execute(&mut *self.tx)
            .await?;
        Ok(())
    }

    /// The underlying connection, for running queries.
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    /// Save everything done in this transaction.
    pub async fn commit(self) -> Result<()> {
        self.tx.commit().await?;
        Ok(())
    }

    /// Discard everything done in this transaction. Dropping a `TenantTx`
    /// without committing does the same.
    pub async fn rollback(self) -> Result<()> {
        self.tx.rollback().await?;
        Ok(())
    }
}
