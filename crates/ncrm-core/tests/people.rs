//! Team management rules and students with contacts, against real Postgres.

use ncrm_core::access::Actor;
use ncrm_core::auth::{self, AuthConfig};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::staff::{self, NewStaff, Staff, StaffEdit, StaffRole};
use ncrm_core::students::{
    self, ListQuery, NewContact, NewStudent, Relation, StudentEdit, StudentStatus,
};
use ncrm_core::Error;
use sqlx::PgPool;
use time::{Date, Month};
use uuid::Uuid;

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

async fn person(pool: &PgPool, org: &Organization, phone: &str, role: StaffRole) -> Staff {
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    let s = staff::create(
        &mut tx,
        NewStaff {
            name: format!("{role:?} {phone}"),
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

fn actor(s: &Staff) -> Actor {
    Actor {
        staff_id: s.id,
        role: s.role,
    }
}

fn edit_of(s: &Staff) -> StaffEdit {
    StaffEdit {
        name: s.name.clone(),
        phone: s.phone.clone(),
        role: s.role,
        branch_id: s.branch_id,
        active: s.active,
    }
}

fn hire_as(role: StaffRole, phone: &str) -> NewStaff {
    NewStaff {
        name: "New Person".into(),
        phone: phone.into(),
        role,
        branch_id: None,
    }
}

// ------------------------------------------------------------------ team

#[sqlx::test(migrator = "MIGRATOR")]
async fn managers_cannot_create_or_touch_owners(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let owner = person(&pool, &org, "+919800000001", StaffRole::Owner).await;
    let manager = person(&pool, &org, "+919800000002", StaffRole::Manager).await;
    let teacher = person(&pool, &org, "+919800000003", StaffRole::Instructor).await;

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let r = staff::hire(
        &mut tx,
        actor(&manager),
        hire_as(StaffRole::Owner, "+919800000009"),
    )
    .await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");
    drop(tx);

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let mut promote = edit_of(&teacher);
    promote.role = StaffRole::Owner;
    let r = staff::update(&mut tx, actor(&manager), teacher.id, promote).await;
    assert!(
        matches!(r, Err(Error::Forbidden)),
        "promote to owner: {r:?}"
    );
    drop(tx);

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let mut deactivate_owner = edit_of(&owner);
    deactivate_owner.active = false;
    let r = staff::update(&mut tx, actor(&manager), owner.id, deactivate_owner).await;
    assert!(
        matches!(r, Err(Error::Forbidden)),
        "deactivate owner: {r:?}"
    );
    drop(tx);

    // But a manager can run the rest of the team.
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let hired = staff::hire(
        &mut tx,
        actor(&manager),
        hire_as(StaffRole::FrontDesk, "+919800000010"),
    )
    .await
    .unwrap();
    let mut demote = edit_of(&teacher);
    demote.active = false;
    staff::update(&mut tx, actor(&manager), teacher.id, demote)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(hired.role, StaffRole::FrontDesk);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn front_desk_and_instructors_cannot_manage_the_team(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let desk = person(&pool, &org, "+919800000002", StaffRole::FrontDesk).await;
    let teacher = person(&pool, &org, "+919800000003", StaffRole::Instructor).await;
    for who in [&desk, &teacher] {
        let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
        let r = staff::hire(
            &mut tx,
            actor(who),
            hire_as(StaffRole::Instructor, "+919800000011"),
        )
        .await;
        assert!(matches!(r, Err(Error::Forbidden)), "{:?}: {r:?}", who.role);
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn nobody_can_demote_or_deactivate_themselves(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let owner = person(&pool, &org, "+919800000001", StaffRole::Owner).await;
    person(&pool, &org, "+919800000004", StaffRole::Owner).await;
    let manager = person(&pool, &org, "+919800000002", StaffRole::Manager).await;

    for (who, change) in [
        (&owner, StaffRole::Manager),
        (&manager, StaffRole::Instructor),
    ] {
        let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
        let mut e = edit_of(who);
        e.role = change;
        let r = staff::update(&mut tx, actor(who), who.id, e).await;
        assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    }
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let mut e = edit_of(&manager);
    e.active = false;
    let r = staff::update(&mut tx, actor(&manager), manager.id, e).await;
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    // Shadowing a transaction does NOT end it; this one holds a row lock
    // (`FOR UPDATE`) that the next transaction would wait on forever.
    drop(tx);

    // Editing your own name is fine.
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let mut e = edit_of(&manager);
    e.name = "Renamed Manager".into();
    let updated = staff::update(&mut tx, actor(&manager), manager.id, e)
        .await
        .unwrap();
    assert_eq!(updated.name, "Renamed Manager");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_studio_always_keeps_an_owner(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let a = person(&pool, &org, "+919800000001", StaffRole::Owner).await;
    let b = person(&pool, &org, "+919800000002", StaffRole::Owner).await;

    // Two owners: A can demote B.
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let mut e = edit_of(&b);
    e.role = StaffRole::Manager;
    staff::update(&mut tx, actor(&a), b.id, e).await.unwrap();
    tx.commit().await.unwrap();

    // Now B (a manager) can't touch A, and there's no other owner to demote
    // A, so A stays. Even an owner can't be deactivated if they're the last.
    let b = {
        let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
        staff::get(&mut tx, b.id).await.unwrap()
    };
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let mut e = edit_of(&a);
    e.active = false;
    let r = staff::update(&mut tx, actor(&b), a.id, e).await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");
    drop(tx);

    // The safety net itself: an owner acting on a stale view (their own row
    // was just deactivated elsewhere) still can't remove the last active
    // owner.
    let c = person(&pool, &org, "+919800000003", StaffRole::Owner).await;
    sqlx::query("UPDATE staff SET active = false WHERE id = $1")
        .bind(c.id)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let mut e = edit_of(&a);
    e.role = StaffRole::Manager;
    let r = staff::update(&mut tx, actor(&c), a.id, e).await;
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn changing_a_phone_number_signs_that_person_out(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let owner = person(&pool, &org, "+919800000001", StaffRole::Owner).await;
    let teacher = person(&pool, &org, "+919800000003", StaffRole::Instructor).await;
    let cfg = AuthConfig::new(vec![1u8; 32]).unwrap();

    let code = auth::request_code(&pool, &cfg, &teacher.phone)
        .await
        .unwrap()
        .unwrap()
        .code;
    let signed_in = auth::verify_code(&pool, &cfg, &teacher.phone, &code)
        .await
        .unwrap();

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let mut e = edit_of(&teacher);
    e.phone = "98000 00099".into();
    let updated = staff::update(&mut tx, actor(&owner), teacher.id, e)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(updated.phone, "+919800000099");

    let session = auth::session_from_token(&pool, &signed_in.token)
        .await
        .unwrap()
        .unwrap();
    assert!(auth::staff_context(&pool, &session)
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn team_changes_are_audited_without_phone_numbers(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let owner = person(&pool, &org, "+919800000001", StaffRole::Owner).await;
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let hired = staff::hire(
        &mut tx,
        actor(&owner),
        hire_as(StaffRole::Instructor, "+919800000050"),
    )
    .await
    .unwrap();
    let mut e = edit_of(&hired);
    e.phone = "+919800000051".into();
    e.role = StaffRole::FrontDesk;
    staff::update(&mut tx, actor(&owner), hired.id, e)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let details: Vec<String> = sqlx::query_scalar(
        "SELECT action || ' ' || detail::text FROM audit_log WHERE org_id = $1 ORDER BY id",
    )
    .bind(org.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(details.len(), 2);
    assert!(details[0].starts_with("staff.hired"));
    assert!(details[1].contains("\"phone\"") && details[1].contains("front_desk"));
    // Only what changed is described; `active` didn't change here.
    assert!(!details[1].contains("\"active\""));
    assert!(details
        .iter()
        .all(|d| !d.contains("9800000050") && !d.contains("9800000051")));
}

// -------------------------------------------------------------- students

fn kid(name: &str, contacts: Vec<NewContact>) -> NewStudent {
    NewStudent {
        name: name.into(),
        date_of_birth: Some(Date::from_calendar_date(2015, Month::March, 4).unwrap()),
        branch_id: None,
        notes: None,
        joined_on: None,
        contacts,
    }
}

fn parent(phone: &str, relation: Relation) -> NewContact {
    NewContact {
        name: "Priya Sharma".into(),
        phone: phone.into(),
        relation,
        is_primary: false,
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn siblings_share_one_parent_contact(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let desk = person(&pool, &org, "+919800000002", StaffRole::FrontDesk).await;

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let aarav = students::create(
        &mut tx,
        actor(&desk),
        kid("Aarav", vec![parent("98111 22233", Relation::Mother)]),
    )
    .await
    .unwrap();
    let mut second = parent("+919811122233", Relation::Mother);
    second.name = "Typed Differently".into();
    let anaya = students::create(&mut tx, actor(&desk), kid("Anaya", vec![second]))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(aarav.contacts[0].contact_id, anaya.contacts[0].contact_id);
    assert!(anaya.contacts[0].is_primary);
    assert_eq!(anaya.contacts[0].student_count, 2);
    // The existing contact keeps its name.
    assert_eq!(anaya.contacts[0].name, "Priya Sharma");

    let contacts: i64 = sqlx::query_scalar("SELECT count(*) FROM contacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(contacts, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn search_matches_names_and_phone_digits_literally(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let desk = person(&pool, &org, "+919800000002", StaffRole::FrontDesk).await;
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    for (name, phone) in [
        ("Meera Kapoor", "9811100001"),
        ("Meena Rao", "9811100002"),
        ("Kabir 100%", "9822200003"),
    ] {
        students::create(
            &mut tx,
            actor(&desk),
            kid(name, vec![parent(phone, Relation::Father)]),
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let names = |p: &students::Page<students::StudentListItem>| {
        p.items.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
    };

    let p = students::list(
        &mut tx,
        ListQuery {
            q: Some("mee".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(names(&p), ["Meena Rao", "Meera Kapoor"]);
    assert_eq!(p.total, 2);

    let p = students::list(
        &mut tx,
        ListQuery {
            q: Some("2220".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(names(&p), ["Kabir 100%"]);

    // '%' and '_' are literal, not wildcards.
    let p = students::list(
        &mut tx,
        ListQuery {
            q: Some("%".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(names(&p), ["Kabir 100%"]);
    let p = students::list(
        &mut tx,
        ListQuery {
            q: Some("_".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(p.items.is_empty());

    // Page size is capped.
    let p = students::list(
        &mut tx,
        ListQuery {
            limit: Some(10_000),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(p.limit, students::MAX_PAGE);
    assert_eq!(
        p.items[0].primary_contact_phone.as_deref(),
        Some("+919822200003")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn status_changes_and_contact_management(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let desk = person(&pool, &org, "+919800000002", StaffRole::FrontDesk).await;
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let s = students::create(
        &mut tx,
        actor(&desk),
        kid("Kabir", vec![parent("9811100001", Relation::Mother)]),
    )
    .await
    .unwrap();

    let s = students::add_contact(
        &mut tx,
        actor(&desk),
        s.student.id,
        NewContact {
            name: "Rohit".into(),
            phone: "9811100002".into(),
            relation: Relation::Father,
            is_primary: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(s.contacts.iter().filter(|c| c.is_primary).count(), 1);
    assert_eq!(s.contacts[0].name, "Rohit");

    let dup = students::add_contact(
        &mut tx,
        actor(&desk),
        s.student.id,
        parent("+91 98111 00001", Relation::Other),
    )
    .await;
    assert!(matches!(dup, Err(Error::Conflict(_))), "{dup:?}");

    // Removing the primary promotes the remaining contact.
    let rohit = s.contacts[0].contact_id;
    let s = students::remove_contact(&mut tx, actor(&desk), s.student.id, rohit)
        .await
        .unwrap();
    assert_eq!(s.contacts.len(), 1);
    assert!(s.contacts[0].is_primary);

    let s = students::update(
        &mut tx,
        actor(&desk),
        s.student.id,
        StudentEdit {
            name: "Kabir T.".into(),
            date_of_birth: None,
            branch_id: None,
            status: StudentStatus::Left,
            notes: Some("  Moved away  ".into()),
            joined_on: s.student.joined_on,
        },
    )
    .await
    .unwrap();
    assert_eq!(s.student.status, StudentStatus::Left);
    assert_eq!(s.student.notes.as_deref(), Some("Moved away"));

    let active = students::list(
        &mut tx,
        ListQuery {
            status: Some(StudentStatus::Active),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(active.total, 0);
    assert_eq!(students::count_active(&mut tx).await.unwrap(), 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn instructors_cannot_manage_students(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let teacher = person(&pool, &org, "+919800000003", StaffRole::Instructor).await;
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let r = students::create(&mut tx, actor(&teacher), kid("Nope", vec![])).await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn students_and_contacts_never_cross_studios(pool: PgPool) {
    let a = studio(&pool, "alpha").await;
    let b = studio(&pool, "beta").await;
    let desk_a = person(&pool, &a, "+919800000002", StaffRole::FrontDesk).await;
    let desk_b = person(&pool, &b, "+919800000005", StaffRole::FrontDesk).await;
    let branch_b = {
        let mut tx = TenantTx::begin(&pool, b.id).await.unwrap();
        let br = branches::create(
            &mut tx,
            NewBranch {
                name: "Beta Main".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        br
    };

    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let theirs_parent = "+919811199999";
    let mine = students::create(
        &mut tx,
        actor(&desk_a),
        kid("Alpha Kid", vec![parent(theirs_parent, Relation::Mother)]),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = TenantTx::begin(&pool, b.id).await.unwrap();
    // Same parent number at another studio is a separate contact there.
    let beta_kid = students::create(
        &mut tx,
        actor(&desk_b),
        kid("Beta Kid", vec![parent(theirs_parent, Relation::Mother)]),
    )
    .await
    .unwrap();
    assert_ne!(beta_kid.contacts[0].contact_id, mine.contacts[0].contact_id);
    assert_eq!(beta_kid.contacts[0].student_count, 1);

    // Studio B can't see, edit or link to studio A's student.
    assert!(matches!(
        students::get(&mut tx, mine.student.id).await,
        Err(Error::NotFound)
    ));
    let edit = StudentEdit {
        name: "Hijacked".into(),
        date_of_birth: None,
        branch_id: None,
        status: StudentStatus::Left,
        notes: None,
        joined_on: mine.student.joined_on,
    };
    assert!(matches!(
        students::update(&mut tx, actor(&desk_b), mine.student.id, edit).await,
        Err(Error::NotFound)
    ));
    let r = students::remove_contact(
        &mut tx,
        actor(&desk_b),
        mine.student.id,
        mine.contacts[0].contact_id,
    )
    .await;
    assert!(matches!(r, Err(Error::NotFound)), "{r:?}");
    let listed = students::list(&mut tx, ListQuery::default()).await.unwrap();
    assert_eq!(listed.total, 1);
    tx.commit().await.unwrap();

    // Studio A can't put a student in studio B's branch.
    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let mut planted = kid("Planted", vec![]);
    planted.branch_id = Some(branch_b.id);
    let r = students::create(&mut tx, actor(&desk_a), planted).await;
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    drop(tx);

    // Even raw SQL can't link A's student to B's contact.
    let mut tx = TenantTx::begin(&pool, a.id).await.unwrap();
    let r = sqlx::query("INSERT INTO student_contacts (org_id, student_id, contact_id, relation) VALUES ($1, $2, $3, 'other')")
        .bind(a.id)
        .bind(mine.student.id)
        .bind(beta_kid.contacts[0].contact_id)
        .execute(tx.conn())
        .await;
    assert!(r.is_err());
    let _ = Uuid::nil();
}

// ------------------------------------------------------------ join dates

#[sqlx::test(migrator = "MIGRATOR")]
async fn join_dates_follow_the_studio_calendar_and_can_be_backdated(pool: PgPool) {
    let org = studio(&pool, "alpha").await;
    let desk = person(&pool, &org, "+919800000003", StaffRole::FrontDesk).await;
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let today = tx.studio_today().await.unwrap();

    // Not given: today, in the studio's calendar, and not backdated.
    let now = students::create(&mut tx, actor(&desk), kid("Aarav", vec![]))
        .await
        .unwrap();
    assert_eq!(now.student.joined_on, today);
    assert_eq!(now.student.entered_on, today);

    // Backdated: kept as given; entered_on shows when it was typed in.
    let earlier = Date::from_calendar_date(2024, Month::March, 3).unwrap();
    let back = students::create(
        &mut tx,
        actor(&desk),
        NewStudent {
            joined_on: Some(earlier),
            ..kid("Bela", vec![])
        },
    )
    .await
    .unwrap();
    assert_eq!(back.student.joined_on, earlier);
    assert!(back.student.entered_on > back.student.joined_on);
    let flagged: bool = sqlx::query_scalar(
        "SELECT (detail->>'backdated')::bool FROM audit_log WHERE entity_id = $1",
    )
    .bind(back.student.id)
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert!(flagged);

    for (when, why) in [
        (today.next_day().unwrap(), "future"),
        (
            Date::from_calendar_date(1999, Month::December, 31).unwrap(),
            "too old",
        ),
        // kid() was born in 2015.
        (
            Date::from_calendar_date(2014, Month::January, 1).unwrap(),
            "before birth",
        ),
    ] {
        let r = students::create(
            &mut tx,
            actor(&desk),
            NewStudent {
                joined_on: Some(when),
                ..kid("Chirag", vec![])
            },
        )
        .await;
        assert!(matches!(r, Err(Error::Invalid(_))), "{why}: {r:?}");
    }

    // 01:30 in India on 23 Nov is still 22 Nov in UTC.
    let local: Date =
        sqlx::query_scalar("SELECT studio_date($1, '2026-11-22T20:00:00Z'::timestamptz)")
            .bind(org.id)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    assert_eq!(
        local,
        Date::from_calendar_date(2026, Month::November, 23).unwrap()
    );
}
