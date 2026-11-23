//! Attendance against real Postgres: sessions from the timetable, marking
//! and credits, who may mark, drop-ins, cancelling, alerts.

use ncrm_core::access::Actor;
use ncrm_core::attendance::{
    self, AttendanceStatus, Cancel, DayQuery, DropIn, Mark, Marks, SessionStatus, TeacherMark,
};
use ncrm_core::batches::{self, BatchEdit, NewBatch, Slot};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::credits::{self, SellPack};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::enrollments::{self, NewEnrollment};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::plans::{self, NewPlan, PlanKind};
use ncrm_core::staff::{self, NewStaff, Staff, StaffRole};
use ncrm_core::students::{self, NewStudent, StudentEdit, StudentStatus};
use ncrm_core::Error;
use sqlx::PgPool;
use time::macros::time;
use time::{Date, Duration, Time};
use uuid::Uuid;

struct World {
    org: Organization,
    owner: Staff,
    desk: Staff,
    teacher: Staff,
    other_teacher: Staff,
    branch: Uuid,
    batch: Uuid,
    monthly: Uuid,
    pack: Uuid,
    /// On a monthly plan.
    asha: Uuid,
    /// Pays with credits and has a 2-class pack.
    bela: Uuid,
    /// Pays with credits and has none.
    chirag: Uuid,
    today: Date,
}

fn actor(s: &Staff) -> Actor {
    Actor {
        staff_id: s.id,
        role: s.role,
    }
}

fn every_day(from: Time, to: Time) -> Vec<Slot> {
    (1..=7)
        .map(|weekday| Slot {
            weekday,
            starts_at: from,
            ends_at: to,
        })
        .collect()
}

async fn new_student(t: &mut TenantTx, who: Actor, name: &str) -> Uuid {
    students::create(
        t,
        who,
        NewStudent {
            name: name.into(),
            date_of_birth: None,
            branch_id: None,
            notes: None,
            joined_on: None,
            contacts: vec![],
        },
    )
    .await
    .unwrap()
    .student
    .id
}

/// A class that meets every day just after midnight (so today's session is
/// always over), started 20 days ago, with three students.
async fn world(pool: &PgPool, slug: &str) -> World {
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
    let person = |name: &str, phone: &str, role| NewStaff {
        name: name.into(),
        phone: phone.into(),
        role,
        branch_id: None,
    };
    let owner = staff::create(&mut t, person("Asha", "+919800000001", StaffRole::Owner))
        .await
        .unwrap();
    let desk = staff::create(
        &mut t,
        person("Farah", "+919800000002", StaffRole::FrontDesk),
    )
    .await
    .unwrap();
    let teacher = staff::create(
        &mut t,
        person("Ritu", "+919800000003", StaffRole::Instructor),
    )
    .await
    .unwrap();
    let other_teacher = staff::create(
        &mut t,
        person("Kabir", "+919800000004", StaffRole::Instructor),
    )
    .await
    .unwrap();
    let me = actor(&owner);
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
            instructor_id: Some(teacher.id),
            capacity: None,
            starts_on: Some(today - Duration::days(20)),
            ends_on: None,
            slots: every_day(time!(00:00), time!(00:01)),
        },
    )
    .await
    .unwrap()
    .summary
    .batch
    .id;
    let plan = |name: &str, kind, classes| NewPlan {
        name: name.into(),
        kind,
        price_paise: 100_000,
        classes,
        valid_days: Some(60),
    };
    let monthly = plans::create(&mut t, me, plan("Monthly", PlanKind::Monthly, None))
        .await
        .unwrap()
        .id;
    let pack = plans::create(&mut t, me, plan("Two classes", PlanKind::Pack, Some(2)))
        .await
        .unwrap()
        .id;
    let asha = new_student(&mut t, me, "Asha Kid").await;
    let bela = new_student(&mut t, me, "Bela").await;
    let chirag = new_student(&mut t, me, "Chirag").await;
    for (kid, plan_id) in [(asha, Some(monthly)), (bela, None), (chirag, None)] {
        enrollments::enroll(
            &mut t,
            me,
            kid,
            NewEnrollment {
                batch_id: batch,
                plan_id,
                starts_on: Some(today - Duration::days(20)),
            },
        )
        .await
        .unwrap();
    }
    credits::sell(&mut t, me, bela, SellPack { plan_id: pack })
        .await
        .unwrap();
    t.commit().await.unwrap();
    World {
        org,
        owner,
        desk,
        teacher,
        other_teacher,
        branch,
        batch,
        monthly,
        pack,
        asha,
        bela,
        chirag,
        today,
    }
}

async fn tx(pool: &PgPool, w: &World) -> TenantTx {
    TenantTx::begin(pool, w.org.id).await.unwrap()
}

async fn session_on(t: &mut TenantTx, w: &World, date: Date) -> Uuid {
    let day = attendance::day(
        t,
        actor(&w.desk),
        DayQuery {
            date: Some(date),
            branch_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(day.len(), 1, "one class a day");
    day[0].id
}

fn marks(list: &[(Uuid, AttendanceStatus)]) -> Marks {
    Marks {
        marks: list
            .iter()
            .map(|(student_id, status)| Mark {
                student_id: *student_id,
                status: *status,
            })
            .collect(),
    }
}

async fn credits_of(t: &mut TenantTx, kid: Uuid) -> i32 {
    credits::summary(t, kid).await.unwrap().available
}

use AttendanceStatus::{Absent, Excused, Late, Present};

#[sqlx::test(migrator = "MIGRATOR")]
async fn sessions_come_from_the_timetable_for_today_and_the_past_only(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let first = session_on(&mut t, &w, w.today).await;
    let again = session_on(&mut t, &w, w.today).await;
    assert_eq!(first, again, "asking twice doesn't create it twice");
    session_on(&mut t, &w, w.today - Duration::days(3)).await;

    let tomorrow = attendance::day(
        &mut t,
        actor(&w.desk),
        DayQuery {
            date: Some(w.today + Duration::days(1)),
            branch_id: None,
        },
    )
    .await;
    assert!(matches!(tomorrow, Err(Error::Invalid(_))), "{tomorrow:?}");
    // Before the class started: nothing.
    let before = attendance::day(
        &mut t,
        actor(&w.desk),
        DayQuery {
            date: Some(w.today - Duration::days(30)),
            branch_id: None,
        },
    )
    .await
    .unwrap();
    assert!(before.is_empty());

    let s = attendance::get(&mut t, actor(&w.desk), first)
        .await
        .unwrap();
    assert_eq!(
        (s.summary.expected, s.summary.unmarked, s.summary.came),
        (3, 3, 0)
    );
    assert!(s.summary.ended);
    assert_eq!(s.summary.instructor_name.as_deref(), Some("Ritu"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn coming_to_class_spends_credits_and_corrections_give_them_back(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let id = session_on(&mut t, &w, w.today).await;
    let desk = actor(&w.desk);

    let all = marks(&[(w.asha, Present), (w.bela, Late), (w.chirag, Present)]);
    let s = attendance::mark(&mut t, desk, id, all.clone())
        .await
        .unwrap();
    assert_eq!(
        (s.summary.came, s.summary.away, s.summary.unmarked),
        (3, 0, 0)
    );
    let row = |s: &attendance::SessionDetail, kid: Uuid| {
        s.roster
            .as_ref()
            .unwrap()
            .iter()
            .find(|r| r.student_id == kid)
            .unwrap()
            .clone()
    };
    assert!(
        !row(&s, w.asha).pays_with_credits,
        "monthly plan: no credit"
    );
    assert_eq!(credits_of(&mut t, w.bela).await, 1, "one of two spent");
    assert!(row(&s, w.chirag).owes_credit, "came with no credit left");

    // Saving the same marks again spends nothing more.
    attendance::mark(&mut t, desk, id, all).await.unwrap();
    assert_eq!(credits_of(&mut t, w.bela).await, 1);

    // A correction gives the credit back; an excused absence spends none.
    let s = attendance::mark(
        &mut t,
        desk,
        id,
        marks(&[(w.bela, Absent), (w.chirag, Excused)]),
    )
    .await
    .unwrap();
    assert_eq!(credits_of(&mut t, w.bela).await, 2);
    assert!(!row(&s, w.chirag).owes_credit);
    assert_eq!((s.summary.came, s.summary.away), (1, 2));

    // Chirag comes after all, still without credits, then buys a pack:
    // the owed class is paid from it straight away.
    attendance::mark(&mut t, desk, id, marks(&[(w.chirag, Present)]))
        .await
        .unwrap();
    let after = credits::sell(&mut t, desk, w.chirag, SellPack { plan_id: w.pack })
        .await
        .unwrap();
    assert_eq!(after.available, 1, "two bought, one already owed");
    let s = attendance::get(&mut t, desk, id).await.unwrap();
    assert!(!row(&s, w.chirag).owes_credit);

    // Someone who isn't in the class can't be marked.
    let stranger = new_student(&mut t, actor(&w.owner), "Stranger").await;
    let r = attendance::mark(&mut t, desk, id, marks(&[(stranger, Present)])).await;
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    let _ = w.monthly;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn two_people_marking_the_same_class_spend_one_credit_not_two(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    // The session has to exist for both to find it.
    let mut setup = tx(&pool, &w).await;
    let id = session_on(&mut setup, &w, w.today).await;
    assert_eq!(credits_of(&mut setup, w.bela).await, 2);
    setup.commit().await.unwrap();

    // The teacher taps Save and, before that finishes, so does the front
    // desk. The second save waits for the session's row lock, then finds
    // Bela already marked and leaves her credit alone.
    let mut first = tx(&pool, &w).await;
    attendance::mark(
        &mut first,
        actor(&w.teacher),
        id,
        marks(&[(w.bela, Present)]),
    )
    .await
    .unwrap();

    let pool2 = pool.clone();
    let org_id = w.org.id;
    let desk = actor(&w.desk);
    let again = marks(&[(w.bela, Late)]);
    let second = tokio::spawn(async move {
        let mut t = TenantTx::begin(&pool2, org_id).await.unwrap();
        let r = attendance::mark(&mut t, desk, id, again).await;
        if r.is_ok() {
            t.commit().await.unwrap();
        }
        r.map(|_| ())
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!second.is_finished(), "the second save waits its turn");
    first.commit().await.unwrap();
    second.await.unwrap().unwrap();

    let mut t = tx(&pool, &w).await;
    assert_eq!(credits_of(&mut t, w.bela).await, 1, "one class, one credit");
    let s = attendance::get(&mut t, desk, id).await.unwrap();
    let bela = s
        .roster
        .unwrap()
        .into_iter()
        .find(|r| r.student_id == w.bela)
        .unwrap();
    assert_eq!(bela.status, Some(Late), "the later save's mark stands");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_the_classs_teacher_and_the_front_desk_mark_it(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let id = session_on(&mut t, &w, w.today).await;
    let one = marks(&[(w.asha, Present)]);

    let other = attendance::get(&mut t, actor(&w.other_teacher), id)
        .await
        .unwrap();
    assert!(!other.can_mark);
    assert!(
        other.roster.is_none(),
        "another teacher doesn't see the students"
    );
    assert_eq!(other.summary.expected, 3, "counts are fine");
    let r = attendance::mark(&mut t, actor(&w.other_teacher), id, one.clone()).await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");

    let own = attendance::mark(&mut t, actor(&w.teacher), id, one.clone())
        .await
        .unwrap();
    assert!(own.can_mark);

    // A teacher checks themselves in, but can't mark themselves absent or
    // name a substitute; the front desk can.
    let me = attendance::set_teacher(
        &mut t,
        actor(&w.teacher),
        id,
        TeacherMark {
            present: true,
            instructor_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(me.summary.teacher_present, Some(true));
    for bad in [
        TeacherMark {
            present: false,
            instructor_id: None,
        },
        TeacherMark {
            present: true,
            instructor_id: Some(w.other_teacher.id),
        },
    ] {
        let r = attendance::set_teacher(&mut t, actor(&w.teacher), id, bad).await;
        assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");
    }
    let sub = attendance::set_teacher(
        &mut t,
        actor(&w.desk),
        id,
        TeacherMark {
            present: true,
            instructor_id: Some(w.other_teacher.id),
        },
    )
    .await
    .unwrap();
    assert_eq!(sub.summary.instructor_name.as_deref(), Some("Kabir"));
    // The substitute can now mark this one session.
    attendance::mark(&mut t, actor(&w.other_teacher), id, one.clone())
        .await
        .unwrap();

    // Classes more than a week old are the front desk's to change.
    let old = session_on(&mut t, &w, w.today - Duration::days(10)).await;
    let r = attendance::mark(&mut t, actor(&w.teacher), old, one.clone()).await;
    assert!(
        matches!(&r, Err(Error::Invalid(m)) if m.contains("more than a week old")),
        "{r:?}"
    );
    attendance::mark(&mut t, actor(&w.desk), old, one)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn drop_ins_join_one_class_and_cancelling_gives_credits_back(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let other = world(&pool, "beta").await;
    let mut t = tx(&pool, &w).await;
    let id = session_on(&mut t, &w, w.today).await;
    let desk = actor(&w.desk);
    let visitor = new_student(&mut t, actor(&w.owner), "Visitor").await;
    credits::sell(&mut t, desk, visitor, SellPack { plan_id: w.pack })
        .await
        .unwrap();

    let s = attendance::add_drop_in(
        &mut t,
        desk,
        id,
        DropIn {
            student_id: visitor,
        },
    )
    .await
    .unwrap();
    let row = s
        .roster
        .unwrap()
        .into_iter()
        .find(|r| r.student_id == visitor)
        .unwrap();
    assert!(row.drop_in && row.status == Some(Present));
    assert_eq!(credits_of(&mut t, visitor).await, 1);
    assert_eq!(s.summary.came, 1);
    assert_eq!(s.summary.expected, 3, "a drop-in isn't an expected member");

    let twice = attendance::add_drop_in(
        &mut t,
        desk,
        id,
        DropIn {
            student_id: visitor,
        },
    )
    .await;
    assert!(matches!(twice, Err(Error::Conflict(_))), "{twice:?}");
    let member = attendance::add_drop_in(&mut t, desk, id, DropIn { student_id: w.asha }).await;
    assert!(matches!(member, Err(Error::Conflict(_))), "{member:?}");
    let foreign = attendance::add_drop_in(
        &mut t,
        desk,
        id,
        DropIn {
            student_id: other.asha,
        },
    )
    .await;
    assert!(matches!(foreign, Err(Error::Invalid(_))), "{foreign:?}");
    let not_drop_in = attendance::remove_drop_in(&mut t, desk, id, w.asha).await;
    assert!(not_drop_in.is_err());

    attendance::remove_drop_in(&mut t, desk, id, visitor)
        .await
        .unwrap();
    assert_eq!(credits_of(&mut t, visitor).await, 2, "credit back");

    // Cancel: marks cleared, credits back, no more marking until restored.
    attendance::add_drop_in(
        &mut t,
        desk,
        id,
        DropIn {
            student_id: visitor,
        },
    )
    .await
    .unwrap();
    attendance::mark(&mut t, desk, id, marks(&[(w.bela, Present)]))
        .await
        .unwrap();
    assert_eq!(credits_of(&mut t, w.bela).await, 1);
    let r = attendance::cancel(
        &mut t,
        actor(&w.teacher),
        id,
        Cancel {
            reason: "Rain".into(),
        },
    )
    .await;
    assert!(
        matches!(r, Err(Error::Forbidden)),
        "teachers don't cancel: {r:?}"
    );
    let no_reason = attendance::cancel(&mut t, desk, id, Cancel { reason: " ".into() }).await;
    assert!(matches!(no_reason, Err(Error::Invalid(_))));
    let c = attendance::cancel(
        &mut t,
        desk,
        id,
        Cancel {
            reason: "Teacher unwell".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(c.summary.status, SessionStatus::Cancelled);
    assert_eq!(c.summary.came, 0);
    assert_eq!(credits_of(&mut t, w.bela).await, 2);
    assert_eq!(credits_of(&mut t, visitor).await, 2);
    let r = attendance::mark(&mut t, desk, id, marks(&[(w.asha, Present)])).await;
    assert!(
        matches!(&r, Err(Error::Invalid(m)) if m.contains("cancelled")),
        "{r:?}"
    );
    let back = attendance::restore(&mut t, desk, id).await.unwrap();
    assert_eq!(back.summary.status, SessionStatus::Scheduled);
    attendance::mark(&mut t, desk, id, marks(&[(w.asha, Present)]))
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unmarked_classes_and_absence_streaks_raise_alerts(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let desk = actor(&w.desk);

    // Nothing marked: the last week's classes (today and 7 days back) all
    // need marking.
    let a = attendance::alerts(&mut t, desk).await.unwrap();
    assert_eq!(a.unmarked.len(), 8);
    assert!(a.streaks.is_empty());
    assert_eq!(attendance::counts(&mut t).await.unwrap(), (1, 8));

    // Asha misses three in a row; Bela misses, is excused, misses again.
    for back in [2, 1, 0] {
        let id = session_on(&mut t, &w, w.today - Duration::days(back)).await;
        let bela = if back == 1 { Excused } else { Absent };
        attendance::mark(
            &mut t,
            desk,
            id,
            marks(&[(w.asha, Absent), (w.bela, bela), (w.chirag, Present)]),
        )
        .await
        .unwrap();
    }
    let a = attendance::alerts(&mut t, desk).await.unwrap();
    assert_eq!(a.unmarked.len(), 5, "three classes fully marked");
    assert_eq!(a.streaks.len(), 1);
    assert_eq!(
        (a.streaks[0].student_name.as_str(), a.streaks[0].absences),
        ("Asha Kid", 3)
    );
    let today_id = session_on(&mut t, &w, w.today).await;
    let today_roster = attendance::get(&mut t, desk, today_id)
        .await
        .unwrap()
        .roster
        .unwrap();
    let asha = today_roster
        .iter()
        .find(|r| r.student_id == w.asha)
        .unwrap();
    assert_eq!(asha.absences_in_a_row, 2, "before today's class");

    // Teachers see alerts for their own classes only.
    assert_eq!(
        attendance::alerts(&mut t, actor(&w.teacher))
            .await
            .unwrap()
            .unmarked
            .len(),
        5
    );
    assert!(attendance::alerts(&mut t, actor(&w.other_teacher))
        .await
        .unwrap()
        .unmarked
        .is_empty());

    // A student on a break isn't expected, so doesn't keep a class "unmarked".
    let id = session_on(&mut t, &w, w.today - Duration::days(3)).await;
    attendance::mark(
        &mut t,
        desk,
        id,
        marks(&[(w.asha, Present), (w.bela, Present)]),
    )
    .await
    .unwrap();
    let before = attendance::get(&mut t, desk, id).await.unwrap().summary;
    assert_eq!((before.expected, before.unmarked), (3, 1));
    students::update(
        &mut t,
        actor(&w.owner),
        w.chirag,
        StudentEdit {
            name: "Chirag".into(),
            date_of_birth: None,
            branch_id: None,
            status: StudentStatus::Paused,
            notes: None,
            joined_on: w.today,
        },
    )
    .await
    .unwrap();
    let after = attendance::get(&mut t, desk, id).await.unwrap().summary;
    assert_eq!((after.expected, after.unmarked), (2, 0));

    let history = attendance::for_student(&mut t, w.asha).await.unwrap();
    assert_eq!((history.came, history.missed), (1, 3));
    assert_eq!(history.recent[0].status, Absent);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_schedule_change_replaces_untouched_sessions_and_keeps_marked_ones(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let desk = actor(&w.desk);
    let yesterday = session_on(&mut t, &w, w.today - Duration::days(1)).await;
    session_on(&mut t, &w, w.today).await;
    attendance::mark(&mut t, desk, yesterday, marks(&[(w.asha, Present)]))
        .await
        .unwrap();

    let detail = batches::get(&mut t, actor(&w.owner), w.batch)
        .await
        .unwrap();
    let b = &detail.summary.batch;
    batches::update(
        &mut t,
        actor(&w.owner),
        w.batch,
        BatchEdit {
            name: b.name.clone(),
            style: None,
            branch_id: w.branch,
            room_id: None,
            instructor_id: b.instructor_id,
            capacity: None,
            starts_on: b.starts_on,
            ends_on: None,
            active: true,
            slots: every_day(time!(00:02), time!(00:03)),
        },
    )
    .await
    .unwrap();

    let today = attendance::day(&mut t, desk, DayQuery::default())
        .await
        .unwrap();
    assert_eq!(today.len(), 1);
    assert_eq!(
        today[0].starts_at,
        time!(00:02),
        "today's session follows the new time"
    );
    let kept = attendance::get(&mut t, desk, yesterday).await.unwrap();
    assert_eq!(
        kept.summary.starts_at,
        time!(00:00),
        "a marked session is history"
    );
    assert_eq!(kept.summary.came, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn another_studio_sees_no_sessions(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let other = world(&pool, "beta").await;
    let mut t = tx(&pool, &w).await;
    let id = session_on(&mut t, &w, w.today).await;
    t.commit().await.unwrap();

    let mut theirs = tx(&pool, &other).await;
    let me = actor(&other.owner);
    assert!(matches!(
        attendance::get(&mut theirs, me, id).await,
        Err(Error::NotFound)
    ));
    let r = attendance::mark(&mut theirs, me, id, marks(&[(other.asha, Present)])).await;
    assert!(matches!(r, Err(Error::NotFound)), "{r:?}");
    assert!(matches!(
        attendance::for_student(&mut theirs, w.asha).await,
        Err(Error::NotFound)
    ));
    // Their own day has their own class only.
    let day = attendance::day(&mut theirs, me, DayQuery::default())
        .await
        .unwrap();
    assert_eq!(day.len(), 1);
    assert_ne!(day[0].id, id);
}
