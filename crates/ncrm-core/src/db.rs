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
