//! Fees against real Postgres: monthly fees, payments and how they settle,
//! cancelling, pack sales, who may do what, and the rules the database
//! holds even if the application got it wrong.

use std::time::Duration as StdDuration;

use ncrm_core::access::Actor;
use ncrm_core::batches::{self, NewBatch, Slot};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::credits::{self, SellPack};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::enrollments::{self, NewEnrollment};
use ncrm_core::fees::{
    self, ChargeKind, DuesQuery, FeeSettings, NewCharge, NewPayment, PaidNow, PaymentMethod,
    PaymentsQuery, Void,
};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::plans::{self, NewPlan, PlanKind};
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
    manager: Staff,
    desk: Staff,
    teacher: Staff,
    north: Uuid,
    batch: Uuid,
    /// ₹2,000 a month.
    monthly: Uuid,
    /// 4 classes for ₹1,200.
    pack: Uuid,
    /// 2 classes, free.
    free_pack: Uuid,
    /// In the class on the monthly plan.
    asha: Uuid,
    /// In the class, paying with credits.
    bela: Uuid,
    /// In the class on the monthly plan, but paused.
    chirag: Uuid,
    today: Date,
    month: Date,
}

fn actor(s: &Staff) -> Actor {
    Actor {
        staff_id: s.id,
        role: s.role,
    }
}

async fn new_student(t: &mut TenantTx, who: Actor, name: &str, branch: Option<Uuid>) -> Uuid {
    students::create(
        t,
        who,
        NewStudent {
            name: name.into(),
            date_of_birth: None,
            branch_id: branch,
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
    let month = today.replace_day(1).unwrap();
    let person = |name: &str, phone: &str, role| NewStaff {
        name: name.into(),
        phone: phone.into(),
        role,
        branch_id: None,
    };
    let owner = staff::create(&mut t, person("Asha", "+919800000001", StaffRole::Owner))
        .await
        .unwrap();
    let manager = staff::create(&mut t, person("Manoj", "+919800000005", StaffRole::Manager))
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
    let north = branches::create(
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
            branch_id: north,
            room_id: None,
            instructor_id: Some(teacher.id),
            capacity: None,
            starts_on: Some(today - Duration::days(60)),
            ends_on: None,
            slots: vec![Slot {
                weekday: 1,
                starts_at: time!(18:00),
                ends_at: time!(19:00),
            }],
        },
    )
    .await
    .unwrap()
    .summary
    .batch
    .id;
    let plan = |name: &str, kind, price_paise, classes| NewPlan {
        name: name.into(),
        kind,
        price_paise,
        classes,
        valid_days: Some(60),
    };
    let monthly = plans::create(
        &mut t,
        me,
        plan("Monthly", PlanKind::Monthly, 200_000, None),
    )
    .await
    .unwrap()
    .id;
    let pack = plans::create(
        &mut t,
        me,
        plan("Four classes", PlanKind::Pack, 120_000, Some(4)),
    )
    .await
    .unwrap()
    .id;
    let free_pack = plans::create(&mut t, me, plan("Trial pair", PlanKind::Pack, 0, Some(2)))
        .await
        .unwrap()
        .id;
    let asha = new_student(&mut t, me, "Asha Kid", Some(north)).await;
    let bela = new_student(&mut t, me, "Bela", None).await;
    let chirag = new_student(&mut t, me, "Chirag", Some(north)).await;
    for (kid, plan_id) in [(asha, Some(monthly)), (bela, None), (chirag, Some(monthly))] {
        enrollments::enroll(
            &mut t,
            me,
            kid,
            NewEnrollment {
                batch_id: batch,
                plan_id,
                starts_on: Some(today - Duration::days(40)),
            },
        )
        .await
        .unwrap();
    }
    students::update(
        &mut t,
        me,
        chirag,
        StudentEdit {
            name: "Chirag".into(),
            date_of_birth: None,
            branch_id: Some(north),
            status: StudentStatus::Paused,
            notes: None,
            joined_on: today,
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();
    World {
        org,
        owner,
        manager,
        desk,
        teacher,
        north,
        batch,
        monthly,
        pack,
        free_pack,
        asha,
        bela,
        chirag,
        today,
        month,
    }
}

async fn tx(pool: &PgPool, w: &World) -> TenantTx {
    TenantTx::begin(pool, w.org.id).await.unwrap()
}

fn charge(description: &str, amount_paise: i64, due_on: Date) -> NewCharge {
    NewCharge {
        description: description.into(),
        amount_paise,
        due_on: Some(due_on),
    }
}

fn pay(amount_paise: i64, method: PaymentMethod) -> NewPayment {
    NewPayment {
        amount_paise,
        method,
        reference: None,
        note: None,
        received_on: None,
    }
}

fn because(reason: &str) -> Void {
    Void {
        reason: reason.into(),
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn monthly_fees_wait_until_switched_on_and_come_once_a_month(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let owner = actor(&w.owner);

    // Nothing is charged until the studio says so.
    let before = fees::summary(&mut t, owner).await.unwrap();
    assert_eq!(
        (before.fees_from, before.due_paise, before.students_owing),
        (None, 0, 0)
    );

    // Front desk and teachers don't decide when fees start.
    let on = FeeSettings {
        fees_from: Some(w.month),
        fee_due_day: 5,
    };
    for who in [&w.desk, &w.teacher] {
        let r = fees::set_settings(&mut t, actor(who), on.clone()).await;
        assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");
    }
    for bad in [
        FeeSettings {
            fees_from: Some(w.month),
            fee_due_day: 31,
        },
        FeeSettings {
            fees_from: Some(w.month + Duration::days(2)),
            fee_due_day: 5,
        },
        FeeSettings {
            fees_from: Some((w.month - Duration::days(1)).replace_day(1).unwrap()),
            fee_due_day: 5,
        },
    ] {
        let r = fees::set_settings(&mut t, owner, bad).await;
        assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    }

    let after = fees::set_settings(&mut t, owner, on).await.unwrap();
    // Asha only: Bela pays with credits, Chirag is on a break.
    assert_eq!((after.due_paise, after.students_owing), (200_000, 1));
    let asha = fees::for_student(&mut t, owner, w.asha).await.unwrap();
    assert_eq!(asha.charges.len(), 1);
    let fee = &asha.charges[0];
    assert_eq!(fee.kind, ChargeKind::Monthly);
    assert_eq!(fee.period_start, Some(w.month));
    assert_eq!(fee.due_on, w.month + Duration::days(4));
    assert!(
        fee.description.starts_with("Hip-hop, "),
        "{}",
        fee.description
    );
    assert!(fees::for_student(&mut t, owner, w.chirag)
        .await
        .unwrap()
        .charges
        .is_empty());

    // Looking again creates nothing new.
    assert_eq!(fees::ensure_current_month(&mut t).await.unwrap(), 0);
    assert_eq!(
        fees::summary(&mut t, owner).await.unwrap().due_paise,
        200_000
    );

    // A waived fee stays waived.
    fees::void_charge(&mut t, owner, fee.id, because("Sibling discount"))
        .await
        .unwrap();
    assert_eq!(fees::ensure_current_month(&mut t).await.unwrap(), 0);
    assert_eq!(fees::summary(&mut t, owner).await.unwrap().due_paise, 0);

    // Someone who joins on a monthly plan owes this month straight away,
    // due on the later of the due day and the day they start.
    let dev = new_student(&mut t, owner, "Dev", None).await;
    enrollments::enroll(
        &mut t,
        owner,
        dev,
        NewEnrollment {
            batch_id: w.batch,
            plan_id: Some(w.monthly),
            starts_on: Some(w.today),
        },
    )
    .await
    .unwrap();
    let dev_fees = fees::for_student(&mut t, owner, dev).await.unwrap();
    assert_eq!(dev_fees.due_paise, 200_000);
    assert_eq!(
        dev_fees.charges[0].due_on,
        w.today.max(w.month + Duration::days(4))
    );

    // Coming back from a break: charged the next time anyone looks.
    students::update(
        &mut t,
        owner,
        w.chirag,
        StudentEdit {
            name: "Chirag".into(),
            date_of_birth: None,
            branch_id: Some(w.north),
            status: StudentStatus::Active,
            notes: None,
            joined_on: w.today,
        },
    )
    .await
    .unwrap();
    assert_eq!(fees::ensure_current_month(&mut t).await.unwrap(), 1);

    // Switching off stops new fees and leaves existing ones alone.
    let off = fees::set_settings(
        &mut t,
        owner,
        FeeSettings {
            fees_from: None,
            fee_due_day: 10,
        },
    )
    .await
    .unwrap();
    assert_eq!((off.fees_from, off.fee_due_day), (None, 10));
    assert_eq!(off.due_paise, 400_000);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn payments_settle_the_oldest_dues_first_and_keep_the_rest_as_advance(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let desk = actor(&w.desk);

    fees::add_charge(
        &mut t,
        desk,
        w.bela,
        charge("Costume", 50_000, w.today - Duration::days(10)),
    )
    .await
    .unwrap();
    let f = fees::add_charge(&mut t, desk, w.bela, charge("Workshop", 30_000, w.today))
        .await
        .unwrap();
    assert_eq!(
        (f.due_paise, f.overdue_paise, f.advance_paise),
        (80_000, 50_000, 0)
    );

    // ₹600 against ₹500 overdue and ₹300 due today.
    let first = fees::record_payment(&mut t, desk, w.bela, pay(60_000, PaymentMethod::Cash))
        .await
        .unwrap();
    assert_eq!(first.payment.receipt_no, 1);
    assert_eq!(first.payment.received_by_name, "Farah");
    assert_eq!(first.payment.unallocated_paise, 0);
    let lines: Vec<(&str, i64)> = first
        .lines
        .iter()
        .map(|l| (l.description.as_str(), l.amount_paise))
        .collect();
    assert_eq!(lines, [("Costume", 50_000), ("Workshop", 10_000)]);
    assert_eq!(first.still_due_paise, 20_000);

    // ₹500 more: ₹200 finishes the workshop, ₹300 is kept as advance.
    let mut second_payment = pay(50_000, PaymentMethod::Upi);
    second_payment.reference = Some("  UPI 4521  ".into());
    second_payment.received_on = Some(w.today - Duration::days(1));
    let second = fees::record_payment(&mut t, desk, w.bela, second_payment)
        .await
        .unwrap();
    assert_eq!(second.payment.receipt_no, 2);
    assert_eq!(second.payment.reference.as_deref(), Some("UPI 4521"));
    assert_eq!(second.payment.unallocated_paise, 30_000);
    assert_eq!(second.still_due_paise, 0);
    let f = fees::for_student(&mut t, desk, w.bela).await.unwrap();
    assert_eq!(
        (f.due_paise, f.overdue_paise, f.advance_paise),
        (0, 0, 30_000)
    );

    // The advance pays towards the next charge by itself.
    let f = fees::add_charge(
        &mut t,
        desk,
        w.bela,
        charge("Annual show", 100_000, w.today),
    )
    .await
    .unwrap();
    assert_eq!((f.due_paise, f.advance_paise), (70_000, 0));
    let show = f.charges.iter().find(|c| c.description == "Annual show");
    assert_eq!(show.unwrap().paid_paise, 30_000);

    // Input the API would otherwise have to trust.
    let mut future = pay(1_000, PaymentMethod::Cash);
    future.received_on = Some(w.today + Duration::days(1));
    let mut ancient = pay(1_000, PaymentMethod::Cash);
    ancient.received_on = Some(w.today - Duration::days(61));
    let mut long_ref = pay(1_000, PaymentMethod::Cash);
    long_ref.reference = Some("x".repeat(81));
    for bad in [
        pay(0, PaymentMethod::Cash),
        pay(-5, PaymentMethod::Cash),
        pay(fees::MAX_AMOUNT_PAISE + 1, PaymentMethod::Cash),
        future,
        ancient,
        long_ref,
    ] {
        let r = fees::record_payment(&mut t, desk, w.bela, bad).await;
        assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    }
    for bad in [
        charge("  ", 1_000, w.today),
        charge("Costume", 0, w.today),
        charge("Costume", 1_000, w.today + Duration::days(400)),
    ] {
        let r = fees::add_charge(&mut t, desk, w.bela, bad).await;
        assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    }
    // None of those took a receipt number.
    let third = fees::record_payment(&mut t, desk, w.bela, pay(1_000, PaymentMethod::Cash))
        .await
        .unwrap();
    assert_eq!(third.payment.receipt_no, 3);

    // Teachers don't handle money.
    let teacher = actor(&w.teacher);
    assert!(matches!(
        fees::record_payment(&mut t, teacher, w.bela, pay(1_000, PaymentMethod::Cash)).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        fees::for_student(&mut t, teacher, w.bela).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        fees::add_charge(&mut t, teacher, w.bela, charge("Costume", 1_000, w.today)).await,
        Err(Error::Forbidden)
    ));
    // A student who doesn't exist.
    assert!(matches!(
        fees::record_payment(
            &mut t,
            desk,
            Uuid::new_v4(),
            pay(1_000, PaymentMethod::Cash)
        )
        .await,
        Err(Error::NotFound)
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_owners_and_managers_cancel_and_the_money_moves_on(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let desk = actor(&w.desk);
    let manager = actor(&w.manager);

    let f = fees::add_charge(
        &mut t,
        desk,
        w.bela,
        charge("Costume", 50_000, w.today - Duration::days(3)),
    )
    .await
    .unwrap();
    let costume = f.charges[0].id;
    let f = fees::add_charge(&mut t, desk, w.bela, charge("Workshop", 30_000, w.today))
        .await
        .unwrap();
    let workshop = f
        .charges
        .iter()
        .find(|c| c.description == "Workshop")
        .unwrap()
        .id;
    let p1 = fees::record_payment(&mut t, desk, w.bela, pay(50_000, PaymentMethod::Cash))
        .await
        .unwrap()
        .payment;
    let p2 = fees::record_payment(&mut t, desk, w.bela, pay(40_000, PaymentMethod::Upi))
        .await
        .unwrap()
        .payment;
    // 500 → costume; 400 → workshop 300, advance 100.
    assert_eq!((p1.unallocated_paise, p2.unallocated_paise), (0, 10_000));

    // Whoever takes the cash can't make the record of it disappear.
    assert!(matches!(
        fees::void_payment(&mut t, desk, p1.id, because("Oops")).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        fees::void_charge(&mut t, desk, costume, because("Oops")).await,
        Err(Error::Forbidden)
    ));
    // A reason is part of the record.
    assert!(matches!(
        fees::void_payment(&mut t, manager, p1.id, because("  ")).await,
        Err(Error::Invalid(_))
    ));

    // Cancel the first payment. What the second payment already settled
    // stays settled (its receipt still says "Workshop ₹300"); only its
    // spare ₹100 moves, to the costume, which now owes ₹400.
    let f = fees::void_payment(&mut t, manager, p1.id, because("Entered twice"))
        .await
        .unwrap();
    assert_eq!(
        (f.due_paise, f.overdue_paise, f.advance_paise),
        (40_000, 40_000, 0)
    );
    let paid = |f: &fees::StudentFees, id: Uuid| {
        f.charges
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.paid_paise)
            .unwrap()
    };
    assert_eq!((paid(&f, costume), paid(&f, workshop)), (10_000, 30_000));
    let voided = f.payments.iter().find(|p| p.id == p1.id).unwrap();
    assert!(voided.voided_at.is_some());
    assert_eq!(voided.void_reason.as_deref(), Some("Entered twice"));
    assert_eq!(voided.receipt_no, 1, "the number stays with it");
    assert!(matches!(
        fees::void_payment(&mut t, manager, p1.id, because("Again")).await,
        Err(Error::Conflict(_))
    ));

    // Cancel the costume: the ₹100 that paid towards it has nowhere to go
    // (the workshop is paid), so it's kept as advance.
    let f = fees::void_charge(&mut t, manager, costume, because("Returned unused"))
        .await
        .unwrap();
    assert_eq!((f.due_paise, f.advance_paise), (0, 10_000));
    assert_eq!(paid(&f, workshop), 30_000);
    assert_eq!(paid(&f, costume), 0, "a cancelled charge holds no money");
    assert!(matches!(
        fees::void_charge(&mut t, manager, costume, because("Again")).await,
        Err(Error::Conflict(_))
    ));

    // Receipt numbers carry on; the cancelled one isn't reused.
    let next = fees::record_payment(&mut t, desk, w.bela, pay(1_000, PaymentMethod::Cash))
        .await
        .unwrap();
    assert_eq!(next.payment.receipt_no, 3);

    // The day's totals leave the cancelled payment out.
    let day = fees::payments(&mut t, manager, PaymentsQuery::default())
        .await
        .unwrap();
    assert_eq!(day.total, 3, "all three are listed");
    assert_eq!(day.collected_paise, 41_000);
    let cash = day
        .by_method
        .iter()
        .find(|m| m.method == PaymentMethod::Cash)
        .unwrap();
    assert_eq!((cash.amount_paise, cash.payments), (1_000, 1));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn selling_a_pack_charges_for_it_and_paying_on_the_spot_settles_it(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let desk = actor(&w.desk);

    // Sold, to be paid later.
    credits::sell(
        &mut t,
        desk,
        w.bela,
        SellPack {
            plan_id: w.pack,
            paid: None,
        },
    )
    .await
    .unwrap();
    let f = fees::for_student(&mut t, desk, w.bela).await.unwrap();
    assert_eq!(f.due_paise, 120_000);
    assert_eq!(
        (f.charges[0].kind, f.charges[0].description.as_str()),
        (ChargeKind::Pack, "Four classes")
    );

    // Something older is also owed.
    fees::add_charge(
        &mut t,
        desk,
        w.bela,
        charge("Costume", 30_000, w.today - Duration::days(5)),
    )
    .await
    .unwrap();

    // Sold and paid by UPI: one sale, one receipt. The money was handed
    // over for this pack, so it pays this pack, not the older dues.
    let c = credits::sell(
        &mut t,
        desk,
        w.bela,
        SellPack {
            plan_id: w.pack,
            paid: Some(PaidNow {
                method: PaymentMethod::Upi,
                reference: Some("UPI 77".into()),
            }),
        },
    )
    .await
    .unwrap();
    assert_eq!(c.available, 8);
    let f = fees::for_student(&mut t, desk, w.bela).await.unwrap();
    assert_eq!((f.due_paise, f.payments.len()), (150_000, 1));
    assert_eq!(f.payments[0].amount_paise, 120_000);
    assert_eq!(f.payments[0].method, PaymentMethod::Upi);
    assert_eq!(f.payments[0].unallocated_paise, 0);
    let paid = |what: &str| -> Vec<i64> {
        let mut v: Vec<i64> = f
            .charges
            .iter()
            .filter(|c| c.description == what)
            .map(|c| c.paid_paise)
            .collect();
        v.sort_unstable();
        v
    };
    assert_eq!(paid("Costume"), [0], "older dues wait");
    assert_eq!(paid("Four classes"), [0, 120_000], "one pack paid in full");

    // A free pack is not a debt.
    credits::sell(
        &mut t,
        desk,
        w.bela,
        SellPack {
            plan_id: w.free_pack,
            paid: Some(PaidNow {
                method: PaymentMethod::Cash,
                reference: None,
            }),
        },
    )
    .await
    .unwrap();
    let f = fees::for_student(&mut t, desk, w.bela).await.unwrap();
    assert_eq!((f.charges.len(), f.payments.len()), (3, 1));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_dues_list_and_reports_show_each_role_what_it_may_see(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let mut t = tx(&pool, &w).await;
    let desk = actor(&w.desk);
    let owner = actor(&w.owner);

    fees::add_charge(&mut t, desk, w.asha, charge("Costume", 90_000, w.today))
        .await
        .unwrap();
    fees::add_charge(
        &mut t,
        desk,
        w.bela,
        charge("Old balance", 20_000, w.today - Duration::days(30)),
    )
    .await
    .unwrap();
    fees::add_charge(&mut t, desk, w.chirag, charge("Shoes", 10_000, w.today))
        .await
        .unwrap();
    fees::record_payment(&mut t, desk, w.chirag, pay(10_000, PaymentMethod::Cash))
        .await
        .unwrap();

    // Overdue first; Chirag has paid and isn't on the list.
    let dues = fees::dues(&mut t, desk, DuesQuery::default())
        .await
        .unwrap();
    let names: Vec<(&str, i64, i64)> = dues
        .items
        .iter()
        .map(|d| (d.student_name.as_str(), d.due_paise, d.overdue_paise))
        .collect();
    assert_eq!(names, [("Bela", 20_000, 20_000), ("Asha Kid", 90_000, 0)]);
    assert_eq!(dues.total, 2);
    assert_eq!(dues.items[0].oldest_due_on, w.today - Duration::days(30));
    let north = fees::dues(
        &mut t,
        desk,
        DuesQuery {
            branch_id: Some(w.north),
            ..DuesQuery::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(north.total, 1, "Bela has no branch");
    assert_eq!(north.items[0].branch_name.as_deref(), Some("North"));

    // The front desk sees what's owed, not what the studio made this month.
    let for_desk = fees::summary(&mut t, desk).await.unwrap();
    assert_eq!(
        (
            for_desk.due_paise,
            for_desk.overdue_paise,
            for_desk.students_owing
        ),
        (110_000, 20_000, 2)
    );
    assert_eq!(for_desk.collected_this_month_paise, None);
    let for_owner = fees::summary(&mut t, owner).await.unwrap();
    assert_eq!(for_owner.collected_this_month_paise, Some(10_000));

    // ...and a week of payments, enough for the cash tally.
    let old = PaymentsQuery {
        from: Some(w.today - Duration::days(8)),
        to: Some(w.today),
        ..PaymentsQuery::default()
    };
    assert!(matches!(
        fees::payments(&mut t, desk, old.clone()).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(fees::payments(&mut t, owner, old).await.unwrap().total, 1);
    let week = PaymentsQuery {
        from: Some(w.today - Duration::days(7)),
        to: Some(w.today),
        ..PaymentsQuery::default()
    };
    assert_eq!(fees::payments(&mut t, desk, week).await.unwrap().total, 1);
    let backwards = PaymentsQuery {
        from: Some(w.today),
        to: Some(w.today - Duration::days(1)),
        ..PaymentsQuery::default()
    };
    assert!(matches!(
        fees::payments(&mut t, owner, backwards).await,
        Err(Error::Invalid(_))
    ));

    for who in [&w.teacher] {
        let a = actor(who);
        assert!(matches!(
            fees::dues(&mut t, a, DuesQuery::default()).await,
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            fees::summary(&mut t, a).await,
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            fees::payments(&mut t, a, PaymentsQuery::default()).await,
            Err(Error::Forbidden)
        ));
    }
}

/// The app role, inside the right studio, trying things the application
/// never does. Each attempt gets its own transaction because a failed
/// statement ends a Postgres transaction.
#[sqlx::test(migrator = "MIGRATOR")]
async fn the_database_itself_refuses_to_bend_the_books(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let desk = actor(&w.desk);
    let owner = actor(&w.owner);
    let mut t = tx(&pool, &w).await;
    let f = fees::add_charge(&mut t, desk, w.bela, charge("Costume", 50_000, w.today))
        .await
        .unwrap();
    let costume = f.charges[0].id;
    let f = fees::add_charge(&mut t, desk, w.asha, charge("Shoes", 70_000, w.today))
        .await
        .unwrap();
    let ashas_shoes = f.charges[0].id;
    let bela_paid = fees::record_payment(&mut t, desk, w.bela, pay(20_000, PaymentMethod::Cash))
        .await
        .unwrap()
        .payment
        .id;
    // A second payment, bigger than what's left, so some is unallocated.
    let advance = fees::record_payment(&mut t, desk, w.bela, pay(90_000, PaymentMethod::Cash))
        .await
        .unwrap()
        .payment
        .id;
    let cancelled = fees::record_payment(&mut t, desk, w.asha, pay(5_000, PaymentMethod::Cash))
        .await
        .unwrap()
        .payment
        .id;
    fees::void_payment(&mut t, owner, cancelled, because("Mistake"))
        .await
        .unwrap();
    t.commit().await.unwrap();

    let attempts: Vec<(&str, String, &str)> = vec![
        (
            "change an amount",
            format!("UPDATE charges SET amount_paise = 1 WHERE id = '{costume}'"),
            "permission denied",
        ),
        (
            "change a payment",
            format!("UPDATE payments SET amount_paise = 1 WHERE id = '{bela_paid}'"),
            "permission denied",
        ),
        (
            "delete a payment",
            format!("DELETE FROM payments WHERE id = '{bela_paid}'"),
            "permission denied",
        ),
        (
            "delete a charge",
            format!("DELETE FROM charges WHERE id = '{costume}'"),
            "permission denied",
        ),
        (
            "edit an allocation",
            "UPDATE payment_allocations SET amount_paise = 1".to_string(),
            "permission denied",
        ),
        (
            "un-cancel a payment",
            format!(
                "UPDATE payments SET voided_at = NULL, voided_by = NULL, void_reason = NULL
                  WHERE id = '{cancelled}'"
            ),
            "fees_void_is_final",
        ),
        (
            "pay a charge twice over",
            format!(
                "INSERT INTO payment_allocations (org_id, student_id, payment_id, charge_id, amount_paise)
                 VALUES ('{org}', '{bela}', '{advance}', '{costume}', 100)",
                org = w.org.id,
                bela = w.bela
            ),
            "payment_allocations_bounds",
        ),
        (
            "spend more than the payment",
            format!(
                "INSERT INTO payment_allocations (org_id, student_id, payment_id, charge_id, amount_paise)
                 VALUES ('{org}', '{bela}', '{bela_paid}', '{costume}', 1)",
                org = w.org.id,
                bela = w.bela
            ),
            "payment_allocations_bounds",
        ),
        (
            "use one student's money for another",
            format!(
                "INSERT INTO payment_allocations (org_id, student_id, payment_id, charge_id, amount_paise)
                 VALUES ('{org}', '{bela}', '{advance}', '{ashas_shoes}', 100)",
                org = w.org.id,
                bela = w.bela
            ),
            "foreign key",
        ),
        (
            "allocate a cancelled payment",
            format!(
                "INSERT INTO payment_allocations (org_id, student_id, payment_id, charge_id, amount_paise)
                 VALUES ('{org}', '{asha}', '{cancelled}', '{ashas_shoes}', 100)",
                org = w.org.id,
                asha = w.asha
            ),
            "payment_allocations_voided",
        ),
        (
            "a charge of nothing",
            format!(
                "INSERT INTO charges (org_id, student_id, kind, description, amount_paise, due_on)
                 VALUES ('{org}', '{bela}', 'other', 'Free', 0, current_date)",
                org = w.org.id,
                bela = w.bela
            ),
            "check constraint",
        ),
        (
            "a second fee for the same class and month",
            // Not possible to fake without an existing fee; covered by
            // charges_one_per_month in the monthly test. Here: a monthly
            // charge with no class, which the shape check refuses.
            format!(
                "INSERT INTO charges (org_id, student_id, kind, description, amount_paise, due_on)
                 VALUES ('{org}', '{bela}', 'monthly', 'Fee', 100, current_date)",
                org = w.org.id,
                bela = w.bela
            ),
            "charges_monthly_shape",
        ),
    ];
    for (what, sql, expected) in attempts {
        let mut t = tx(&pool, &w).await;
        let r = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(t.conn())
            .await;
        let err = r.expect_err(what);
        // The message, plus the constraint's name when Postgres gives one.
        let constraint = err
            .as_database_error()
            .and_then(|e| e.constraint())
            .unwrap_or_default()
            .to_string();
        let seen = format!("{err} [{constraint}]");
        assert!(seen.contains(expected), "{what}: {seen}");
    }

    // After all that, the books are as they were.
    let mut t = tx(&pool, &w).await;
    let bela = fees::for_student(&mut t, desk, w.bela).await.unwrap();
    assert_eq!((bela.due_paise, bela.advance_paise), (0, 60_000));
    let asha = fees::for_student(&mut t, desk, w.asha).await.unwrap();
    assert_eq!((asha.due_paise, asha.advance_paise), (70_000, 0));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn two_payments_at_once_get_their_own_receipt_numbers(pool: PgPool) {
    let w = world(&pool, "alpha").await;
    let desk = actor(&w.desk);
    let mut setup = tx(&pool, &w).await;
    fees::add_charge(&mut setup, desk, w.bela, charge("Costume", 50_000, w.today))
        .await
        .unwrap();
    setup.commit().await.unwrap();

    // Two front-desk screens take ₹400 each for the same ₹500 charge.
    let mut first = tx(&pool, &w).await;
    let a = fees::record_payment(&mut first, desk, w.bela, pay(40_000, PaymentMethod::Cash))
        .await
        .unwrap();

    let pool2 = pool.clone();
    let org_id = w.org.id;
    let bela = w.bela;
    let second = tokio::spawn(async move {
        let mut t = TenantTx::begin(&pool2, org_id).await.unwrap();
        let r = fees::record_payment(&mut t, desk, bela, pay(40_000, PaymentMethod::Upi)).await;
        if r.is_ok() {
            t.commit().await.unwrap();
        }
        r
    });
    tokio::time::sleep(StdDuration::from_millis(300)).await;
    assert!(!second.is_finished(), "the second payment waits its turn");
    first.commit().await.unwrap();
    let b = second.await.unwrap().unwrap();

    assert_eq!((a.payment.receipt_no, b.payment.receipt_no), (1, 2));
    // The second saw the first: ₹100 finished the charge, ₹300 is advance.
    assert_eq!(b.payment.unallocated_paise, 30_000);
    let mut t = tx(&pool, &w).await;
    let f = fees::for_student(&mut t, desk, w.bela).await.unwrap();
    assert_eq!((f.due_paise, f.advance_paise), (0, 30_000));
    assert_eq!(f.charges[0].paid_paise, 50_000, "paid once, not twice");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn another_studio_sees_no_fees(pool: PgPool) {
    let alpha = world(&pool, "alpha").await;
    let beta = world(&pool, "beta").await;
    let mut a = tx(&pool, &alpha).await;
    let f = fees::add_charge(
        &mut a,
        actor(&alpha.desk),
        alpha.bela,
        charge("Costume", 50_000, alpha.today),
    )
    .await
    .unwrap();
    let charge_id = f.charges[0].id;
    let payment_id = fees::record_payment(
        &mut a,
        actor(&alpha.desk),
        alpha.bela,
        pay(10_000, PaymentMethod::Cash),
    )
    .await
    .unwrap()
    .payment
    .id;
    a.commit().await.unwrap();

    let mut b = tx(&pool, &beta).await;
    let owner = actor(&beta.owner);
    assert!(matches!(
        fees::for_student(&mut b, owner, alpha.bela).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        fees::receipt(&mut b, owner, payment_id).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        fees::void_payment(&mut b, owner, payment_id, because("Not ours")).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        fees::void_charge(&mut b, owner, charge_id, because("Not ours")).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        fees::record_payment(&mut b, owner, alpha.bela, pay(1_000, PaymentMethod::Cash)).await,
        Err(Error::NotFound)
    ));
    assert!(fees::dues(&mut b, owner, DuesQuery::default())
        .await
        .unwrap()
        .items
        .is_empty());
    let s = fees::summary(&mut b, owner).await.unwrap();
    assert_eq!((s.due_paise, s.collected_this_month_paise), (0, Some(0)));
    // Beta's own receipts start at 1, whatever alpha has issued.
    let first = fees::record_payment(&mut b, owner, beta.bela, pay(1_000, PaymentMethod::Cash))
        .await
        .unwrap();
    assert_eq!(first.payment.receipt_no, 1);
    let _ = (alpha.pack, alpha.chirag, beta.free_pack);
}
