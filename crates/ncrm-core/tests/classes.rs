//! Rooms, classes and schedules, plans, enrollments and class credits,
//! against real Postgres. Each rule is tested from the side that should be
//! refused as well as the side that should work.

use std::time::Duration as StdDuration;

use ncrm_core::access::Actor;
use ncrm_core::batches::{self, BatchEdit, NewBatch, Slot};
use ncrm_core::branches::{self, Branch, NewBranch};
use ncrm_core::credits::{self, GiveCredits, SellPack, TakeBack};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::enrollments::{self, NewEnrollment};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::plans::{self, NewPlan, Plan, PlanEdit, PlanKind};
use ncrm_core::rooms::{self, NewRoom, Room, RoomEdit};
use ncrm_core::staff::{self, NewStaff, Staff, StaffRole};
use ncrm_core::students::{self, NewStudent, StudentEdit, StudentStatus};
use ncrm_core::Error;
use sqlx::PgPool;
use time::macros::time;
use time::{Duration, Time};
use uuid::Uuid;

// ----------------------------------------------------------------- setup

struct Studio {
    org: Organization,
    owner: Staff,
    front_desk: Staff,
    teacher: Staff,
    other_teacher: Staff,
    north: Branch,
    south: Branch,
    studio_a: Room,
}

async fn studio(pool: &PgPool, slug: &str) -> Studio {
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
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    let person = |phone: &'static str, role: StaffRole, name: &'static str| NewStaff {
        name: name.into(),
        phone: phone.into(),
        role,
        branch_id: None,
    };
    let owner = staff::create(&mut tx, person("+919800000001", StaffRole::Owner, "Asha"))
        .await
        .unwrap();
    let front_desk = staff::create(
        &mut tx,
        person("+919800000002", StaffRole::FrontDesk, "Farah"),
    )
    .await
    .unwrap();
    let teacher = staff::create(
        &mut tx,
        person("+919800000003", StaffRole::Instructor, "Ritu"),
    )
    .await
    .unwrap();
    let other_teacher = staff::create(
        &mut tx,
        person("+919800000004", StaffRole::Instructor, "Kabir"),
    )
    .await
    .unwrap();
    let north = branches::create(&mut tx, branch("North")).await.unwrap();
    let south = branches::create(&mut tx, branch("South")).await.unwrap();
    let studio_a = rooms::create(
        &mut tx,
        actor(&owner),
        north.id,
        NewRoom {
            name: "Studio A".into(),
            capacity: Some(20),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    Studio {
        org,
        owner,
        front_desk,
        teacher,
        other_teacher,
        north,
        south,
        studio_a,
    }
}

fn branch(name: &str) -> NewBranch {
    NewBranch {
        name: name.into(),
        ..NewBranch::default()
    }
}

fn actor(s: &Staff) -> Actor {
    Actor {
        staff_id: s.id,
        role: s.role,
    }
}

fn slot(weekday: i16, from: Time, to: Time) -> Slot {
    Slot {
        weekday,
        starts_at: from,
        ends_at: to,
    }
}

fn class(s: &Studio, name: &str, slots: Vec<Slot>) -> NewBatch {
    NewBatch {
        name: name.into(),
        style: Some("Hip-hop".into()),
        branch_id: s.north.id,
        room_id: Some(s.studio_a.id),
        instructor_id: Some(s.teacher.id),
        capacity: None,
        starts_on: None,
        ends_on: None,
        slots,
    }
}

fn edit_of(d: &batches::BatchDetail) -> BatchEdit {
    let b = &d.summary.batch;
    BatchEdit {
        name: b.name.clone(),
        style: b.style.clone(),
        branch_id: b.branch_id,
        room_id: b.room_id,
        instructor_id: b.instructor_id,
        capacity: b.capacity,
        starts_on: b.starts_on,
        ends_on: b.ends_on,
        active: b.active,
        slots: d.summary.slots.clone(),
    }
}

async fn tx(pool: &PgPool, s: &Studio) -> TenantTx {
    TenantTx::begin(pool, s.org.id).await.unwrap()
}

async fn student(pool: &PgPool, s: &Studio, name: &str) -> Uuid {
    let mut t = tx(pool, s).await;
    let created = students::create(
        &mut t,
        actor(&s.owner),
        NewStudent {
            name: name.into(),
            date_of_birth: None,
            branch_id: Some(s.north.id),
            notes: None,
            contacts: vec![],
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();
    created.student.id
}

async fn plan(pool: &PgPool, s: &Studio, name: &str, kind: PlanKind, classes: Option<i32>) -> Plan {
    let mut t = tx(pool, s).await;
    let p = plans::create(
        &mut t,
        actor(&s.owner),
        NewPlan {
            name: name.into(),
            kind,
            price_paise: 250_000,
            classes,
            valid_days: Some(30),
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();
    p
}

fn mon_wed_6pm() -> Vec<Slot> {
    vec![
        slot(1, time!(18:00), time!(19:00)),
        slot(3, time!(18:00), time!(19:00)),
    ]
}

// ----------------------------------------------------------------- rooms

#[sqlx::test(migrator = "MIGRATOR")]
async fn rooms_belong_to_a_branch_and_only_managers_add_them(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let other = studio(&pool, "beta").await;
    let mut t = tx(&pool, &s).await;

    let by_desk = rooms::create(
        &mut t,
        actor(&s.front_desk),
        s.north.id,
        NewRoom {
            name: "Studio B".into(),
            capacity: None,
        },
    )
    .await;
    assert!(matches!(by_desk, Err(Error::Forbidden)), "{by_desk:?}");

    let dup = rooms::create(
        &mut t,
        actor(&s.owner),
        s.north.id,
        NewRoom {
            name: "studio a".into(),
            capacity: None,
        },
    )
    .await;
    assert!(matches!(dup, Err(Error::Conflict(_))), "{dup:?}");

    // Another studio's branch doesn't exist from here.
    let elsewhere = rooms::create(
        &mut t,
        actor(&s.owner),
        other.north.id,
        NewRoom {
            name: "Sneaky".into(),
            capacity: None,
        },
    )
    .await;
    assert!(matches!(elsewhere, Err(Error::NotFound)), "{elsewhere:?}");

    // The same name in another branch is fine.
    rooms::create(
        &mut t,
        actor(&s.owner),
        s.south.id,
        NewRoom {
            name: "Studio A".into(),
            capacity: Some(12),
        },
    )
    .await
    .unwrap();
    assert_eq!(rooms::list(&mut t).await.unwrap().len(), 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_room_in_use_cant_close_or_shrink_below_its_class(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let mut t = tx(&pool, &s).await;
    batches::create(
        &mut t,
        actor(&s.owner),
        class(&s, "Hip-hop Beginners", mon_wed_6pm()),
    )
    .await
    .unwrap();

    let close = rooms::update(
        &mut t,
        actor(&s.owner),
        s.studio_a.id,
        RoomEdit {
            name: "Studio A".into(),
            capacity: Some(20),
            active: false,
        },
    )
    .await;
    assert!(
        matches!(&close, Err(Error::Invalid(m)) if m.contains("Hip-hop Beginners still meets")),
        "{close:?}"
    );
    let shrink = rooms::update(
        &mut t,
        actor(&s.owner),
        s.studio_a.id,
        RoomEdit {
            name: "Studio A".into(),
            capacity: Some(10),
            active: true,
        },
    )
    .await;
    assert!(matches!(shrink, Err(Error::Invalid(_))), "{shrink:?}");
}

// ------------------------------------------------------- classes, schedule

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_class_takes_its_rooms_capacity_and_cant_exceed_it(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let mut t = tx(&pool, &s).await;
    let created = batches::create(&mut t, actor(&s.owner), class(&s, "Hip-hop", mon_wed_6pm()))
        .await
        .unwrap();
    assert_eq!(created.summary.batch.capacity, Some(20));
    assert_eq!(created.summary.slots.len(), 2);
    assert_eq!(created.summary.room_name.as_deref(), Some("Studio A"));
    assert_eq!(created.summary.instructor_name.as_deref(), Some("Ritu"));

    let mut too_big = class(&s, "Big", vec![slot(2, time!(10:00), time!(11:00))]);
    too_big.capacity = Some(25);
    let r = batches::create(&mut t, actor(&s.owner), too_big).await;
    assert!(
        matches!(&r, Err(Error::Invalid(m)) if m.contains("Studio A holds 20")),
        "{r:?}"
    );

    // A room from another branch is refused.
    let mut wrong_branch = class(&s, "Elsewhere", vec![slot(2, time!(10:00), time!(11:00))]);
    wrong_branch.branch_id = s.south.id;
    let r = batches::create(&mut t, actor(&s.owner), wrong_branch).await;
    assert!(
        matches!(&r, Err(Error::Invalid(m)) if m.contains("different branch")),
        "{r:?}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_room_and_a_teacher_cant_be_in_two_places_at_once(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let mut t = tx(&pool, &s).await;
    batches::create(
        &mut t,
        actor(&s.owner),
        class(&s, "Hip-hop Beginners", mon_wed_6pm()),
    )
    .await
    .unwrap();

    // Same room, overlapping Wednesday time, different teacher.
    let mut same_room = class(&s, "Salsa", vec![slot(3, time!(18:30), time!(19:30))]);
    same_room.instructor_id = Some(s.other_teacher.id);
    let r = batches::create(&mut t, actor(&s.owner), same_room).await;
    assert!(
        matches!(&r, Err(Error::Conflict(m))
            if m == "Studio A is booked for Hip-hop Beginners on Wednesday 18:00–19:00."),
        "{r:?}"
    );

    // Same teacher at another branch at the same time.
    let mut same_teacher = class(
        &s,
        "Contemporary",
        vec![slot(1, time!(17:30), time!(18:30))],
    );
    same_teacher.branch_id = s.south.id;
    same_teacher.room_id = None;
    let r = batches::create(&mut t, actor(&s.owner), same_teacher).await;
    assert!(
        matches!(&r, Err(Error::Conflict(m))
            if m == "Ritu teaches Hip-hop Beginners on Monday 18:00–19:00."),
        "{r:?}"
    );

    // Back to back in the same room with the same teacher is fine.
    batches::create(
        &mut t,
        actor(&s.owner),
        class(
            &s,
            "Hip-hop Advanced",
            vec![slot(1, time!(19:00), time!(20:00))],
        ),
    )
    .await
    .unwrap();

    // A class that ends before another starts doesn't clash with it.
    let today = t.studio_today().await.unwrap();
    let mut summer = class(&s, "Summer camp", mon_wed_6pm());
    summer.starts_on = Some(today + Duration::days(400));
    let mut earlier = class(&s, "Old", vec![slot(2, time!(9:00), time!(10:00))]);
    earlier.ends_on = Some(today + Duration::days(30));
    batches::create(&mut t, actor(&s.owner), earlier)
        .await
        .unwrap();
    // Summer camp clashes with Hip-hop Beginners, which has no end date.
    assert!(batches::create(&mut t, actor(&s.owner), summer)
        .await
        .is_err());
    let mut after_old = class(&s, "After old", vec![slot(2, time!(9:00), time!(10:00))]);
    after_old.starts_on = Some(today + Duration::days(31));
    batches::create(&mut t, actor(&s.owner), after_old)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_archived_class_frees_its_slot_and_its_students(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let kid = student(&pool, &s, "Aarav").await;
    let mut t = tx(&pool, &s).await;
    let hiphop = batches::create(&mut t, actor(&s.owner), class(&s, "Hip-hop", mon_wed_6pm()))
        .await
        .unwrap();
    enrollments::enroll(
        &mut t,
        actor(&s.front_desk),
        kid,
        NewEnrollment {
            batch_id: hiphop.summary.batch.id,
            plan_id: None,
            starts_on: None,
        },
    )
    .await
    .unwrap();

    let mut archive = edit_of(&hiphop);
    archive.active = false;
    let archived = batches::update(&mut t, actor(&s.owner), hiphop.summary.batch.id, archive)
        .await
        .unwrap();
    assert!(!archived.summary.batch.active);
    assert_eq!(archived.summary.enrolled, 0);
    let theirs = enrollments::for_student(&mut t, kid).await.unwrap();
    assert!(theirs[0].ended_on.is_some());

    // The slot is free again.
    batches::create(&mut t, actor(&s.owner), class(&s, "Salsa", mon_wed_6pm()))
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_managers_shape_classes(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let mut t = tx(&pool, &s).await;
    for who in [&s.front_desk, &s.teacher] {
        let r = batches::create(&mut t, actor(who), class(&s, "Nope", mon_wed_6pm())).await;
        assert!(matches!(r, Err(Error::Forbidden)), "{:?}: {r:?}", who.role);
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_a_classs_own_teacher_sees_its_students(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let kid = student(&pool, &s, "Aarav").await;
    let mut t = tx(&pool, &s).await;
    let hiphop = batches::create(&mut t, actor(&s.owner), class(&s, "Hip-hop", mon_wed_6pm()))
        .await
        .unwrap();
    let id = hiphop.summary.batch.id;
    enrollments::enroll(
        &mut t,
        actor(&s.front_desk),
        kid,
        NewEnrollment {
            batch_id: id,
            plan_id: None,
            starts_on: None,
        },
    )
    .await
    .unwrap();

    let own = batches::get(&mut t, actor(&s.teacher), id).await.unwrap();
    assert_eq!(own.roster.unwrap()[0].student_name, "Aarav");
    let other = batches::get(&mut t, actor(&s.other_teacher), id)
        .await
        .unwrap();
    assert!(other.roster.is_none());
    assert_eq!(other.summary.enrolled, 1, "counts are fine to see");
    let desk = batches::get(&mut t, actor(&s.front_desk), id)
        .await
        .unwrap();
    assert_eq!(desk.roster.unwrap().len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn two_people_booking_the_same_room_at_once_cant_both_win(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let mut first = tx(&pool, &s).await;
    batches::create(
        &mut first,
        actor(&s.owner),
        class(&s, "First", mon_wed_6pm()),
    )
    .await
    .unwrap();

    // The second save starts while the first is still open. It waits on
    // the studio's schedule lock, then sees the first class.
    let pool2 = pool.clone();
    let org_id = s.org.id;
    let mut second_class = class(&s, "Second", mon_wed_6pm());
    second_class.instructor_id = Some(s.other_teacher.id);
    let owner = actor(&s.owner);
    let second = tokio::spawn(async move {
        let mut t = TenantTx::begin(&pool2, org_id).await.unwrap();
        let r = batches::create(&mut t, owner, second_class).await;
        if r.is_ok() {
            t.commit().await.unwrap();
        }
        r.map(|_| ())
    });
    tokio::time::sleep(StdDuration::from_millis(300)).await;
    first.commit().await.unwrap();
    let r = second.await.unwrap();
    assert!(matches!(r, Err(Error::Conflict(_))), "{r:?}");
}

// ----------------------------------------------------------------- plans

#[sqlx::test(migrator = "MIGRATOR")]
async fn plans_have_the_right_shape(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let mut t = tx(&pool, &s).await;
    let new = |kind, classes, valid_days| NewPlan {
        name: format!("{kind:?} {classes:?} {valid_days:?}"),
        kind,
        price_paise: 100_000,
        classes,
        valid_days,
    };

    let monthly = plans::create(
        &mut t,
        actor(&s.owner),
        new(PlanKind::Monthly, Some(8), Some(30)),
    )
    .await
    .unwrap();
    assert_eq!((monthly.classes, monthly.valid_days), (None, None));
    let drop_in = plans::create(
        &mut t,
        actor(&s.owner),
        new(PlanKind::DropIn, Some(5), None),
    )
    .await
    .unwrap();
    assert_eq!((drop_in.classes, drop_in.valid_days), (Some(1), Some(30)));
    let no_count =
        plans::create(&mut t, actor(&s.owner), new(PlanKind::Pack, None, Some(30))).await;
    assert!(matches!(no_count, Err(Error::Invalid(_))), "{no_count:?}");
    let pricey = plans::create(
        &mut t,
        actor(&s.owner),
        NewPlan {
            price_paise: 100_000_001,
            ..new(PlanKind::Monthly, None, None)
        },
    )
    .await;
    assert!(matches!(pricey, Err(Error::Invalid(_))), "{pricey:?}");

    // Front desk sells plans but doesn't set prices.
    let r = plans::create(
        &mut t,
        actor(&s.front_desk),
        new(PlanKind::Monthly, None, None),
    )
    .await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");

    // The kind can't change through an edit: a monthly plan stays monthly
    // and ignores a class count.
    let edited = plans::update(
        &mut t,
        actor(&s.owner),
        monthly.id,
        PlanEdit {
            name: "Monthly, 2 a week".into(),
            price_paise: 180_000,
            classes: Some(10),
            valid_days: Some(10),
            active: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(edited.kind, PlanKind::Monthly);
    assert_eq!(edited.classes, None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_database_refuses_a_pack_without_a_class_count(pool: PgPool) {
    // A CHECK that evaluates to NULL passes. plans_shape is written so it
    // can't: this is the raw insert the application would never send.
    let s = studio(&pool, "alpha").await;
    let mut t = tx(&pool, &s).await;
    let r = sqlx::query(
        "INSERT INTO plans (org_id, name, kind, price_paise, classes, valid_days)
         VALUES ($1, 'Broken', 'pack', 100, NULL, 30)",
    )
    .bind(s.org.id)
    .execute(t.conn())
    .await;
    let err = Error::from(r.unwrap_err());
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
}

// ----------------------------------------------------------- enrollments

#[sqlx::test(migrator = "MIGRATOR")]
async fn enrolling_respects_capacity_status_and_plan_kind(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let a = student(&pool, &s, "Aarav").await;
    let b = student(&pool, &s, "Bela").await;
    let c = student(&pool, &s, "Chirag").await;
    let monthly = plan(&pool, &s, "Monthly", PlanKind::Monthly, None).await;
    let pack = plan(&pool, &s, "Ten classes", PlanKind::Pack, Some(10)).await;
    let mut t = tx(&pool, &s).await;
    let mut small = class(&s, "Small", mon_wed_6pm());
    small.capacity = Some(2);
    let small = batches::create(&mut t, actor(&s.owner), small)
        .await
        .unwrap();
    let id = small.summary.batch.id;
    let join = |plan_id| NewEnrollment {
        batch_id: id,
        plan_id,
        starts_on: None,
    };

    enrollments::enroll(&mut t, actor(&s.front_desk), a, join(Some(monthly.id)))
        .await
        .unwrap();
    let again = enrollments::enroll(&mut t, actor(&s.front_desk), a, join(None)).await;
    assert!(
        matches!(&again, Err(Error::Conflict(m)) if m == "Aarav is already in Small."),
        "{again:?}"
    );
    let with_pack = enrollments::enroll(&mut t, actor(&s.front_desk), b, join(Some(pack.id))).await;
    assert!(matches!(with_pack, Err(Error::Invalid(_))), "{with_pack:?}");
    enrollments::enroll(&mut t, actor(&s.front_desk), b, join(None))
        .await
        .unwrap();
    let full = enrollments::enroll(&mut t, actor(&s.front_desk), c, join(None)).await;
    assert!(
        matches!(&full, Err(Error::Conflict(m)) if m == "Small is full (2 of 2)."),
        "{full:?}"
    );

    // Instructors can't put students in classes.
    let r = enrollments::enroll(&mut t, actor(&s.teacher), c, join(None)).await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");

    // Capacity can't drop below who's already in.
    let mut shrink = edit_of(&small);
    shrink.capacity = Some(1);
    let r = batches::update(&mut t, actor(&s.owner), id, shrink).await;
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");

    // Leaving frees the place.
    let theirs = enrollments::for_student(&mut t, b).await.unwrap();
    let ended = enrollments::end(&mut t, actor(&s.front_desk), theirs[0].id)
        .await
        .unwrap();
    assert!(ended[0].ended_on.is_some());
    let twice = enrollments::end(&mut t, actor(&s.front_desk), theirs[0].id).await;
    assert!(matches!(twice, Err(Error::Conflict(_))), "{twice:?}");
    enrollments::enroll(&mut t, actor(&s.front_desk), c, join(None))
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn students_on_a_break_or_gone_arent_enrolled_and_leaving_ends_classes(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let kid = student(&pool, &s, "Aarav").await;
    let mut t = tx(&pool, &s).await;
    let hiphop = batches::create(&mut t, actor(&s.owner), class(&s, "Hip-hop", mon_wed_6pm()))
        .await
        .unwrap();
    let join = NewEnrollment {
        batch_id: hiphop.summary.batch.id,
        plan_id: None,
        starts_on: None,
    };
    enrollments::enroll(&mut t, actor(&s.owner), kid, join.clone())
        .await
        .unwrap();

    let set_status = |status| StudentEdit {
        name: "Aarav".into(),
        date_of_birth: None,
        branch_id: Some(s.north.id),
        status,
        notes: None,
    };
    // A break keeps the place.
    students::update(
        &mut t,
        actor(&s.owner),
        kid,
        set_status(StudentStatus::Paused),
    )
    .await
    .unwrap();
    assert!(enrollments::for_student(&mut t, kid).await.unwrap()[0]
        .ended_on
        .is_none());
    // Leaving the studio leaves the class.
    students::update(
        &mut t,
        actor(&s.owner),
        kid,
        set_status(StudentStatus::Left),
    )
    .await
    .unwrap();
    assert!(enrollments::for_student(&mut t, kid).await.unwrap()[0]
        .ended_on
        .is_some());
    let r = enrollments::enroll(&mut t, actor(&s.owner), kid, join).await;
    assert!(
        matches!(&r, Err(Error::Invalid(m)) if m.contains("has left")),
        "{r:?}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_last_place_goes_to_one_student_only(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let a = student(&pool, &s, "Aarav").await;
    let b = student(&pool, &s, "Bela").await;
    let mut t = tx(&pool, &s).await;
    let mut one = class(&s, "One place", mon_wed_6pm());
    one.capacity = Some(1);
    let id = batches::create(&mut t, actor(&s.owner), one)
        .await
        .unwrap()
        .summary
        .batch
        .id;
    t.commit().await.unwrap();

    let join = NewEnrollment {
        batch_id: id,
        plan_id: None,
        starts_on: None,
    };
    let mut first = tx(&pool, &s).await;
    enrollments::enroll(&mut first, actor(&s.front_desk), a, join.clone())
        .await
        .unwrap();

    let pool2 = pool.clone();
    let org_id = s.org.id;
    let desk = actor(&s.front_desk);
    let second = tokio::spawn(async move {
        let mut t = TenantTx::begin(&pool2, org_id).await.unwrap();
        let r = enrollments::enroll(&mut t, desk, b, join).await;
        if r.is_ok() {
            t.commit().await.unwrap();
        }
        r.map(|_| ())
    });
    tokio::time::sleep(StdDuration::from_millis(300)).await;
    first.commit().await.unwrap();
    let r = second.await.unwrap();
    assert!(matches!(r, Err(Error::Conflict(_))), "{r:?}");
}

// --------------------------------------------------------------- credits

#[sqlx::test(migrator = "MIGRATOR")]
async fn packs_give_credits_that_expire_and_staff_roles_differ(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let kid = student(&pool, &s, "Aarav").await;
    let pack = plan(&pool, &s, "Ten classes", PlanKind::Pack, Some(10)).await;
    let monthly = plan(&pool, &s, "Monthly", PlanKind::Monthly, None).await;
    let mut t = tx(&pool, &s).await;
    let today = t.studio_today().await.unwrap();

    // Front desk sells packs.
    let after = credits::sell(
        &mut t,
        actor(&s.front_desk),
        kid,
        SellPack { plan_id: pack.id },
    )
    .await
    .unwrap();
    assert_eq!(after.available, 10);
    assert_eq!(after.lots[0].price_paise, 250_000);
    assert_eq!(after.lots[0].plan_name.as_deref(), Some("Ten classes"));
    // Valid for 30 days including today.
    assert_eq!(after.lots[0].expires_on, Some(today + Duration::days(29)));
    assert_eq!(after.next_expiry.unwrap().classes, 10);

    let monthly_sale = credits::sell(
        &mut t,
        actor(&s.front_desk),
        kid,
        SellPack {
            plan_id: monthly.id,
        },
    )
    .await;
    assert!(
        matches!(monthly_sale, Err(Error::Invalid(_))),
        "{monthly_sale:?}"
    );

    // ...but can't give classes away or take them back.
    let gift = GiveCredits {
        classes: 2,
        expires_on: None,
        note: "Missed class on Diwali".into(),
    };
    let r = credits::give(&mut t, actor(&s.front_desk), kid, gift.clone()).await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");
    let r = credits::take_back(
        &mut t,
        actor(&s.front_desk),
        kid,
        TakeBack {
            classes: 1,
            note: "x".into(),
        },
    )
    .await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");

    // Owners give, with a reason.
    let no_reason = credits::give(
        &mut t,
        actor(&s.owner),
        kid,
        GiveCredits {
            note: "  ".into(),
            ..gift.clone()
        },
    )
    .await;
    assert!(matches!(no_reason, Err(Error::Invalid(_))), "{no_reason:?}");
    let after = credits::give(&mut t, actor(&s.owner), kid, gift)
        .await
        .unwrap();
    assert_eq!(after.available, 12);

    // Credits that expired don't count.
    sqlx::query(
        "INSERT INTO credit_lots (org_id, student_id, classes, expires_on, created_by)
         VALUES ($1, $2, 5, $3, $4)",
    )
    .bind(s.org.id)
    .bind(kid)
    .bind(today - Duration::days(1))
    .bind(s.owner.id)
    .execute(t.conn())
    .await
    .unwrap();
    let after = credits::summary(&mut t, kid).await.unwrap();
    assert_eq!(after.available, 12);
    assert!(after.lots.iter().any(|l| l.expired && l.remaining == 5));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn taking_back_uses_the_soonest_expiring_credits_first(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let kid = student(&pool, &s, "Aarav").await;
    let pack = plan(&pool, &s, "Ten classes", PlanKind::Pack, Some(10)).await;
    let mut t = tx(&pool, &s).await;
    // 2 that never expire, then 10 that expire in 30 days.
    credits::give(
        &mut t,
        actor(&s.owner),
        kid,
        GiveCredits {
            classes: 2,
            expires_on: None,
            note: "Make-up".into(),
        },
    )
    .await
    .unwrap();
    credits::sell(&mut t, actor(&s.owner), kid, SellPack { plan_id: pack.id })
        .await
        .unwrap();

    let after = credits::take_back(
        &mut t,
        actor(&s.owner),
        kid,
        TakeBack {
            classes: 3,
            note: "Sold by mistake".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(after.available, 9);
    let sold = after.lots.iter().find(|l| l.plan_id.is_some()).unwrap();
    let free = after.lots.iter().find(|l| l.plan_id.is_none()).unwrap();
    assert_eq!(
        sold.remaining, 7,
        "the pack expires first, so it goes first"
    );
    assert_eq!(free.remaining, 2);

    let too_many = credits::take_back(
        &mut t,
        actor(&s.owner),
        kid,
        TakeBack {
            classes: 10,
            note: "x".into(),
        },
    )
    .await;
    assert!(
        matches!(&too_many, Err(Error::Invalid(m)) if m == "They have 9 usable classes."),
        "{too_many:?}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_database_keeps_every_credit_lot_between_zero_and_its_size(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let kid = student(&pool, &s, "Aarav").await;
    let mut t = tx(&pool, &s).await;
    let lots = credits::give(
        &mut t,
        actor(&s.owner),
        kid,
        GiveCredits {
            classes: 3,
            expires_on: None,
            note: "Trial".into(),
        },
    )
    .await
    .unwrap();
    let lot = lots.lots[0].id;
    t.commit().await.unwrap();

    let raw_move = |change: i32, reason: &'static str| {
        let pool = pool.clone();
        let org_id = s.org.id;
        let staff_id = s.owner.id;
        async move {
            let mut t = TenantTx::begin(&pool, org_id).await.unwrap();
            let r = sqlx::query(
                "INSERT INTO credit_moves (org_id, lot_id, change, reason, created_by)
                 VALUES ($1, $2, $3, $4::credit_reason, $5)",
            )
            .bind(org_id)
            .bind(lot)
            .bind(change)
            .bind(reason)
            .bind(staff_id)
            .execute(t.conn())
            .await;
            (t, r.map(|_| ()).map_err(Error::from))
        }
    };

    // Straight past the application: more than the lot holds, or more back
    // than was ever used.
    let (_, r) = raw_move(-4, "used").await;
    assert!(
        matches!(&r, Err(Error::Invalid(m)) if m == "That's more class credits than they have."),
        "{r:?}"
    );
    let (_, r) = raw_move(1, "returned").await;
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");

    // Two spends at the same moment: the second waits for the first, then
    // sees there's nothing left.
    let (first, r) = raw_move(-3, "used").await;
    r.unwrap();
    let second = tokio::spawn(raw_move(-1, "used"));
    tokio::time::sleep(StdDuration::from_millis(300)).await;
    first.commit().await.unwrap();
    let (_, r) = second.await.unwrap();
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");

    // The ledger can't be rewritten by the app role.
    let mut t = tx(&pool, &s).await;
    let edit = sqlx::query("UPDATE credit_lots SET classes = 100")
        .execute(t.conn())
        .await;
    assert!(edit.is_err(), "lots are append-only");
    drop(t);
    let mut t = tx(&pool, &s).await;
    let wipe = sqlx::query("DELETE FROM credit_moves")
        .execute(t.conn())
        .await;
    assert!(wipe.is_err(), "moves are append-only");
}

// ------------------------------------------------------- between studios

#[sqlx::test(migrator = "MIGRATOR")]
async fn classes_plans_and_credits_never_cross_studios(pool: PgPool) {
    let s = studio(&pool, "alpha").await;
    let other = studio(&pool, "beta").await;
    let kid = student(&pool, &s, "Aarav").await;
    let their_kid = student(&pool, &other, "Zoya").await;
    let pack = plan(&pool, &s, "Ten classes", PlanKind::Pack, Some(10)).await;
    let mut t = tx(&pool, &s).await;
    let hiphop = batches::create(&mut t, actor(&s.owner), class(&s, "Hip-hop", mon_wed_6pm()))
        .await
        .unwrap();
    credits::sell(&mut t, actor(&s.owner), kid, SellPack { plan_id: pack.id })
        .await
        .unwrap();
    t.commit().await.unwrap();

    let mut theirs = tx(&pool, &other).await;
    let me = actor(&other.owner);
    assert!(batches::list(&mut theirs, Default::default())
        .await
        .unwrap()
        .is_empty());
    assert!(plans::list(&mut theirs).await.unwrap().is_empty());
    assert!(matches!(
        batches::get(&mut theirs, me, hiphop.summary.batch.id).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        credits::summary(&mut theirs, kid).await,
        Err(Error::NotFound)
    ));
    // Using our ids in their requests: refused as "not yours".
    let r = enrollments::enroll(
        &mut theirs,
        me,
        their_kid,
        NewEnrollment {
            batch_id: hiphop.summary.batch.id,
            plan_id: None,
            starts_on: None,
        },
    )
    .await;
    assert!(
        matches!(&r, Err(Error::Invalid(m)) if m == "Choose one of your classes."),
        "{r:?}"
    );
    let r = credits::sell(&mut theirs, me, their_kid, SellPack { plan_id: pack.id }).await;
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    let mut sneaky = class(&s, "Sneaky", vec![slot(5, time!(10:00), time!(11:00))]);
    sneaky.branch_id = other.north.id;
    sneaky.instructor_id = Some(s.teacher.id);
    sneaky.room_id = None;
    let r = batches::create(&mut theirs, me, sneaky).await;
    assert!(
        matches!(&r, Err(Error::Invalid(m)) if m == "Choose someone on your team."),
        "{r:?}"
    );
}
