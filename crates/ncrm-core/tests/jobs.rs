//! The worker's jobs against real Postgres: they tidy what is old, create
//! what is due, change nothing the second time, and keep studios apart.

use ncrm_core::access::Actor;
use ncrm_core::batches::{self, NewBatch, Slot};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::enrollments::{self, NewEnrollment};
use ncrm_core::fees::{self, FeeSettings};
use ncrm_core::orgs::{self, NewOrganization};
use ncrm_core::plans::{self, NewPlan, PlanKind};
use ncrm_core::staff::{self, NewStaff, StaffRole};
use ncrm_core::students::{self, NewStudent};
use ncrm_core::{auth, jobs};
use sqlx::PgPool;
use time::macros::time;
use time::Duration;
use uuid::Uuid;

/// A studio with a daily class, one student on a monthly plan, and monthly
/// fees switched on (but not yet created: nobody has looked).
async fn studio(pool: &PgPool, slug: &str) -> (Uuid, Actor) {
    let org = orgs::create(
        pool,
        NewOrganization {
            name: format!("Studio {slug}"),
            slug: slug.into(),
            timezone: None,
        },
    )
    .await
    .unwrap();
    let mut t = TenantTx::begin(pool, org.id).await.unwrap();
    let today = t.studio_today().await.unwrap();
    let owner = staff::create(
        &mut t,
        NewStaff {
            name: "Asha".into(),
            phone: "+919800000001".into(),
            role: StaffRole::Owner,
            branch_id: None,
        },
    )
    .await
    .unwrap();
    let me = Actor {
        staff_id: owner.id,
        role: owner.role,
    };
    let branch = branches::create(
        &mut t,
        NewBranch {
            name: "North".into(),
            ..NewBranch::default()
        },
    )
    .await
    .unwrap()
    .id;
    let batch = batches::create(
        &mut t,
        me,
        NewBatch {
            name: "Hip-hop".into(),
            style: None,
            branch_id: branch,
            room_id: None,
            instructor_id: None,
            capacity: None,
            starts_on: Some(today - Duration::days(30)),
            ends_on: None,
            slots: (1..=7)
                .map(|weekday| Slot {
                    weekday,
                    starts_at: time!(18:00),
                    ends_at: time!(19:00),
                })
                .collect(),
        },
    )
    .await
    .unwrap()
    .summary
    .batch
    .id;
    let monthly = plans::create(
        &mut t,
        me,
        NewPlan {
            name: "Monthly".into(),
            kind: PlanKind::Monthly,
            price_paise: 200_000,
            classes: None,
            valid_days: None,
        },
    )
    .await
    .unwrap()
    .id;
    let kid = students::create(
        &mut t,
        me,
        NewStudent {
            name: "Bela".into(),
            date_of_birth: None,
            branch_id: None,
            notes: None,
            joined_on: Some(today - Duration::days(20)),
            contacts: vec![],
        },
    )
    .await
    .unwrap()
    .student
    .id;
    // Fees are off while they join, so nothing is charged yet.
    enrollments::enroll(
        &mut t,
        me,
        kid,
        NewEnrollment {
            batch_id: batch,
            plan_id: Some(monthly),
            starts_on: Some(today - Duration::days(20)),
        },
    )
    .await
    .unwrap();
    // Switched on directly, so the charge is left for the job to create.
    sqlx::query("UPDATE organizations SET fees_from = $2 WHERE id = $1")
        .bind(org.id)
        .bind(today.replace_day(1).unwrap())
        .execute(t.conn())
        .await
        .unwrap();
    t.commit().await.unwrap();
    (org.id, me)
}

async fn count(pool: &PgPool, org_id: Uuid, sql: &str) -> i64 {
    let mut t = TenantTx::begin(pool, org_id).await.unwrap();
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(t.conn())
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn housekeeping_creates_what_is_due_once_and_keeps_studios_apart(pool: PgPool) {
    let (alpha, me) = studio(&pool, "alpha").await;
    let (beta, _) = studio(&pool, "beta").await;
    assert_eq!(
        count(&pool, alpha, "SELECT count(*) FROM sessions").await,
        0
    );
    assert_eq!(count(&pool, alpha, "SELECT count(*) FROM charges").await, 0);

    let first = jobs::housekeeping(&pool).await.unwrap();
    assert_eq!(first.studios, 2);
    assert_eq!(first.fees_created, 2, "one fee in each studio");
    assert!(first.failed.is_empty(), "{:?}", first.failed);
    // Eight days of a daily class: a week back, and today.
    for org in [alpha, beta] {
        assert_eq!(count(&pool, org, "SELECT count(*) FROM sessions").await, 8);
        assert_eq!(count(&pool, org, "SELECT count(*) FROM charges").await, 1);
    }

    // The second pass has nothing left to do.
    let second = jobs::housekeeping(&pool).await.unwrap();
    assert_eq!(second.fees_created, 0);
    assert_eq!(
        count(&pool, alpha, "SELECT count(*) FROM sessions").await,
        8
    );
    assert_eq!(count(&pool, alpha, "SELECT count(*) FROM charges").await, 1);

    // The dashboard agrees with what the job made.
    let mut t = TenantTx::begin(&pool, alpha).await.unwrap();
    let summary = fees::summary(&mut t, me).await.unwrap();
    assert_eq!((summary.due_paise, summary.students_owing), (200_000, 1));
    // Switching fees off stops the job creating more (nothing new anyway).
    fees::set_settings(
        &mut t,
        me,
        FeeSettings {
            fees_from: None,
            fee_due_day: 5,
            fee_reminders: None,
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();
    assert_eq!(jobs::studio_upkeep(&pool, alpha).await.unwrap(), 0);

    // A studio that doesn't exist fails by itself.
    assert!(jobs::studio_upkeep(&pool, Uuid::new_v4()).await.is_err());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn cleanup_removes_only_what_is_old(pool: PgPool) {
    let hash = vec![7u8; 32];
    for (age, expires) in [
        ("2 days", "2 days"),
        ("2 hours", "2 hours"),
        ("0 minutes", "-10 minutes"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO platform.login_codes (phone, code_hash, created_at, expires_at)
             VALUES ('+919800000001', $1, now() - interval '{age}', now() - interval '{expires}')"
        )))
        .bind(&hash)
        .execute(&pool)
        .await
        .unwrap();
    }
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.login_codes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, 3);

    let done = auth::cleanup(&pool).await.unwrap();
    assert_eq!(done.codes, 1, "only the two-day-old code");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.login_codes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        left, 2,
        "the last hour's codes still count towards the limit"
    );
    assert_eq!(
        auth::cleanup(&pool).await.unwrap(),
        auth::Cleanup::default()
    );

    // Sessions: live ones stay, as do ones that only just expired or were
    // signed out; older ones go.
    let rows = [
        (1u8, "now() + interval '20 days'", "NULL"),
        (2, "now() - interval '1 day'", "NULL"),
        (3, "now() - interval '8 days'", "NULL"),
        (4, "now() + interval '20 days'", "now() - interval '9 days'"),
        (5, "now() + interval '20 days'", "now() - interval '1 hour'"),
    ];
    for (n, expires, revoked) in rows {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO platform.sessions (token_hash, phone, expires_at, revoked_at)
             VALUES ($1, '+919800000001', {expires}, {revoked})"
        )))
        .bind(vec![n; 32])
        .execute(&pool)
        .await
        .unwrap();
    }
    let done = auth::cleanup(&pool).await.unwrap();
    assert_eq!((done.codes, done.sessions), (0, 2));
    let kept: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT token_hash FROM platform.sessions ORDER BY token_hash")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(kept.iter().map(|h| h[0]).collect::<Vec<_>>(), [1, 2, 5]);
}
