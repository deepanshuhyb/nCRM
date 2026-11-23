//! Fee reminders and broadcasts against real Postgres: who gets what,
//! that nothing is sent twice, and the ways each should refuse (switched
//! off, night time, wrong role, wrong studio, bad input).

use ncrm_core::access::Actor;
use ncrm_core::batches::{self, NewBatch, Slot};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::broadcasts::{self, Audience, NewBroadcast};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::enrollments::{self, NewEnrollment};
use ncrm_core::fees::{self, FeeSettings, NewCharge, NewPayment, PaymentMethod};
use ncrm_core::messaging::{self, Incoming};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::reminders;
use ncrm_core::staff::{self, NewStaff, Staff, StaffRole};
use ncrm_core::students::{self, NewContact, NewStudent, Relation, StudentEdit, StudentStatus};
use ncrm_core::whatsapp::{MessageSender, Templates};
use ncrm_core::{jobs, Error};
use sqlx::PgPool;
use time::macros::time;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

const MEENA: &str = "+919811100001";
const RAVI: &str = "+919811100002";
const SUNIL: &str = "+919811100003";

struct World {
    org: Organization,
    owner: Staff,
    desk: Staff,
    teacher: Staff,
    branch: Uuid,
    batch: Uuid,
    /// Aarav and Anya share a mother (Meena). Bela's father is Ravi.
    /// Chirag has no contact. Dev has left; his father is Sunil.
    aarav: Uuid,
    anya: Uuid,
    bela: Uuid,
    chirag: Uuid,
    dev: Uuid,
    today: Date,
}

fn actor(s: &Staff) -> Actor {
    Actor {
        staff_id: s.id,
        role: s.role,
    }
}

/// A time zone in which it is `hour` o'clock right now, so a test can
/// run at any time of day and still choose "daytime" or "night" for the
/// studio. (In these names the sign is reversed: `Etc/GMT-5` is UTC+5.)
fn zone_where_it_is(hour: i32) -> String {
    let mut offset = hour - i32::from(OffsetDateTime::now_utc().hour());
    if offset < -12 {
        offset += 24;
    }
    if offset > 14 {
        offset -= 24;
    }
    match offset {
        0 => "Etc/GMT".into(),
        o if o > 0 => format!("Etc/GMT-{o}"),
        o => format!("Etc/GMT+{}", -o),
    }
}

async fn set_hour(pool: &PgPool, org: &Organization, hour: i32) {
    sqlx::query("UPDATE organizations SET timezone = $2 WHERE id = $1")
        .bind(org.id)
        .bind(zone_where_it_is(hour))
        .execute(pool)
        .await
        .unwrap();
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
    // One in the afternoon at the studio, whenever the test runs.
    set_hour(pool, &org, 13).await;
    let mut t = TenantTx::begin(pool, org.id).await.unwrap();
    let today = t.studio_today().await.unwrap();
    let n = slug.len() as u64 % 10;
    let person = |name: &str, last: u8, role| NewStaff {
        name: name.into(),
        phone: format!("+91980000{n}00{last}"),
        role,
        branch_id: None,
    };
    let owner = staff::create(&mut t, person("Asha", 1, StaffRole::Owner))
        .await
        .unwrap();
    let desk = staff::create(&mut t, person("Farah", 2, StaffRole::FrontDesk))
        .await
        .unwrap();
    let teacher = staff::create(&mut t, person("Ritu", 3, StaffRole::Instructor))
        .await
        .unwrap();
    let me = actor(&owner);
    let branch = branches::create(
        &mut t,
        NewBranch {
            name: "Hauz Khas".into(),
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
            name: "Hip-hop Beginners".into(),
            style: None,
            branch_id: branch,
            room_id: None,
            instructor_id: None,
            capacity: None,
            starts_on: None,
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
    let mut ids = Vec::new();
    for (name, contact) in [
        ("Aarav", Some(("Meena", MEENA))),
        ("Anya", Some(("Meena", MEENA))),
        ("Bela", Some(("Ravi", RAVI))),
        ("Chirag", None),
        ("Dev", Some(("Sunil", SUNIL))),
    ] {
        let id = students::create(
            &mut t,
            me,
            NewStudent {
                name: name.into(),
                date_of_birth: None,
                branch_id: None,
                notes: None,
                joined_on: None,
                contacts: contact
                    .map(|(name, phone)| NewContact {
                        name: name.into(),
                        phone: phone.into(),
                        relation: Relation::Other,
                        is_primary: true,
                    })
                    .into_iter()
                    .collect(),
            },
        )
        .await
        .unwrap()
        .student
        .id;
        ids.push(id);
    }
    // Aarav and Bela are in the class.
    for id in [ids[0], ids[2]] {
        enrollments::enroll(
            &mut t,
            me,
            id,
            NewEnrollment {
                batch_id: batch,
                plan_id: None,
                starts_on: None,
            },
        )
        .await
        .unwrap();
    }
    // Dev has left.
    students::update(
        &mut t,
        me,
        ids[4],
        StudentEdit {
            name: "Dev".into(),
            date_of_birth: None,
            branch_id: None,
            status: StudentStatus::Left,
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
        desk,
        teacher,
        branch,
        batch,
        aarav: ids[0],
        anya: ids[1],
        bela: ids[2],
        chirag: ids[3],
        dev: ids[4],
        today,
    }
}

async fn owes(pool: &PgPool, w: &World, student: Uuid, rupees: i64, due_in_days: i64) {
    let mut t = TenantTx::begin(pool, w.org.id).await.unwrap();
    fees::add_charge(
        &mut t,
        actor(&w.owner),
        student,
        NewCharge {
            description: "Fee".into(),
            amount_paise: rupees * 100,
            due_on: Some(w.today + Duration::days(due_in_days)),
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();
}

async fn remind(pool: &PgPool, w: &World) -> reminders::Reminded {
    let mut t = TenantTx::begin(pool, w.org.id).await.unwrap();
    let done = reminders::queue_due(&mut t, &Templates::default())
        .await
        .unwrap();
    t.commit().await.unwrap();
    done
}

/// (phone, body, template, how many template values) of every outgoing
/// message in a studio, by phone.
async fn outgoing(pool: &PgPool, w: &World) -> Vec<(String, String, Option<String>, i32)> {
    sqlx::query_as(
        "SELECT c.phone, m.body, m.template, coalesce(jsonb_array_length(m.template_params), 0)
           FROM messages m JOIN conversations c ON c.id = m.conversation_id
          WHERE m.org_id = $1 AND m.direction = 'out'
          ORDER BY c.phone, m.created_at",
    )
    .bind(w.org.id)
    .fetch_all(pool)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------
// Fee reminders
// ---------------------------------------------------------------------------

#[sqlx::test(migrator = "MIGRATOR")]
async fn each_contact_gets_one_reminder_covering_everything_due(pool: PgPool) {
    let w = world(&pool, "remind").await;
    owes(&pool, &w, w.aarav, 2500, 3).await; // three days before
    owes(&pool, &w, w.anya, 1000, 0).await; // due today, same mother
    owes(&pool, &w, w.aarav, 700, -20).await; // long overdue: no reminder of its own
    owes(&pool, &w, w.bela, 500, -3).await; // three days late
    owes(&pool, &w, w.chirag, 900, 0).await; // nobody to tell
    owes(&pool, &w, w.dev, 400, 0).await; // has left

    let done = remind(&pool, &w).await;
    assert_eq!((done.messages, done.charges, done.no_contact), (2, 3, 1));

    let sent = outgoing(&pool, &w).await;
    assert_eq!(sent.len(), 2);
    // Meena hears once, about both children, with the old ₹700 counted in.
    assert_eq!(sent[0].0, MEENA);
    assert_eq!(
        sent[0].1,
        "Hi Meena, this is Studio remind. Fees for Aarav and Anya: ₹4,200 to pay, ₹700 of it overdue. You can pay at the front desk by cash, UPI or card."
    );
    assert_eq!(sent[1].0, RAVI);
    assert!(sent[1].1.contains("Fees for Bela: ₹500 is overdue."));
    for (_, _, template, values) in &sent {
        assert_eq!(template.as_deref(), Some("ncrm_fee_reminder"));
        assert_eq!(*values, 4);
    }
    // Nobody who has left, and nobody without a contact.
    assert!(sent.iter().all(|m| m.0 != SUNIL));

    // Again, a minute or a day's worth of passes later: nothing new.
    assert_eq!(remind(&pool, &w).await, reminders::Reminded::default());
    assert_eq!(outgoing(&pool, &w).await.len(), 2);

    // They leave with the outbox like any other message.
    let pass = jobs::send_messages(&pool, &MessageSender::Test)
        .await
        .unwrap();
    assert_eq!(pass.sent, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_reminder_follows_the_fee_through_its_steps_and_stops(pool: PgPool) {
    let w = world(&pool, "steps").await;
    owes(&pool, &w, w.bela, 500, 3).await;
    assert_eq!(remind(&pool, &w).await.messages, 1); // 3 days before
                                                     // Move the fee back a day at a time by moving its due date (the
                                                     // operator's connection may; the app's role may not).
    let mut total = 1;
    for day in 1..=14 {
        sqlx::query("UPDATE charges SET due_on = due_on - 1 WHERE org_id = $1")
            .bind(w.org.id)
            .execute(&pool)
            .await
            .unwrap();
        let made = remind(&pool, &w).await.messages;
        // Due day (3 days on), 3 days late (6 on), 7 days late (10 on).
        assert_eq!(made, u32::from([3, 6, 10].contains(&day)), "day {day}");
        total += made;
    }
    assert_eq!(total, 4);
    let last = outgoing(&pool, &w).await.pop().unwrap();
    assert!(last.1.contains("₹500 is overdue"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reminders_respect_the_switch_the_clock_payment_and_the_studio(pool: PgPool) {
    let w = world(&pool, "polite").await;
    let other = world(&pool, "neighbour").await;
    owes(&pool, &w, w.bela, 500, 0).await;
    owes(&pool, &w, w.aarav, 800, 0).await;
    owes(&pool, &other, other.bela, 999, 0).await;

    // Switched off by the studio.
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    let switch = |on: bool| FeeSettings {
        fees_from: None,
        fee_due_day: 5,
        fee_reminders: Some(on),
    };
    let summary = fees::set_settings(&mut t, actor(&w.owner), switch(false))
        .await
        .unwrap();
    assert!(!summary.fee_reminders);
    // Front desk takes money but doesn't change how fees work.
    assert!(matches!(
        fees::set_settings(&mut t, actor(&w.desk), switch(true)).await,
        Err(Error::Forbidden)
    ));
    t.commit().await.unwrap();
    assert_eq!(remind(&pool, &w).await.messages, 0);

    // On again, but it is three in the morning there.
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    // Leaving the switch out of a settings change doesn't touch it.
    let unchanged = fees::set_settings(
        &mut t,
        actor(&w.owner),
        FeeSettings {
            fees_from: None,
            fee_due_day: 5,
            fee_reminders: None,
        },
    )
    .await
    .unwrap();
    assert!(!unchanged.fee_reminders);
    assert!(
        fees::set_settings(&mut t, actor(&w.owner), switch(true))
            .await
            .unwrap()
            .fee_reminders
    );
    t.commit().await.unwrap();
    set_hour(&pool, &w.org, 3).await;
    assert_eq!(remind(&pool, &w).await.messages, 0);

    // Daytime. Bela pays first, so only Aarav's mother hears.
    set_hour(&pool, &w.org, 13).await;
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    fees::record_payment(
        &mut t,
        actor(&w.desk),
        w.bela,
        NewPayment {
            amount_paise: 50_000,
            method: PaymentMethod::Cash,
            reference: None,
            note: None,
            received_on: None,
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();
    let done = remind(&pool, &w).await;
    assert_eq!((done.messages, done.charges), (1, 1));
    let sent = outgoing(&pool, &w).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, MEENA);

    // The neighbouring studio's fee was never this studio's business,
    // and its own pass is its own.
    assert!(outgoing(&pool, &other).await.is_empty());
    let all = jobs::fee_reminders(&pool, &Templates::default())
        .await
        .unwrap();
    assert_eq!((all.studios, all.messages), (2, 1));
    assert!(all.errors.is_empty());
    assert!(outgoing(&pool, &other).await[0].1.contains("₹999"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_reminder_stays_out_of_the_inbox_until_they_reply(pool: PgPool) {
    let w = world(&pool, "quiet").await;
    owes(&pool, &w, w.bela, 500, 0).await;
    remind(&pool, &w).await;

    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    assert!(messaging::inbox(&mut t, actor(&w.desk))
        .await
        .unwrap()
        .is_empty());
    let got = messaging::receive(
        &mut t,
        &Incoming {
            from: RAVI.into(),
            wa_message_id: "wamid.reply".into(),
            text: "Paid by UPI just now".into(),
            profile_name: None,
        },
    )
    .await
    .unwrap();
    // Their reply joins the conversation the reminder started.
    assert!(!got.first_contact);
    let inbox = messaging::inbox(&mut t, actor(&w.desk)).await.unwrap();
    assert_eq!(inbox.len(), 1);
    let thread = messaging::thread(&mut t, actor(&w.desk), inbox[0].id)
        .await
        .unwrap();
    assert_eq!(thread.messages.len(), 2);
    assert!(thread.messages[0].body.contains("Fees for Bela"));
    assert_eq!(thread.messages[1].body, "Paid by UPI just now");
}

// ---------------------------------------------------------------------------
// Broadcasts
// ---------------------------------------------------------------------------

fn text(audience: Audience, text: &str) -> NewBroadcast {
    NewBroadcast {
        audience,
        text: text.into(),
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_broadcast_reaches_each_main_contact_once(pool: PgPool) {
    let w = world(&pool, "cast").await;
    let me = actor(&w.owner);
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();

    // Everyone: four students haven't left; two parents; Chirag is missed.
    let preview = broadcasts::preview(&mut t, me, &Audience::Everyone)
        .await
        .unwrap();
    assert_eq!(preview.label, "Everyone");
    assert_eq!(
        (
            preview.students,
            preview.recipients,
            preview.without_contact
        ),
        (4, 2, 1)
    );
    assert_eq!(preview.sample, ["Meena", "Ravi"]);
    // Looking sends nothing.
    assert!(outgoing(&pool, &w).await.is_empty());

    let sent = broadcasts::send(
        &mut t,
        me,
        text(
            Audience::Everyone,
            "  No class tomorrow.\n\nThe hall is   being painted. ",
        ),
        &Templates::default(),
    )
    .await
    .unwrap();
    // One paragraph, however it was typed.
    assert_eq!(sent.body, "No class tomorrow. The hall is being painted.");
    assert_eq!((sent.recipients, sent.waiting, sent.sent), (2, 2, 0));
    assert_eq!(sent.audience_label, "Everyone");
    assert_eq!(sent.sent_by, "Asha");
    t.commit().await.unwrap();

    let out = outgoing(&pool, &w).await;
    assert_eq!(out.len(), 2);
    assert_eq!((out[0].0.as_str(), out[1].0.as_str()), (MEENA, RAVI));
    for (_, body, template, values) in &out {
        assert_eq!(body, "No class tomorrow. The hall is being painted.");
        assert_eq!(template.as_deref(), Some("ncrm_announcement"));
        assert_eq!(*values, 3);
    }

    jobs::send_messages(&pool, &MessageSender::Test)
        .await
        .unwrap();
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    let history = broadcasts::list(&mut t, me).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!((history[0].waiting, history[0].sent), (0, 2));

    // The same thing to the same people straight away is a slip.
    let again = broadcasts::send(
        &mut t,
        me,
        text(
            Audience::Everyone,
            "No class tomorrow. The hall is being painted.",
        ),
        &Templates::default(),
    )
    .await;
    assert!(matches!(again, Err(Error::Conflict(_))));
    // Different words are a different message.
    broadcasts::send(
        &mut t,
        me,
        text(Audience::Everyone, "Class is back on Thursday."),
        &Templates::default(),
    )
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn each_kind_of_audience_means_the_right_people(pool: PgPool) {
    let w = world(&pool, "who").await;
    let me = actor(&w.owner);
    owes(&pool, &w, w.anya, 1000, 5).await;
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    let mut see = async |audience: Audience| {
        let p = broadcasts::preview(&mut t, me, &audience).await.unwrap();
        (p.label, p.students, p.sample)
    };
    // The class has Aarav and Bela.
    assert_eq!(
        see(Audience::Batch { batch_id: w.batch }).await,
        (
            "Hip-hop Beginners".into(),
            2,
            vec!["Meena".to_string(), "Ravi".into()]
        )
    );
    // The branch: students in a class there (nobody has it as their own branch).
    assert_eq!(
        see(Audience::Branch {
            branch_id: w.branch
        })
        .await
        .1,
        2
    );
    // Only Anya owes.
    assert_eq!(
        see(Audience::Owing).await,
        ("Students who owe".into(), 1, vec!["Meena".to_string()])
    );
    // Picked by hand, even someone who has left. Picking twice counts once.
    assert_eq!(
        see(Audience::Students {
            student_ids: vec![w.dev, w.dev, w.bela]
        })
        .await,
        (
            "2 chosen students".into(),
            2,
            vec!["Ravi".to_string(), "Sunil".into()]
        )
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn broadcasts_refuse_the_wrong_role_studio_and_input(pool: PgPool) {
    let w = world(&pool, "strict").await;
    let other = world(&pool, "elsewhere").await;
    let me = actor(&w.owner);
    let templates = Templates::default();
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();

    // Front desk answers people; it doesn't message everyone. Instructors neither.
    for who in [actor(&w.desk), actor(&w.teacher)] {
        assert!(matches!(
            broadcasts::preview(&mut t, who, &Audience::Everyone).await,
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            broadcasts::send(&mut t, who, text(Audience::Everyone, "Hello"), &templates).await,
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            broadcasts::list(&mut t, who).await,
            Err(Error::Forbidden)
        ));
    }

    // Nothing to say, too much to say, nobody to say it to.
    for (audience, words) in [
        (Audience::Everyone, " \n ".to_string()),
        (Audience::Everyone, "x".repeat(601)),
        (
            Audience::Students {
                student_ids: vec![],
            },
            "Hello".to_string(),
        ),
        (
            Audience::Students {
                student_ids: vec![w.chirag],
            },
            "Hello".to_string(),
        ),
    ] {
        let r = broadcasts::send(&mut t, me, text(audience, &words), &templates).await;
        assert!(matches!(r, Err(Error::Invalid(_))), "{words:.20}");
    }

    // Another studio's class and branch don't exist here; its students
    // match nobody.
    assert!(matches!(
        broadcasts::preview(
            &mut t,
            me,
            &Audience::Batch {
                batch_id: other.batch
            }
        )
        .await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        broadcasts::preview(
            &mut t,
            me,
            &Audience::Branch {
                branch_id: other.branch
            }
        )
        .await,
        Err(Error::NotFound)
    ));
    let theirs = Audience::Students {
        student_ids: vec![other.aarav, other.bela],
    };
    assert_eq!(
        broadcasts::preview(&mut t, me, &theirs)
            .await
            .unwrap()
            .recipients,
        0
    );
    assert!(matches!(
        broadcasts::send(&mut t, me, text(theirs, "Hello"), &templates).await,
        Err(Error::Invalid(_))
    ));
    t.commit().await.unwrap();
    // After all those refusals, nothing was queued anywhere.
    assert!(outgoing(&pool, &w).await.is_empty());
    assert!(outgoing(&pool, &other).await.is_empty());

    // A broadcast in one studio isn't in the other's history.
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    broadcasts::send(&mut t, me, text(Audience::Everyone, "Hello"), &templates)
        .await
        .unwrap();
    t.commit().await.unwrap();
    let mut t = TenantTx::begin(&pool, other.org.id).await.unwrap();
    assert!(broadcasts::list(&mut t, actor(&other.owner))
        .await
        .unwrap()
        .is_empty());
    assert!(outgoing(&pool, &other).await.is_empty());
}
