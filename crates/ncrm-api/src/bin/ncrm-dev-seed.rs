//! Fill a development database with a demo studio you can sign in to.
//!
//!     cargo run -p ncrm-api --bin ncrm-dev-seed -- 9876543210 "Your Name"
//!
//! Creates "Rhythm House" with three branches, rooms, a team, classes,
//! prices and students, and makes the given phone number its owner. Run it
//! again with another number to add another owner. There is no sign-up
//! screen yet, so this is how a local database gets its first studio.
//!
//! It refuses to run unless `COOKIE_SECURE=0`, which only a local
//! development setup has, so it can't be pointed at production by accident.

use anyhow::{bail, Context};
use ncrm_core::access::Actor;
use ncrm_core::attendance::{self, AttendanceStatus, DayQuery, Mark, Marks};
use ncrm_core::batches::{self, NewBatch, Slot};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::credits::{self, GiveCredits, SellPack};
use ncrm_core::db::{self, TenantTx};
use ncrm_core::enrollments::{self, NewEnrollment};
use ncrm_core::fees::{self, FeeSettings, NewPayment, PaymentMethod};
use ncrm_core::orgs::{self, NewOrganization};
use ncrm_core::plans::{self, NewPlan, PlanKind};
use ncrm_core::rooms::{self, NewRoom};
use ncrm_core::staff::{self, NewStaff, StaffRole};
use ncrm_core::students::{self, NewContact, NewStudent, Relation};
use time::{Duration, Time};
use uuid::Uuid;

const SLUG: &str = "rhythm-house";

/// Name, branch, room, teacher, capacity, weekdays, from, to, students.
type DemoClass = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<i32>,
    &'static [i16],
    Time,
    Time,
    usize,
);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(phone) = args.next() else {
        bail!("usage: ncrm-dev-seed <your phone number> [your name]");
    };
    let name = args.next().unwrap_or_else(|| "Studio Owner".to_string());
    if std::env::var("COOKIE_SECURE").as_deref() != Ok("0") {
        bail!("refusing to seed: COOKIE_SECURE is not 0, so this doesn't look like a development setup");
    }
    let phone = ncrm_core::phone::normalize(&phone).context("that phone number can't be used")?;
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
    let pool = db::connect(&database_url, 5).await?;
    db::migrate(&pool).await?;

    if let Some(org) = orgs::find_by_slug(&pool, SLUG).await? {
        // The studio exists: just make sure this number can sign in.
        let mut tx = TenantTx::begin(&pool, org.id).await?;
        let already: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM staff WHERE phone = $1)")
                .bind(&phone)
                .fetch_one(tx.conn())
                .await?;
        if !already {
            staff::create(&mut tx, person(&name, &phone, StaffRole::Owner)).await?;
        }
        let owner: (Uuid, StaffRole) = sqlx::query_as(
            "SELECT id, role FROM staff WHERE role = 'owner' AND active ORDER BY created_at LIMIT 1",
        )
        .fetch_one(tx.conn())
        .await?;
        tx.commit().await?;
        println!(
            "Rhythm House already exists. {phone} can sign in as {}.",
            if already { "before" } else { "a new owner" }
        );
        let me = Actor {
            staff_id: owner.0,
            role: owner.1,
        };
        if history(&pool, org.id, me).await? {
            println!("Added four weeks of attendance and this month's payments, so the graphs have something to show.");
        }
        return Ok(());
    }

    let org = orgs::create(
        &pool,
        NewOrganization {
            name: "Rhythm House".into(),
            slug: SLUG.into(),
            timezone: None,
        },
    )
    .await?;
    let mut tx = TenantTx::begin(&pool, org.id).await?;
    let today = tx.studio_today().await?;

    let owner = staff::create(&mut tx, person(&name, &phone, StaffRole::Owner)).await?;
    let me = Actor {
        staff_id: owner.id,
        role: owner.role,
    };
    // Made-up numbers. Sign-in codes for them only ever go to this
    // machine's console.
    let mut team = std::collections::HashMap::new();
    for (i, (who, role)) in [
        ("Manoj Kapoor", StaffRole::Manager),
        ("Farah Khan", StaffRole::FrontDesk),
        ("Ritu Malhotra", StaffRole::Instructor),
        ("Kabir Sethi", StaffRole::Instructor),
        ("Meera Iyer", StaffRole::Instructor),
    ]
    .into_iter()
    .enumerate()
    {
        let s = staff::create(
            &mut tx,
            person(who, &format!("+9160000000{:02}", i + 1), role),
        )
        .await?;
        team.insert(who.split(' ').next().unwrap_or(who).to_string(), s.id);
    }

    let mut branch = std::collections::HashMap::new();
    for (n, address) in [
        ("Hauz Khas", "Aurobindo Marg"),
        ("Rajouri Garden", "Ring Road"),
        ("Noida 18", "Sector 18"),
    ] {
        let b = branches::create(
            &mut tx,
            NewBranch {
                name: n.into(),
                address: Some(address.into()),
                timezone: None,
            },
        )
        .await?;
        branch.insert(n, b.id);
    }
    let mut room = std::collections::HashMap::new();
    for (b, n, cap) in [
        ("Hauz Khas", "Studio A", 20),
        ("Hauz Khas", "Studio B", 12),
        ("Rajouri Garden", "Main hall", 25),
        ("Noida 18", "Studio 1", 15),
    ] {
        let r = rooms::create(
            &mut tx,
            me,
            branch[b],
            NewRoom {
                name: n.into(),
                capacity: Some(cap),
            },
        )
        .await?;
        room.insert(n, r.id);
    }

    let t = |h: u8, m: u8| Time::from_hms(h, m, 0).expect("valid time");
    let classes: [DemoClass; 10] = [
        (
            "Hip-hop Beginners",
            "Hauz Khas",
            "Studio A",
            "Ritu",
            None,
            &[1, 3],
            t(18, 0),
            t(19, 0),
            14,
        ),
        (
            "Hip-hop Advanced",
            "Hauz Khas",
            "Studio A",
            "Ritu",
            Some(16),
            &[1, 3],
            t(19, 0),
            t(20, 30),
            15,
        ),
        (
            "Contemporary",
            "Hauz Khas",
            "Studio A",
            "Meera",
            None,
            &[6],
            t(10, 0),
            t(11, 30),
            5,
        ),
        (
            "Bollywood Kids",
            "Hauz Khas",
            "Studio B",
            "Kabir",
            Some(12),
            &[2, 4],
            t(17, 0),
            t(18, 0),
            12,
        ),
        (
            "Kathak",
            "Hauz Khas",
            "Studio B",
            "Meera",
            Some(10),
            &[2, 4],
            t(18, 30),
            t(19, 30),
            7,
        ),
        (
            "Zumba Mornings",
            "Rajouri Garden",
            "Main hall",
            "Kabir",
            None,
            &[1, 3, 5],
            t(7, 0),
            t(8, 0),
            19,
        ),
        (
            "Bollywood Adults",
            "Rajouri Garden",
            "Main hall",
            "Kabir",
            Some(20),
            &[1],
            t(18, 0),
            t(19, 15),
            9,
        ),
        (
            "Salsa Couples",
            "Rajouri Garden",
            "Main hall",
            "Kabir",
            Some(20),
            &[5],
            t(19, 30),
            t(21, 0),
            20,
        ),
        (
            "Bharatanatyam",
            "Noida 18",
            "Studio 1",
            "Meera",
            None,
            &[7],
            t(9, 0),
            t(10, 30),
            11,
        ),
        (
            "Hip-hop Kids",
            "Noida 18",
            "Studio 1",
            "Ritu",
            None,
            &[6],
            t(16, 0),
            t(17, 0),
            3,
        ),
    ];
    let mut class_ids: Vec<(Uuid, usize)> = Vec::new();
    for (n, b, r, who, cap, days, from, to, fill) in classes {
        let created = batches::create(
            &mut tx,
            me,
            NewBatch {
                name: n.into(),
                style: n.split(' ').next().map(str::to_string),
                branch_id: branch[b],
                room_id: Some(room[r]),
                instructor_id: Some(team[who]),
                capacity: cap,
                starts_on: Some(today - Duration::days(60)),
                ends_on: None,
                slots: days
                    .iter()
                    .map(|d| Slot {
                        weekday: *d,
                        starts_at: from,
                        ends_at: to,
                    })
                    .collect(),
            },
        )
        .await?;
        class_ids.push((created.summary.batch.id, fill));
    }

    let mut plan = std::collections::HashMap::new();
    for (n, kind, price, classes, days) in [
        (
            "Monthly, twice a week",
            PlanKind::Monthly,
            250_000,
            None,
            None,
        ),
        (
            "Monthly, three times a week",
            PlanKind::Monthly,
            320_000,
            None,
            None,
        ),
        ("8 classes", PlanKind::Pack, 320_000, Some(8), Some(45)),
        ("Drop-in", PlanKind::DropIn, 50_000, None, Some(30)),
    ] {
        let p = plans::create(
            &mut tx,
            me,
            NewPlan {
                name: n.into(),
                kind,
                price_paise: price,
                classes,
                valid_days: days,
            },
        )
        .await?;
        plan.insert(n, p.id);
    }

    let names = "Aarav Sharma, Anya Sharma, Bela Gupta, Chirag Jain, Diya Nair, Esha Reddy, Farhan Ali, Gauri Joshi, \
                 Harsh Vardhan, Ira Kapoor, Jai Mehta, Kavya Pillai, Laksh Arora, Mira Desai, Neel Bose, Ojas Rao, \
                 Pari Sinha, Rhea Thomas, Sahil Khanna, Tara Menon, Uday Bhat, Vani Iyer, Yash Gill, Zara Qureshi, \
                 Aditi Das, Kabir Anand, Nikhil Seth, Pooja Verma, Rohan Dutta, Simran Kaur, Tanvi Shah, Varun Goel, \
                 Ishaan Chopra, Myra Batra";
    let mut kids = Vec::new();
    for (i, n) in names.split(", ").enumerate() {
        let first = n.split(' ').next().unwrap_or(n);
        let created = students::create(
            &mut tx,
            me,
            NewStudent {
                name: n.into(),
                date_of_birth: None,
                branch_id: Some(branch["Hauz Khas"]),
                notes: None,
                // Every fourth student joined a while ago: backdated.
                joined_on: (i % 4 == 0).then(|| today - Duration::days(200 + i as i64 * 3)),
                contacts: vec![NewContact {
                    name: format!("Parent of {first}"),
                    // Siblings (the two Sharmas) share a number.
                    phone: format!("+9160001{:05}", if i == 1 { 0 } else { i }),
                    relation: Relation::Mother,
                    is_primary: true,
                }],
            },
        )
        .await?;
        kids.push(created.student.id);
    }
    let mut offset = 0;
    for (class_id, fill) in class_ids {
        for j in 0..fill {
            enrollments::enroll(
                &mut tx,
                me,
                kids[(offset + j) % kids.len()],
                NewEnrollment {
                    batch_id: class_id,
                    plan_id: (j % 3 != 0).then(|| plan["Monthly, twice a week"]),
                    starts_on: None,
                },
            )
            .await?;
        }
        offset += 5;
    }
    credits::sell(
        &mut tx,
        me,
        kids[0],
        SellPack {
            plan_id: plan["8 classes"],
            paid: None,
        },
    )
    .await?;
    credits::sell(
        &mut tx,
        me,
        kids[1],
        SellPack {
            plan_id: plan["Drop-in"],
            paid: None,
        },
    )
    .await?;
    credits::give(
        &mut tx,
        me,
        kids[0],
        GiveCredits {
            classes: 2,
            expires_on: None,
            note: "Make-up for the Diwali holiday".into(),
        },
    )
    .await?;
    tx.commit().await?;
    history(&pool, org.id, me).await?;

    println!(
        "Created Rhythm House: 3 branches, 10 classes, {} students, four weeks of history.",
        kids.len()
    );
    println!("Sign in at the dashboard with {phone}. The code is printed in the API's output.");
    Ok(())
}

fn person(name: &str, phone: &str, role: StaffRole) -> NewStaff {
    NewStaff {
        name: name.into(),
        phone: phone.into(),
        role,
        branch_id: None,
    }
}

/// A number from 0 to 99 that is always the same for the same inputs, so
/// the demo looks the same every time it's built.
fn dice(id: Uuid, salt: i64) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ (salt as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for b in id.as_bytes() {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    ((h >> 33) % 100) as i64
}

/// Give the demo studio a past: students who joined over the last months,
/// four weeks of marked attendance, monthly fees and some payments. Without
/// it every graph on the dashboard is a single bar.
///
/// Does nothing (and says so by returning false) once any class before
/// today has been marked. The backdating below uses plain SQL where the
/// app has no "change the past" feature; that's acceptable here because
/// this binary refuses to run outside local development.
async fn history(pool: &sqlx::PgPool, org_id: Uuid, me: Actor) -> anyhow::Result<bool> {
    let mut tx = TenantTx::begin(pool, org_id).await?;
    let today = tx.studio_today().await?;
    let has_past: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sessions s JOIN attendance a ON a.session_id = s.id
                         WHERE s.on_date < $1)",
    )
    .bind(today)
    .fetch_one(tx.conn())
    .await?;
    if has_past {
        return Ok(false);
    }

    // Classes have been running for two months; students joined over the
    // last five, and have been in their classes for at least six weeks.
    sqlx::query("UPDATE batches SET starts_on = least(starts_on, $1)")
        .bind(today - Duration::days(60))
        .execute(tx.conn())
        .await?;
    sqlx::query(
        "UPDATE students
            SET joined_on = least(joined_on, $1::date - (45 + abs(hashtext(id::text)) % 110))",
    )
    .bind(today)
    .execute(tx.conn())
    .await?;
    sqlx::query("UPDATE enrollments SET starts_on = least(starts_on, $1) WHERE ended_on IS NULL")
        .bind(today - Duration::days(45))
        .execute(tx.conn())
        .await?;
    // A handful joined this month and last, so growth isn't flat.
    sqlx::query(
        "UPDATE students SET joined_on = $1::date - (abs(hashtext(id::text)) % 40)
          WHERE id IN (SELECT id FROM students ORDER BY name LIMIT 7)",
    )
    .bind(today)
    .execute(tx.conn())
    .await?;
    sqlx::query(
        "UPDATE enrollments e SET starts_on = greatest(e.starts_on, s.joined_on)
           FROM students s WHERE s.id = e.student_id AND e.ended_on IS NULL",
    )
    .execute(tx.conn())
    .await?;

    // Most students pay monthly. The few holding packs keep paying per
    // class, so credits (and "owes a class") still show up, without a third
    // of the studio owing.
    sqlx::query(
        "UPDATE enrollments e
            SET plan_id = (SELECT id FROM plans WHERE kind = 'monthly' AND active
                            ORDER BY price_paise LIMIT 1)
          WHERE e.plan_id IS NULL AND e.ended_on IS NULL
            AND NOT EXISTS (SELECT 1 FROM credit_lots l
                             WHERE l.org_id = e.org_id AND l.student_id = e.student_id)",
    )
    .execute(tx.conn())
    .await?;

    // Four weeks of classes, oldest first. Most people come; one student
    // has missed their last three, so the alert has something to say.
    let slipping: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM students WHERE status = 'active' ORDER BY name DESC LIMIT 1",
    )
    .fetch_optional(tx.conn())
    .await?;
    for back in (0..28).rev() {
        let date = today - Duration::days(back);
        let sessions = attendance::day(
            &mut tx,
            me,
            DayQuery {
                date: Some(date),
                branch_id: None,
            },
        )
        .await?;
        for s in sessions.into_iter().filter(|s| s.ended) {
            let detail = attendance::get(&mut tx, me, s.id).await?;
            let Some(roster) = detail.roster else {
                continue;
            };
            // Attendance dips a little three weeks ago and recovers.
            let dip = if (14..21).contains(&back) { 9 } else { 0 };
            let marks: Vec<Mark> = roster
                .iter()
                .filter(|r| r.status.is_none())
                .map(|r| {
                    let roll = dice(r.student_id, date.to_julian_day().into()) + dip;
                    let status = if Some(r.student_id) == slipping && back < 12 {
                        AttendanceStatus::Absent
                    } else if roll < 84 {
                        AttendanceStatus::Present
                    } else if roll < 89 {
                        AttendanceStatus::Late
                    } else if roll < 97 {
                        AttendanceStatus::Absent
                    } else {
                        AttendanceStatus::Excused
                    };
                    Mark {
                        student_id: r.student_id,
                        status,
                    }
                })
                .collect();
            if !marks.is_empty() {
                attendance::mark(&mut tx, me, s.id, Marks { marks }).await?;
            }
        }
    }

    // Monthly fees from this month, and about two in three have paid,
    // on different days.
    let month = today.replace_day(1)?;
    fees::set_settings(
        &mut tx,
        me,
        FeeSettings {
            fees_from: Some(month),
            fee_due_day: 5,
        },
    )
    .await?;
    let owing = fees::dues(
        &mut tx,
        me,
        fees::DuesQuery {
            limit: Some(200),
            ..fees::DuesQuery::default()
        },
    )
    .await?;
    let span = i64::from(today.day());
    for d in owing.items {
        let roll = dice(d.student_id, 7);
        if roll >= 68 {
            continue;
        }
        let method = match roll % 5 {
            0 | 1 => PaymentMethod::Upi,
            2 => PaymentMethod::Card,
            _ => PaymentMethod::Cash,
        };
        fees::record_payment(
            &mut tx,
            me,
            d.student_id,
            NewPayment {
                // A few pay part of it.
                amount_paise: if roll % 9 == 0 {
                    d.due_paise / 2
                } else {
                    d.due_paise
                },
                method,
                reference: None,
                note: None,
                received_on: Some(today - Duration::days(dice(d.student_id, 11) % span)),
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(true)
}
