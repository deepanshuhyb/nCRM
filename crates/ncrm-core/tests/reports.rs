//! The dashboard's aggregates against real Postgres: the numbers add up,
//! each role sees what it should, and another studio's data never counts.

use ncrm_core::access::Actor;
use ncrm_core::attendance::{self, AttendanceStatus, Cancel, DayQuery, Mark, Marks, SessionStatus};
use ncrm_core::batches::{self, NewBatch, Slot};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::enrollments::{self, NewEnrollment};
use ncrm_core::fees::{self, NewCharge, NewPayment, PaymentMethod, Void};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::reports::{self, AttendanceQuery};
use ncrm_core::staff::{self, NewStaff, Staff, StaffRole};
use ncrm_core::students::{self, NewStudent, StudentEdit, StudentStatus};
use ncrm_core::Error;
use sqlx::PgPool;
use time::macros::time;
use time::{Date, Duration};
use uuid::Uuid;

struct World {
    org: Organization,
    owner: Staff,
    desk: Staff,
    teacher: Staff,
    batch: Uuid,
    kids: Vec<Uuid>,
    today: Date,
}

fn actor(s: &Staff) -> Actor {
    Actor {
        staff_id: s.id,
        role: s.role,
    }
}

/// A class that meets every day just after midnight with three students,
/// two of whom joined 40 days ago and one today.
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
            starts_on: Some(today - Duration::days(60)),
            ends_on: None,
            slots: (1..=7)
                .map(|weekday| Slot {
                    weekday,
                    starts_at: time!(00:00),
                    ends_at: time!(00:01),
                })
                .collect(),
        },
    )
    .await
    .unwrap()
    .summary
    .batch
    .id;
    let mut kids = Vec::new();
    for (name, joined) in [
        ("Asha Kid", today - Duration::days(40)),
        ("Bela", today - Duration::days(40)),
        ("Chirag", today),
    ] {
        let id = students::create(
            &mut t,
            me,
            NewStudent {
                name: name.into(),
                date_of_birth: None,
                branch_id: None,
                notes: None,
                joined_on: Some(joined),
                contacts: vec![],
            },
        )
        .await
        .unwrap()
        .student
        .id;
        enrollments::enroll(
            &mut t,
            me,
            id,
            NewEnrollment {
                batch_id: batch,
                plan_id: None,
                starts_on: Some(joined),
            },
        )
        .await
        .unwrap();
        kids.push(id);
    }
    t.commit().await.unwrap();
    World {
        org,
        owner,
        desk,
        teacher,
        batch,
        kids,
        today,
    }
}

async fn tx(pool: &PgPool, w: &World) -> TenantTx {
    TenantTx::begin(pool, w.org.id).await.unwrap()
}

async fn mark(t: &mut TenantTx, w: &World, date: Date, list: &[(usize, AttendanceStatus)]) -> Uuid {
    let desk = actor(&w.desk);
    let day = attendance::day(
        t,
        desk,
        DayQuery {
            date: Some(date),
            branch_id: None,
        },
    )
    .await
    .unwrap();
    let id = day[0].id;
    attendance::mark(
        t,
        desk,
        id,
        Marks {
            marks: list
                .iter()
                .map(|(kid, status)| Mark {
                    student_id: w.kids[*kid],
                    status: *status,
                })
                .collect(),
        },
    )
    .await
    .unwrap();
    id
}

use AttendanceStatus::{Absent, Excused, Late, Present};

#[sqlx::test(migrator = "MIGRATOR")]
async fn attendance_is_counted_per_day_with_no_gaps(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let other = world(&pool, "beta").await;
    let mut t = tx(&pool, &w).await;
    mark(&mut t, &w, w.today, &[(0, Present), (1, Late), (2, Absent)]).await;
    mark(
        &mut t,
        &w,
        w.today - Duration::days(2),
        &[(0, Excused), (1, Present)],
    )
    .await;
    // A cancelled class doesn't count, whatever was marked before.
    let cancelled = mark(&mut t, &w, w.today - Duration::days(3), &[(0, Present)]).await;
    attendance::cancel(
        &mut t,
        actor(&w.desk),
        cancelled,
        Cancel {
            reason: "Rain".into(),
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();
    // The other studio marks too; none of it may show up in ours.
    let mut o = tx(&pool, &other).await;
    mark(&mut o, &other, other.today, &[(0, Present), (1, Present)]).await;
    o.commit().await.unwrap();

    let mut t = tx(&pool, &w).await;
    // Teachers may see it: it's counts, not names.
    let days = reports::attendance_by_day(&mut t, actor(&w.teacher), AttendanceQuery::default())
        .await
        .unwrap();
    assert_eq!(days.len(), 28, "one row per day");
    assert_eq!(days[0].date, w.today - Duration::days(27));
    let on = |back: i64| {
        let d = days
            .iter()
            .find(|d| d.date == w.today - Duration::days(back))
            .unwrap();
        (d.came, d.away)
    };
    assert_eq!(on(0), (2, 1));
    assert_eq!(on(1), (0, 0), "a day nobody marked is zeros, not missing");
    assert_eq!(on(2), (1, 1));
    assert_eq!(on(3), (0, 0), "cancelled");

    let week =
        reports::attendance_by_day(&mut t, actor(&w.owner), AttendanceQuery { days: Some(7) })
            .await
            .unwrap();
    assert_eq!(week.len(), 7);
    for bad in [0, 6, 93, -5] {
        let r = reports::attendance_by_day(
            &mut t,
            actor(&w.owner),
            AttendanceQuery { days: Some(bad) },
        )
        .await;
        assert!(matches!(r, Err(Error::Invalid(_))), "{bad}: {r:?}");
    }
}

async fn left_on(t: &mut TenantTx, id: Uuid) -> Option<Date> {
    sqlx::query_scalar("SELECT left_on FROM students WHERE id = $1")
        .bind(id)
        .fetch_one(t.conn())
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn joining_and_leaving_are_counted_by_month(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let owner = actor(&w.owner);
    let edit = |status| StudentEdit {
        name: "Bela".into(),
        date_of_birth: None,
        branch_id: None,
        status,
        notes: None,
        joined_on: w.today - Duration::days(40),
    };
    students::update(&mut t, owner, w.kids[1], edit(StudentStatus::Left))
        .await
        .unwrap();
    assert_eq!(left_on(&mut t, w.kids[1]).await, Some(w.today));
    let months = reports::students_by_month(&mut t, actor(&w.teacher))
        .await
        .unwrap();
    assert_eq!(months.len(), 6);
    let this_month = w.today.replace_day(1).unwrap();
    assert_eq!(months[5].month, this_month);
    assert_eq!(months[5].left, 1);
    let joined: i64 = months.iter().map(|m| m.joined).sum();
    assert_eq!(joined, 3, "two 40 days ago, one today");
    assert!(months[5].joined >= 1);

    // Coming back clears the date; a break isn't leaving.
    students::update(&mut t, owner, w.kids[1], edit(StudentStatus::Active))
        .await
        .unwrap();
    assert_eq!(left_on(&mut t, w.kids[1]).await, None);
    students::update(&mut t, owner, w.kids[1], edit(StudentStatus::Paused))
        .await
        .unwrap();
    assert_eq!(left_on(&mut t, w.kids[1]).await, None);
    let months = reports::students_by_month(&mut t, owner).await.unwrap();
    assert_eq!(months[5].left, 0);

    // The database keeps status and date in step even if the app forgets.
    let r = sqlx::query("UPDATE students SET status = 'left' WHERE id = $1")
        .bind(w.kids[0])
        .execute(t.conn())
        .await;
    let err = r.expect_err("left without a date");
    assert_eq!(
        err.as_database_error().and_then(|e| e.constraint()),
        Some("students_left_on_matches_status")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_fees_report_ages_dues_and_hides_collection_from_the_front_desk(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let other = world(&pool, "beta").await;
    let mut t = tx(&pool, &w).await;
    let desk = actor(&w.desk);
    let charge = |what: &str, paise, days_back| NewCharge {
        description: what.into(),
        amount_paise: paise,
        due_on: Some(w.today - Duration::days(days_back)),
    };
    fees::add_charge(&mut t, desk, w.kids[0], charge("Due today", 10_000, 0))
        .await
        .unwrap();
    fees::add_charge(&mut t, desk, w.kids[0], charge("A few days", 20_000, 3))
        .await
        .unwrap();
    fees::add_charge(&mut t, desk, w.kids[1], charge("Weeks", 30_000, 20))
        .await
        .unwrap();
    let f = fees::add_charge(&mut t, desk, w.kids[2], charge("Months", 40_000, 45))
        .await
        .unwrap();
    let months_old = f.charges[0].id;
    let pay = |paise| NewPayment {
        amount_paise: paise,
        method: PaymentMethod::Cash,
        reference: None,
        note: None,
        received_on: None,
    };
    // Kid 0 pays ₹250: the older ₹200 in full and ₹50 of today's.
    fees::record_payment(&mut t, desk, w.kids[0], pay(25_000))
        .await
        .unwrap();
    // A cancelled payment and a cancelled charge count for nothing.
    let gone = fees::record_payment(&mut t, desk, w.kids[1], pay(5_000))
        .await
        .unwrap()
        .payment
        .id;
    fees::void_payment(
        &mut t,
        actor(&w.owner),
        gone,
        Void {
            reason: "Mistake".into(),
        },
    )
    .await
    .unwrap();
    fees::void_charge(
        &mut t,
        actor(&w.owner),
        months_old,
        Void {
            reason: "Waived".into(),
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();
    let mut o = tx(&pool, &other).await;
    fees::add_charge(
        &mut o,
        actor(&other.desk),
        other.kids[0],
        charge("Theirs", 99_000, 10),
    )
    .await
    .unwrap();
    fees::record_payment(&mut o, actor(&other.desk), other.kids[0], pay(1_000))
        .await
        .unwrap();
    o.commit().await.unwrap();

    let mut t = tx(&pool, &w).await;
    let for_owner = reports::fees(&mut t, actor(&w.owner)).await.unwrap();
    let ageing: Vec<(&str, i64, i64)> = for_owner
        .ageing
        .iter()
        .map(|b| (b.key, b.due_paise, b.students))
        .collect();
    assert_eq!(
        ageing,
        [
            ("not_due", 5_000, 1),
            ("week", 0, 0),
            ("month", 30_000, 1),
            ("older", 0, 0)
        ]
    );
    let c = for_owner.collection.expect("owners see collection");
    assert_eq!(c.collected_paise, 25_000);
    assert_eq!(c.days.last().unwrap().date, w.today);
    assert_eq!(c.days.last().unwrap().collected_paise, 25_000);
    assert_eq!(c.days[0].date, for_owner.month);
    assert_eq!(c.days.len() as u8, w.today.day());

    let for_desk = reports::fees(&mut t, desk).await.unwrap();
    assert_eq!(for_desk.ageing, for_owner.ageing);
    assert!(
        for_desk.collection.is_none(),
        "the month's takings need view financials"
    );
    assert!(matches!(
        reports::fees(&mut t, actor(&w.teacher)).await,
        Err(Error::Forbidden)
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_classs_history_shows_marked_meetings_only(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let other = world(&pool, "beta").await;
    let mut t = tx(&pool, &w).await;
    for back in [9, 5, 1] {
        mark(
            &mut t,
            &w,
            w.today - Duration::days(back),
            &[(0, Present), (1, Absent)],
        )
        .await;
    }
    let off = mark(&mut t, &w, w.today - Duration::days(3), &[(0, Present)]).await;
    attendance::cancel(
        &mut t,
        actor(&w.desk),
        off,
        Cancel {
            reason: "Holiday".into(),
        },
    )
    .await
    .unwrap();
    // Looked at, never marked: not history.
    attendance::day(
        &mut t,
        actor(&w.desk),
        DayQuery {
            date: Some(w.today - Duration::days(2)),
            branch_id: None,
        },
    )
    .await
    .unwrap();

    let history = reports::class_history(&mut t, actor(&w.teacher), w.batch)
        .await
        .unwrap();
    let seen: Vec<(i64, SessionStatus, i64, i64)> = history
        .iter()
        .map(|m| ((w.today - m.on_date).whole_days(), m.status, m.came, m.away))
        .collect();
    assert_eq!(
        seen,
        [
            (9, SessionStatus::Scheduled, 1, 1),
            (5, SessionStatus::Scheduled, 1, 1),
            (3, SessionStatus::Cancelled, 0, 0),
            (1, SessionStatus::Scheduled, 1, 1),
        ],
        "oldest first"
    );

    // Someone else's class, or one that doesn't exist, is not found.
    for id in [other.batch, Uuid::new_v4()] {
        let r = reports::class_history(&mut t, actor(&w.owner), id).await;
        assert!(matches!(r, Err(Error::NotFound)), "{r:?}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_period_report_adds_up_and_shows_each_role_its_share(pool: PgPool) {
    use ncrm_core::reports::PeriodQuery;
    let w = world(&pool, "period").await;
    let other = world(&pool, "elsewhere").await;
    let today = w.today;
    let mut t = tx(&pool, &w).await;

    // Today: two came, one away. Yesterday: one came, one away.
    // The day before was cancelled.
    let today_class = mark(&mut t, &w, today, &[(0, Present), (1, Absent), (2, Late)]).await;
    mark(
        &mut t,
        &w,
        today - Duration::days(1),
        &[(0, Present), (1, Excused)],
    )
    .await;
    let cancelled = attendance::day(
        &mut t,
        actor(&w.desk),
        DayQuery {
            date: Some(today - Duration::days(2)),
            branch_id: None,
        },
    )
    .await
    .unwrap()[0]
        .id;
    attendance::cancel(
        &mut t,
        actor(&w.desk),
        cancelled,
        Cancel {
            reason: "Rain".into(),
        },
    )
    .await
    .unwrap();
    // ₹1,000 falls due today; ₹400 comes in by cash and ₹100 by UPI.
    fees::add_charge(
        &mut t,
        actor(&w.owner),
        w.kids[0],
        NewCharge {
            description: "Costume".into(),
            amount_paise: 100_000,
            due_on: None,
        },
    )
    .await
    .unwrap();
    for (amount_paise, method) in [(40_000, PaymentMethod::Cash), (10_000, PaymentMethod::Upi)] {
        fees::record_payment(
            &mut t,
            actor(&w.desk),
            w.kids[0],
            NewPayment {
                amount_paise,
                method,
                reference: None,
                note: None,
                received_on: None,
            },
        )
        .await
        .unwrap();
    }
    t.commit().await.unwrap();
    // The teacher took today's class; yesterday's was never said.
    sqlx::query("UPDATE sessions SET teacher_present = true WHERE id = $1")
        .bind(today_class)
        .execute(&pool)
        .await
        .unwrap();

    let last_three_days = || PeriodQuery {
        from: Some(today - Duration::days(2)),
        to: Some(today),
    };
    let mut t = tx(&pool, &w).await;
    let report = reports::period(&mut t, actor(&w.owner), last_three_days())
        .await
        .unwrap();
    assert_eq!((report.from, report.to), (today - Duration::days(2), today));
    assert_eq!(report.classes.len(), 1);
    let class = &report.classes[0];
    assert_eq!(class.name, "Hip-hop");
    assert_eq!(
        (class.meetings, class.cancelled, class.came, class.away),
        (2, 1, 3, 2)
    );
    // One student joined today; the other two long before the period.
    assert_eq!(
        (
            report.students.joined,
            report.students.left,
            report.students.active
        ),
        (1, 0, 3)
    );
    let teachers = report.teachers.expect("an owner sees the team");
    assert_eq!(teachers.len(), 1);
    assert_eq!(teachers[0].name, "Ritu");
    assert_eq!(
        (
            teachers[0].classes,
            teachers[0].present,
            teachers[0].absent,
            teachers[0].unmarked
        ),
        (2, 1, 0, 1)
    );
    let money = report.money.expect("an owner sees the money");
    assert_eq!(
        (money.collected_paise, money.payments, money.charged_paise),
        (50_000, 2, 100_000)
    );
    assert_eq!(money.by_method.len(), 2);
    assert_eq!(
        (money.by_method[0].method, money.by_method[0].paise),
        (PaymentMethod::Cash, 40_000)
    );
    assert_eq!(
        money
            .by_month
            .iter()
            .map(|m| m.collected_paise)
            .sum::<i64>(),
        50_000
    );

    // Yesterday alone: the cancelled day and today fall outside.
    let yesterday = reports::period(
        &mut t,
        actor(&w.owner),
        PeriodQuery {
            from: Some(today - Duration::days(1)),
            to: Some(today - Duration::days(1)),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (yesterday.classes[0].meetings, yesterday.classes[0].came),
        (1, 1)
    );
    assert_eq!(yesterday.money.unwrap().collected_paise, 0);

    // Front desk and instructors see classes and students, not money or
    // the team's record.
    for who in [actor(&w.desk), actor(&w.teacher)] {
        let theirs = reports::period(&mut t, who, last_three_days())
            .await
            .unwrap();
        assert_eq!(theirs.classes.len(), 1);
        assert!(theirs.money.is_none() && theirs.teachers.is_none());
    }

    // With no dates: this month so far.
    let default = reports::period(&mut t, actor(&w.owner), PeriodQuery::default())
        .await
        .unwrap();
    assert_eq!(
        (default.from, default.to),
        (today.replace_day(1).unwrap(), today)
    );

    // Backwards, and too long.
    for (from, to) in [
        (today, today - Duration::days(1)),
        (today - Duration::days(400), today),
    ] {
        let r = reports::period(
            &mut t,
            actor(&w.owner),
            PeriodQuery {
                from: Some(from),
                to: Some(to),
            },
        )
        .await;
        assert!(matches!(r, Err(Error::Invalid(_))));
    }
    drop(t);

    // The other studio has had no classes or money in the period: none
    // of this studio's show up in its report.
    let mut o = tx(&pool, &other).await;
    let theirs = reports::period(&mut o, actor(&other.owner), last_three_days())
        .await
        .unwrap();
    assert!(theirs.classes.is_empty());
    let money = theirs.money.unwrap();
    assert_eq!((money.collected_paise, money.charged_paise), (0, 0));
    assert!(theirs.teachers.unwrap().is_empty());
}
