//! WhatsApp messaging against real Postgres: the inbox, the bot, the
//! outbox, and the ways each should refuse (wrong role, wrong studio, too
//! late to reply, a studio trying to claim a number).

use ncrm_core::access::Actor;
use ncrm_core::batches::{self, NewBatch, Slot};
use ncrm_core::bot;
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::enrollments::{self, NewEnrollment};
use ncrm_core::fees::{self, NewCharge};
use ncrm_core::messaging::{self, Author, BotMode, Incoming, MessageStatus, Reply, SettingsEdit};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::staff::{self, NewStaff, Staff, StaffRole};
use ncrm_core::students::{self, NewContact, NewStudent, Relation};
use ncrm_core::whatsapp::MessageSender;
use ncrm_core::Error;
use sqlx::PgPool;
use time::macros::time;
use uuid::Uuid;

const PARENT: &str = "+919811100001";
const STRANGER: &str = "+919822200002";

struct World {
    org: Organization,
    owner: Staff,
    desk: Staff,
    teacher: Staff,
}

fn actor(s: &Staff) -> Actor {
    Actor {
        staff_id: s.id,
        role: s.role,
    }
}

/// A studio with one class (Mon and Wed at 6pm), one student in it whose
/// mother is `PARENT`, and ₹2,500 owed.
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
    let person = |name: &str, phone: &str, role| NewStaff {
        name: name.into(),
        phone: phone.into(),
        role,
        branch_id: None,
    };
    let n = slug.len() as u64 % 10;
    let owner = staff::create(
        &mut t,
        person("Asha", &format!("+91980000{n}001"), StaffRole::Owner),
    )
    .await
    .unwrap();
    let desk = staff::create(
        &mut t,
        person("Farah", &format!("+91980000{n}002"), StaffRole::FrontDesk),
    )
    .await
    .unwrap();
    let teacher = staff::create(
        &mut t,
        person("Ritu", &format!("+91980000{n}003"), StaffRole::Instructor),
    )
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
            instructor_id: Some(teacher.id),
            capacity: None,
            starts_on: None,
            ends_on: None,
            slots: [1, 3]
                .map(|weekday| Slot {
                    weekday,
                    starts_at: time!(18:00),
                    ends_at: time!(19:00),
                })
                .to_vec(),
        },
    )
    .await
    .unwrap()
    .summary
    .batch
    .id;
    let kid = students::create(
        &mut t,
        me,
        NewStudent {
            name: "Aarav".into(),
            date_of_birth: None,
            branch_id: None,
            notes: None,
            joined_on: None,
            contacts: vec![NewContact {
                name: "Meena".into(),
                phone: PARENT.into(),
                relation: Relation::Mother,
                is_primary: true,
            }],
        },
    )
    .await
    .unwrap()
    .student
    .id;
    enrollments::enroll(
        &mut t,
        me,
        kid,
        NewEnrollment {
            batch_id: batch,
            plan_id: None,
            starts_on: None,
        },
    )
    .await
    .unwrap();
    fees::add_charge(
        &mut t,
        me,
        kid,
        NewCharge {
            description: "Costume".into(),
            amount_paise: 250_000,
            due_on: None,
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
    }
}

fn says(from: &str, id: &str, text: &str) -> Incoming {
    Incoming {
        from: from.into(),
        wa_message_id: id.into(),
        text: text.into(),
        profile_name: Some("WhatsApp name".into()),
    }
}

/// Store an incoming message and let the bot answer it. Returns the
/// conversation.
async fn arrives(pool: &PgPool, w: &World, incoming: Incoming) -> Uuid {
    let mut t = TenantTx::begin(pool, w.org.id).await.unwrap();
    let got = messaging::receive(&mut t, &incoming).await.unwrap();
    t.commit().await.unwrap();
    bot::answer(pool, w.org.id, got.conversation_id, None)
        .await
        .unwrap();
    got.conversation_id
}

/// The bodies of the bot's messages in a conversation, oldest first.
async fn bot_lines(pool: &PgPool, w: &World, conversation: Uuid) -> Vec<String> {
    let mut t = TenantTx::begin(pool, w.org.id).await.unwrap();
    messaging::thread(&mut t, actor(&w.owner), conversation)
        .await
        .unwrap()
        .messages
        .into_iter()
        .filter(|m| m.author == Author::Bot)
        .map(|m| m.body)
        .collect()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_inbox_shows_who_wrote_and_staff_can_reply(pool: PgPool) {
    let w = world(&pool, "inbox").await;
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    let got = messaging::receive(&mut t, &says(PARENT, "wamid.1", "Is class on today?"))
        .await
        .unwrap();
    assert!(got.new && got.first_contact);

    let inbox = messaging::inbox(&mut t, actor(&w.desk)).await.unwrap();
    assert_eq!(inbox.len(), 1);
    let c = &inbox[0];
    assert_eq!(c.phone, PARENT);
    // The studio's own name for them, not the WhatsApp profile name.
    assert_eq!(c.contact_name.as_deref(), Some("Meena"));
    assert_eq!(c.students.as_deref(), Some("Aarav"));
    assert_eq!((c.unread, c.can_reply), (1, true));
    assert_eq!(c.preview.as_deref(), Some("Is class on today?"));

    // Opening it marks it read.
    let thread = messaging::thread(&mut t, actor(&w.desk), c.id)
        .await
        .unwrap();
    assert_eq!(thread.conversation.unread, 0);
    assert_eq!(thread.messages.len(), 1);

    // A reply is queued under the person's name, and the bot steps back.
    let after = messaging::reply(
        &mut t,
        actor(&w.desk),
        c.id,
        Reply {
            text: "  Yes, 6pm as usual.  ".into(),
        },
    )
    .await
    .unwrap();
    let mine = after.messages.last().unwrap();
    assert_eq!(mine.body, "Yes, 6pm as usual.");
    assert_eq!(mine.author, Author::Staff);
    assert_eq!(mine.staff_name.as_deref(), Some("Farah"));
    assert_eq!(mine.status, MessageStatus::Queued);
    assert!(after.conversation.bot_paused);
    assert!(!messaging::bot_may_answer(&mut t, c.id).await.unwrap());

    // Handing back lets the bot answer again.
    let back = messaging::hand_back(&mut t, actor(&w.desk), c.id)
        .await
        .unwrap();
    assert!(!back.conversation.bot_paused);
    assert!(messaging::bot_may_answer(&mut t, c.id).await.unwrap());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_inbox_refuses_the_wrong_role_studio_and_input(pool: PgPool) {
    let w = world(&pool, "refuse").await;
    let other = world(&pool, "elsewhere").await;
    let conversation = arrives(&pool, &w, says(PARENT, "wamid.1", "hello")).await;
    let reply = || Reply {
        text: "Hello".into(),
    };

    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    // Instructors don't read parents' messages.
    let teacher = actor(&w.teacher);
    assert!(matches!(
        messaging::inbox(&mut t, teacher).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        messaging::thread(&mut t, teacher, conversation).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        messaging::reply(&mut t, teacher, conversation, reply()).await,
        Err(Error::Forbidden)
    ));
    // Front desk uses the inbox but doesn't choose how the bot behaves.
    assert!(matches!(
        messaging::set_bot_mode(
            &mut t,
            actor(&w.desk),
            SettingsEdit {
                bot_mode: BotMode::Off
            }
        )
        .await,
        Err(Error::Forbidden)
    ));
    // Empty and oversized replies.
    for text in ["   ".to_string(), "x".repeat(1001)] {
        assert!(matches!(
            messaging::reply(&mut t, actor(&w.desk), conversation, Reply { text }).await,
            Err(Error::Invalid(_))
        ));
    }
    // WhatsApp won't deliver free text a day after their last message.
    sqlx::query("UPDATE conversations SET last_inbound_at = now() - interval '25 hours'")
        .execute(t.conn())
        .await
        .unwrap();
    let late = messaging::reply(&mut t, actor(&w.desk), conversation, reply()).await;
    assert!(matches!(late, Err(Error::Invalid(m)) if m.contains("24 hours")));
    drop(t);

    // Another studio sees nothing and can't reach the conversation.
    let mut t = TenantTx::begin(&pool, other.org.id).await.unwrap();
    assert!(messaging::inbox(&mut t, actor(&other.owner))
        .await
        .unwrap()
        .is_empty());
    assert!(matches!(
        messaging::thread(&mut t, actor(&other.owner), conversation).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        messaging::reply(&mut t, actor(&other.owner), conversation, reply()).await,
        Err(Error::NotFound)
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_message_delivered_twice_is_stored_once(pool: PgPool) {
    let w = world(&pool, "twice").await;
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    let first = messaging::receive(&mut t, &says(PARENT, "wamid.same", "hi"))
        .await
        .unwrap();
    let again = messaging::receive(&mut t, &says(PARENT, "wamid.same", "hi"))
        .await
        .unwrap();
    assert!(first.new && !again.new);
    assert_eq!(first.conversation_id, again.conversation_id);
    let thread = messaging::thread(&mut t, actor(&w.owner), first.conversation_id)
        .await
        .unwrap();
    assert_eq!(thread.messages.len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_menu_bot_answers_a_parent_from_their_own_records(pool: PgPool) {
    let w = world(&pool, "menu").await;
    let c = arrives(&pool, &w, says(PARENT, "wamid.1", "Hi")).await;
    arrives(&pool, &w, says(PARENT, "wamid.2", "1")).await;
    arrives(&pool, &w, says(PARENT, "wamid.3", "2")).await;
    let lines = bot_lines(&pool, &w, c).await;
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("Hi Meena, this is Studio menu"));
    assert!(lines[1].contains("Hip-hop Beginners: Mon, Wed 18:00–19:00 at Hauz Khas"));
    assert!(lines[2].contains("Aarav: ₹2,500 to pay"));

    // Something it doesn't understand goes to a person, and then it
    // keeps quiet until staff hand the conversation back.
    arrives(&pool, &w, says(PARENT, "wamid.4", "Aarav hurt his ankle")).await;
    arrives(&pool, &w, says(PARENT, "wamid.5", "2")).await;
    assert_eq!(bot_lines(&pool, &w, c).await.len(), 4);
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    assert_eq!(messaging::waiting(&mut t).await.unwrap(), 1);
    messaging::hand_back(&mut t, actor(&w.desk), c)
        .await
        .unwrap();
    t.commit().await.unwrap();
    arrives(&pool, &w, says(PARENT, "wamid.6", "3")).await;
    assert_eq!(bot_lines(&pool, &w, c).await.len(), 5);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_stranger_learns_nothing_about_students(pool: PgPool) {
    let w = world(&pool, "stranger").await;
    let c = arrives(&pool, &w, says(STRANGER, "wamid.1", "hello")).await;
    arrives(&pool, &w, says(STRANGER, "wamid.2", "1")).await;
    arrives(
        &pool,
        &w,
        says(STRANGER, "wamid.3", "how much does Aarav owe?"),
    )
    .await;
    let lines = bot_lines(&pool, &w, c).await;
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("1. Classes and timings"));
    assert!(lines[1].contains("Hip-hop Beginners: Mon, Wed 18:00–19:00"));
    for line in &lines {
        assert!(!line.contains('₹') && !line.contains("Aarav") && !line.contains("Meena"));
    }
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    let inbox = messaging::inbox(&mut t, actor(&w.desk)).await.unwrap();
    assert!(inbox[0].needs_staff);
    assert_eq!(inbox[0].contact_name, None);
    assert_eq!(inbox[0].profile_name.as_deref(), Some("WhatsApp name"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_parent_in_two_studios_hears_only_about_the_one_they_wrote_to(pool: PgPool) {
    let here = world(&pool, "here").await;
    let there = world(&pool, "there").await;
    // In the other studio the same number owes far more.
    let mut t = TenantTx::begin(&pool, there.org.id).await.unwrap();
    let kid: Uuid = sqlx::query_scalar("SELECT id FROM students LIMIT 1")
        .fetch_one(t.conn())
        .await
        .unwrap();
    fees::add_charge(
        &mut t,
        actor(&there.owner),
        kid,
        NewCharge {
            description: "Annual show".into(),
            amount_paise: 9_999_900,
            due_on: None,
        },
    )
    .await
    .unwrap();
    t.commit().await.unwrap();

    let c = arrives(&pool, &here, says(PARENT, "wamid.1", "fees?")).await;
    let lines = bot_lines(&pool, &here, c).await;
    assert!(lines[0].contains("₹2,500 to pay"));
    assert!(!lines[0].contains("99,999") && !lines[0].contains("1,02,499"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_bot_that_is_off_says_nothing(pool: PgPool) {
    let w = world(&pool, "off").await;
    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    messaging::set_bot_mode(
        &mut t,
        actor(&w.owner),
        SettingsEdit {
            bot_mode: BotMode::Off,
        },
    )
    .await
    .unwrap();
    let settings = messaging::settings(&mut t, actor(&w.desk), false, false)
        .await
        .unwrap();
    assert_eq!(settings.bot_mode, BotMode::Off);
    assert!(!settings.connected && !settings.live);
    t.commit().await.unwrap();
    let c = arrives(&pool, &w, says(PARENT, "wamid.1", "Hi")).await;
    assert!(bot_lines(&pool, &w, c).await.is_empty());
}

/// The status of the one outgoing message in a conversation.
async fn out_status(t: &mut TenantTx, conversation: Uuid) -> MessageStatus {
    sqlx::query_scalar(
        "SELECT status FROM messages WHERE conversation_id = $1 AND direction = 'out'",
    )
    .bind(conversation)
    .fetch_one(t.conn())
    .await
    .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn queued_messages_are_sent_once_and_reports_only_move_forward(pool: PgPool) {
    let w = world(&pool, "send").await;
    let c = arrives(&pool, &w, says(PARENT, "wamid.1", "Hi")).await;

    // Test mode: nothing leaves the server, the message is marked sent.
    let sent = messaging::send_queued(&pool, w.org.id, &MessageSender::Test)
        .await
        .unwrap();
    assert_eq!((sent.sent, sent.failed, sent.retrying), (1, 0, 0));
    let again = messaging::send_queued(&pool, w.org.id, &MessageSender::Test)
        .await
        .unwrap();
    assert_eq!(again.sent, 0);

    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    assert_eq!(out_status(&mut t, c).await, MessageStatus::Sent);
    sqlx::query("UPDATE messages SET wa_message_id = 'wamid.out' WHERE direction = 'out'")
        .execute(t.conn())
        .await
        .unwrap();
    // Reports can arrive out of order; the status never goes backwards.
    assert!(
        messaging::delivery_report(&mut t, "wamid.out", MessageStatus::Read, None)
            .await
            .unwrap()
    );
    assert!(
        !messaging::delivery_report(&mut t, "wamid.out", MessageStatus::Delivered, None)
            .await
            .unwrap()
    );
    assert_eq!(out_status(&mut t, c).await, MessageStatus::Read);
    // A report for a message that isn't ours changes nothing.
    assert!(
        !messaging::delivery_report(&mut t, "wamid.unknown", MessageStatus::Read, None)
            .await
            .unwrap()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_the_operator_can_give_a_studio_a_number(pool: PgPool) {
    let w = world(&pool, "number").await;
    let other = world(&pool, "taker").await;

    // From inside a studio (the app's database role) the column can't be
    // written, so a studio can't point someone else's number at itself.
    let mut t = TenantTx::begin(&pool, other.org.id).await.unwrap();
    let claimed = sqlx::query("UPDATE organizations SET wa_phone_number_id = '123456789'")
        .execute(t.conn())
        .await;
    let denied = claimed.unwrap_err();
    assert_eq!(
        denied.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501"),
        "expected permission denied, got {denied}"
    );
    drop(t);

    // The operator sets it; incoming messages for that number then find
    // the studio, and no other number does.
    sqlx::query("UPDATE organizations SET wa_phone_number_id = '123456789', wa_display_phone = '+91 98110 00000' WHERE id = $1")
        .bind(w.org.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        messaging::studio_for_number(&pool, "123456789")
            .await
            .unwrap(),
        Some(w.org.id)
    );
    assert_eq!(
        messaging::studio_for_number(&pool, "987654321")
            .await
            .unwrap(),
        None
    );
    // And one number can't belong to two studios.
    let twice =
        sqlx::query("UPDATE organizations SET wa_phone_number_id = '123456789' WHERE id = $1")
            .bind(other.org.id)
            .execute(&pool)
            .await;
    assert!(twice.is_err());

    let mut t = TenantTx::begin(&pool, w.org.id).await.unwrap();
    let settings = messaging::settings(&mut t, actor(&w.owner), true, false)
        .await
        .unwrap();
    assert!(settings.connected && settings.live);
    assert_eq!(settings.display_phone.as_deref(), Some("+91 98110 00000"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_worker_sends_what_is_waiting_in_every_studio(pool: PgPool) {
    use ncrm_core::jobs;
    let one = world(&pool, "one").await;
    let two = world(&pool, "second").await;
    let quiet = world(&pool, "quiet-studio").await;
    arrives(&pool, &one, says(PARENT, "wamid.1", "Hi")).await;
    arrives(&pool, &two, says(PARENT, "wamid.2", "Hi")).await;

    // Only the studios with something to send are opened.
    let mut waiting = jobs::studios_with_messages_to_send(&pool).await.unwrap();
    waiting.sort();
    let mut expected = vec![one.org.id, two.org.id];
    expected.sort();
    assert_eq!(waiting, expected);
    assert!(!waiting.contains(&quiet.org.id));

    let pass = jobs::send_messages(&pool, &MessageSender::Test)
        .await
        .unwrap();
    assert_eq!((pass.studios, pass.sent, pass.failed), (2, 2, 0));
    assert!(pass.errors.is_empty());
    // Nothing is left, so the next pass has no studio to visit.
    let again = jobs::send_messages(&pool, &MessageSender::Test)
        .await
        .unwrap();
    assert_eq!((again.studios, again.sent), (0, 0));
}
