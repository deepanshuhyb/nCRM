//! Sign-in with one-time codes, against a real Postgres.

use ncrm_core::auth::{self, AuthConfig};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::staff::{self, NewStaff, StaffRole};
use ncrm_core::Error;
use sqlx::PgPool;

const PHONE: &str = "+919876543210";

fn cfg() -> AuthConfig {
    AuthConfig::new(vec![42u8; 32]).unwrap()
}

async fn studio(pool: &PgPool, slug: &str) -> Organization {
    orgs::create(
        pool,
        NewOrganization {
            name: format!("Studio {slug}"),
            slug: slug.into(),
            timezone: None,
        },
    )
    .await
    .unwrap()
}

async fn hire(pool: &PgPool, org: &Organization, phone: &str, role: StaffRole) -> staff::Staff {
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    let s = staff::create(
        &mut tx,
        NewStaff {
            name: "Asha".into(),
            phone: phone.into(),
            role,
            branch_id: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    s
}

/// A wrong code that is guaranteed to differ from `code`.
fn wrong(code: &str) -> String {
    let n: u32 = code.parse().unwrap();
    format!("{:06}", (n + 1) % 1_000_000)
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn staff_sign_in_and_land_in_their_only_studio(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let me = hire(&pool, &org, PHONE, StaffRole::Manager).await;

    // Typed with spaces and no country code: still the same person.
    let pending = auth::request_code(&pool, &cfg(), "98765 43210")
        .await
        .unwrap()
        .expect("staff get a code");
    assert_eq!(pending.phone, PHONE);
    assert_eq!(pending.code.len(), 6);

    let signed_in = auth::verify_code(&pool, &cfg(), PHONE, &pending.code)
        .await
        .unwrap();
    assert_eq!(signed_in.memberships.len(), 1);
    assert_eq!(signed_in.session.org_id, Some(org.id));
    assert_eq!(signed_in.session.staff_id, Some(me.id));

    let session = auth::session_from_token(&pool, &signed_in.token)
        .await
        .unwrap()
        .expect("token works");
    let ctx = auth::staff_context(&pool, &session)
        .await
        .unwrap()
        .expect("studio context");
    assert_eq!(ctx.org_id, org.id);
    assert_eq!(ctx.role, StaffRole::Manager);
    assert_eq!(ctx.org_name, "Studio alpha");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unknown_numbers_get_no_code_but_look_the_same(pool: PgPool) {
    studio(&pool, "alpha").await;
    let pending = auth::request_code(&pool, &cfg(), PHONE).await.unwrap();
    assert!(pending.is_none());

    // A row is still written, so rate limits behave the same for everyone.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.login_codes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);

    let result = auth::verify_code(&pool, &cfg(), PHONE, "123456").await;
    assert!(matches!(result, Err(Error::InvalidCode)), "{result:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_code_works_once(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    hire(&pool, &org, PHONE, StaffRole::Owner).await;
    let code = auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;

    auth::verify_code(&pool, &cfg(), PHONE, &code)
        .await
        .unwrap();
    let again = auth::verify_code(&pool, &cfg(), PHONE, &code).await;
    assert!(matches!(again, Err(Error::InvalidCode)), "{again:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn wrong_guesses_lock_the_code(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    hire(&pool, &org, PHONE, StaffRole::Owner).await;
    let cfg = cfg();
    let code = auth::request_code(&pool, &cfg, PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;

    for _ in 0..cfg.max_attempts {
        let r = auth::verify_code(&pool, &cfg, PHONE, &wrong(&code)).await;
        assert!(matches!(r, Err(Error::InvalidCode)), "{r:?}");
    }
    // Even the right code is refused once the guesses are used up.
    let r = auth::verify_code(&pool, &cfg, PHONE, &code).await;
    assert!(matches!(r, Err(Error::CodeLocked)), "{r:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn expired_codes_are_refused(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    hire(&pool, &org, PHONE, StaffRole::Owner).await;
    let code = auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;
    sqlx::query("UPDATE platform.login_codes SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();

    let r = auth::verify_code(&pool, &cfg(), PHONE, &code).await;
    assert!(matches!(r, Err(Error::InvalidCode)), "{r:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn asking_again_replaces_the_old_code(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    hire(&pool, &org, PHONE, StaffRole::Owner).await;
    let first = auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;
    let second = auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;

    if first != second {
        let r = auth::verify_code(&pool, &cfg(), PHONE, &first).await;
        assert!(matches!(r, Err(Error::InvalidCode)), "{r:?}");
    }
    auth::verify_code(&pool, &cfg(), PHONE, &second)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn too_many_requests_are_refused(pool: PgPool) {
    let cfg = cfg();
    for _ in 0..cfg.max_codes_per_hour {
        auth::request_code(&pool, &cfg, PHONE).await.unwrap();
    }
    let r = auth::request_code(&pool, &cfg, PHONE).await;
    assert!(matches!(r, Err(Error::RateLimited)), "{r:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn someone_at_two_studios_picks_one(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;
    let c = studio(&pool, "gamma").await;
    hire(&pool, &a, PHONE, StaffRole::Instructor).await;
    let at_b = hire(&pool, &b, PHONE, StaffRole::Owner).await;

    let code = auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;
    let signed_in = auth::verify_code(&pool, &cfg(), PHONE, &code)
        .await
        .unwrap();
    assert_eq!(signed_in.memberships.len(), 2);
    assert_eq!(signed_in.session.org_id, None, "no studio picked yet");
    assert!(auth::staff_context(&pool, &signed_in.session)
        .await
        .unwrap()
        .is_none());

    let session = auth::select_studio(&pool, &signed_in.session, b.id)
        .await
        .unwrap();
    let ctx = auth::staff_context(&pool, &session).await.unwrap().unwrap();
    assert_eq!(ctx.org_id, b.id);
    assert_eq!(ctx.staff_id, at_b.id);
    assert_eq!(ctx.role, StaffRole::Owner);

    // A studio they do not work at cannot be picked.
    let r = auth::select_studio(&pool, &session, c.id).await;
    assert!(matches!(r, Err(Error::NotFound)), "{r:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn deactivated_staff_lose_access_at_once(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let me = hire(&pool, &org, PHONE, StaffRole::FrontDesk).await;
    let code = auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;
    let signed_in = auth::verify_code(&pool, &cfg(), PHONE, &code)
        .await
        .unwrap();

    sqlx::query("UPDATE staff SET active = false WHERE id = $1")
        .bind(me.id)
        .execute(&pool)
        .await
        .unwrap();

    let session = auth::session_from_token(&pool, &signed_in.token)
        .await
        .unwrap()
        .unwrap();
    assert!(auth::staff_context(&pool, &session)
        .await
        .unwrap()
        .is_none());
    // And they cannot ask for a new code either.
    assert!(auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn signing_out_kills_the_token(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    hire(&pool, &org, PHONE, StaffRole::Owner).await;
    let code = auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;
    let signed_in = auth::verify_code(&pool, &cfg(), PHONE, &code)
        .await
        .unwrap();

    auth::sign_out(&pool, &signed_in.token).await.unwrap();
    assert!(auth::session_from_token(&pool, &signed_in.token)
        .await
        .unwrap()
        .is_none());
    assert!(auth::session_from_token(&pool, "not-a-real-token")
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tokens_and_codes_are_never_stored_in_the_clear(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    hire(&pool, &org, PHONE, StaffRole::Owner).await;
    let code = auth::request_code(&pool, &cfg(), PHONE)
        .await
        .unwrap()
        .unwrap()
        .code;
    let signed_in = auth::verify_code(&pool, &cfg(), PHONE, &code)
        .await
        .unwrap();

    let dump: String = sqlx::query_scalar(
        "SELECT concat_ws('|',
            (SELECT string_agg(encode(code_hash, 'escape'), ',') FROM platform.login_codes),
            (SELECT string_agg(encode(token_hash, 'escape'), ',') FROM platform.sessions))",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!dump.contains(&code));
    assert!(!dump.contains(&signed_in.token));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_studio_role_cannot_touch_sign_in_tables(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    for table in ["platform.login_codes", "platform.sessions"] {
        let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
        let r = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
            .execute(tx.conn())
            .await;
        let err = r.expect_err("ncrm_app must not read sign-in tables");
        assert!(
            err.to_string().contains("permission denied"),
            "{table}: {err}"
        );
    }
}
