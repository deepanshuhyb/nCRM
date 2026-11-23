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
use ncrm_core::batches::{self, NewBatch, Slot};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::credits::{self, GiveCredits, SellPack};
use ncrm_core::db::{self, TenantTx};
use ncrm_core::enrollments::{self, NewEnrollment};
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
        tx.commit().await?;
        println!(
            "Rhythm House already exists. {phone} can sign in as {}.",
            if already { "before" } else { "a new owner" }
        );
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

    println!(
        "Created Rhythm House: 3 branches, 10 classes, {} students.",
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
