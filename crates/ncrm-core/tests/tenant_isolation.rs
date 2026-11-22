//! Proof that one studio cannot see or change another studio's data.
//!
//! Each test gets its own fresh database with all migrations applied.
//! Run with `DATABASE_URL` pointing at a Postgres server where the user may
//! create databases.

use ncrm_core::audit::{self, NewAuditEntry};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::staff::{self, NewStaff, StaffRole};
use ncrm_core::Error;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn studio(pool: &PgPool, slug: &str) -> Organization {
    orgs::create(
        pool,
        NewOrganization {
            name: format!("Studio {slug}"),
            slug: slug.to_string(),
            timezone: None,
        },
    )
    .await
    .expect("create studio")
}

async fn branch(pool: &PgPool, org: &Organization, name: &str) -> branches::Branch {
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    let b = branches::create(
        &mut tx,
        NewBranch {
            name: name.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    b
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_studio_lists_only_its_own_rows(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;
    branch(&pool, &a, "Alpha North").await;
    branch(&pool, &a, "Alpha South").await;
    branch(&pool, &b, "Beta Central").await;

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let names: Vec<String> = branches::list(&mut tx)
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert_eq!(names, ["Alpha North", "Alpha South"]);

    // Even SQL with no WHERE clause at all sees one studio.
    let orgs_visible: i64 = sqlx::query_scalar("SELECT count(*) FROM organizations")
        .fetch_one(tx.conn())
        .await
        .unwrap();
    assert_eq!(orgs_visible, 1);
    assert_eq!(orgs::current(&mut tx).await.unwrap().id, a.id);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn another_studios_row_looks_like_it_does_not_exist(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;
    let theirs = branch(&pool, &b, "Beta Central").await;

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let found = branches::get(&mut tx, theirs.id).await;
    assert!(matches!(found, Err(Error::NotFound)), "{found:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_studio_cannot_write_rows_into_another_studio(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let smuggled = sqlx::query("INSERT INTO branches (org_id, name) VALUES ($1, 'Smuggled')")
        .bind(b.id)
        .execute(tx.conn())
        .await;
    let err = smuggled.expect_err("insert into another studio must fail");
    assert!(
        err.to_string().contains("row-level security"),
        "unexpected error: {err}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_studio_cannot_update_or_delete_another_studios_rows(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;
    let theirs = branch(&pool, &b, "Beta Central").await;

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let updated = sqlx::query("UPDATE branches SET name = 'Hacked' WHERE id = $1")
        .bind(theirs.id)
        .execute(tx.conn())
        .await
        .unwrap();
    assert_eq!(updated.rows_affected(), 0);

    let deleted = sqlx::query("DELETE FROM branches")
        .execute(tx.conn())
        .await
        .unwrap();
    assert_eq!(deleted.rows_affected(), 0);

    let renamed_all = sqlx::query("UPDATE organizations SET name = 'Hacked'")
        .execute(tx.conn())
        .await
        .unwrap();
    assert_eq!(renamed_all.rows_affected(), 1, "only its own row");
    tx.commit().await.unwrap();

    let mut tx_b = TenantTx::begin(&pool, b.id).await.unwrap();
    assert_eq!(orgs::current(&mut tx_b).await.unwrap().name, "Studio beta");
    assert_eq!(
        branches::get(&mut tx_b, theirs.id).await.unwrap().name,
        "Beta Central"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_studio_cannot_move_its_row_to_another_studio(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;
    let mine = branch(&pool, &a, "Alpha North").await;

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let moved = sqlx::query("UPDATE branches SET org_id = $1 WHERE id = $2")
        .bind(b.id)
        .bind(mine.id)
        .execute(tx.conn())
        .await;
    assert!(moved.is_err(), "changing org_id must be refused");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn with_no_studio_set_the_app_role_sees_nothing(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    branch(&pool, &a, "Alpha North").await;

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE ncrm_app")
        .execute(&mut *tx)
        .await
        .unwrap();
    for table in ["organizations", "branches", "staff", "audit_log"] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table} leaked rows with no studio set");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn studio_scope_does_not_leak_to_the_next_user_of_a_connection(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    branch(&pool, &a, "Alpha North").await;

    // Force every transaction onto the same physical connection.
    let one = sqlx::pool::PoolOptions::<sqlx::Postgres>::new()
        .max_connections(1)
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();

    let mut scoped = TenantTx::begin(&one, a.id).await.unwrap();
    assert_eq!(branches::list(&mut scoped).await.unwrap().len(), 1);
    scoped.commit().await.unwrap();

    let mut next = one.begin().await.unwrap();
    let org_setting: Option<String> =
        sqlx::query_scalar("SELECT nullif(current_setting('app.org_id', true), '')")
            .fetch_one(&mut *next)
            .await
            .unwrap();
    assert_eq!(org_setting, None, "app.org_id survived the transaction");
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&mut *next)
        .await
        .unwrap();
    assert_ne!(role, "ncrm_app", "role survived the transaction");

    // A rolled-back (dropped) transaction must not leak either.
    drop(next);
    let dropped = TenantTx::begin(&one, a.id).await.unwrap();
    drop(dropped);
    let mut after = one.begin().await.unwrap();
    let org_setting: Option<String> =
        sqlx::query_scalar("SELECT nullif(current_setting('app.org_id', true), '')")
            .fetch_one(&mut *after)
            .await
            .unwrap();
    assert_eq!(org_setting, None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_app_role_cannot_create_or_delete_studios(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();

    let created = sqlx::query("INSERT INTO organizations (name, slug) VALUES ('Rogue', 'rogue')")
        .execute(tx.conn())
        .await;
    assert!(created.is_err());
    drop(tx);

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let deleted = sqlx::query("DELETE FROM organizations")
        .execute(tx.conn())
        .await;
    assert!(deleted.is_err());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn staff_cannot_be_attached_to_another_studios_branch(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;
    let theirs = branch(&pool, &b, "Beta Central").await;

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let result = staff::create(
        &mut tx,
        NewStaff {
            name: "Asha".into(),
            phone: "98765 43210".into(),
            role: StaffRole::Instructor,
            branch_id: Some(theirs.id),
        },
    )
    .await;
    assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_same_phone_can_work_at_two_studios_but_only_once_in_each(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;
    let new = |phone: &str| NewStaff {
        name: "Asha".into(),
        phone: phone.into(),
        role: StaffRole::Instructor,
        branch_id: None,
    };

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let first = staff::create(&mut tx, new("98765 43210")).await.unwrap();
    assert_eq!(first.phone, "+919876543210");
    assert_eq!(first.role, StaffRole::Instructor);
    tx.commit().await.unwrap();

    // Same person, written differently, same studio: refused.
    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let again = staff::create(&mut tx, new("+91-9876543210")).await;
    assert!(matches!(again, Err(Error::Conflict(_))), "{again:?}");
    drop(tx);

    // Same person at another studio: allowed, and each studio finds only its own.
    let mut tx = TenantTx::begin(&pool, b.id).await.unwrap();
    let second = staff::create(&mut tx, new("09876543210")).await.unwrap();
    let found = staff::find_by_phone(&mut tx, "9876543210")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, second.id);
    assert_eq!(staff::list(&mut tx).await.unwrap().len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_audit_log_is_per_studio_and_cannot_be_rewritten(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let entry = audit::record(
        &mut tx,
        NewAuditEntry {
            actor_staff_id: None,
            action: "branch.created".into(),
            entity: "branch".into(),
            entity_id: Some(Uuid::new_v4()),
            detail: json!({ "name": "Alpha North" }),
        },
    )
    .await
    .unwrap();
    assert_eq!(entry.detail["name"], "Alpha North");
    tx.commit().await.unwrap();

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    assert_eq!(audit::recent(&mut tx, 10).await.unwrap().len(), 1);
    let edited = sqlx::query("UPDATE audit_log SET action = 'nothing.happened'")
        .execute(tx.conn())
        .await;
    assert!(edited.is_err(), "audit entries must not be editable");
    drop(tx);

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let erased = sqlx::query("DELETE FROM audit_log")
        .execute(tx.conn())
        .await;
    assert!(erased.is_err(), "audit entries must not be deletable");
    drop(tx);

    let mut tx = TenantTx::begin(&pool, b.id).await.unwrap();
    assert!(audit::recent(&mut tx, 10).await.unwrap().is_empty());
}

/// Guard for future migrations: a new table without row-level security, or
/// without an org_id, fails this test.
#[sqlx::test(migrator = "MIGRATOR")]
async fn every_table_is_protected_by_row_level_security(pool: PgPool) {
    let unprotected: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname::text
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = 'public'
            AND c.relkind IN ('r', 'p')
            AND c.relname <> '_sqlx_migrations'
            AND NOT c.relrowsecurity
          ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        unprotected.is_empty(),
        "tables without row-level security: {unprotected:?}"
    );

    let without_policy: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname::text
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = 'public'
            AND c.relkind IN ('r', 'p')
            AND c.relname <> '_sqlx_migrations'
            AND NOT EXISTS (
                SELECT 1 FROM pg_policies p
                 WHERE p.schemaname = 'public'
                   AND p.tablename = c.relname
                   AND 'ncrm_app' = ANY (p.roles))
          ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        without_policy.is_empty(),
        "tables without an ncrm_app policy: {without_policy:?}"
    );

    let without_org_id: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname::text
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = 'public'
            AND c.relkind IN ('r', 'p')
            AND c.relname NOT IN ('_sqlx_migrations', 'organizations')
            AND NOT EXISTS (
                SELECT 1 FROM pg_attribute a
                 WHERE a.attrelid = c.oid
                   AND a.attname = 'org_id'
                   AND a.attnotnull
                   AND NOT a.attisdropped)
          ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        without_org_id.is_empty(),
        "tables without a NOT NULL org_id: {without_org_id:?}"
    );
}
