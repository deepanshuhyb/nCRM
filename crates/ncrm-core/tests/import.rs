//! Importing a student list from a spreadsheet, against real Postgres:
//! a messy real-world sheet, hostile files, roles, and other studios.

use std::io::Write;

use ncrm_core::access::Actor;
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::import::{self, ImportOptions, Outcome};
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::staff::{self, NewStaff, Staff, StaffRole};
use ncrm_core::students::{self, NewContact, NewStudent, Relation};
use ncrm_core::Error;
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};
use sqlx::PgPool;
use time::macros::date;
use uuid::Uuid;

async fn studio(pool: &PgPool, slug: &str) -> (Organization, Staff, Staff, Uuid) {
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
    let person = |phone: &str, role| NewStaff {
        name: format!("{role:?}"),
        phone: phone.into(),
        role,
        branch_id: None,
    };
    let desk = staff::create(&mut tx, person("+919800000002", StaffRole::FrontDesk))
        .await
        .unwrap();
    let teacher = staff::create(&mut tx, person("+919800000003", StaffRole::Instructor))
        .await
        .unwrap();
    let hauz_khas = branches::create(
        &mut tx,
        NewBranch {
            name: "Hauz Khas".into(),
            ..NewBranch::default()
        },
    )
    .await
    .unwrap();
    branches::create(
        &mut tx,
        NewBranch {
            name: "Noida 18".into(),
            ..NewBranch::default()
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (org, desk, teacher, hauz_khas.id)
}

fn actor(s: &Staff) -> Actor {
    Actor {
        staff_id: s.id,
        role: s.role,
    }
}

/// The kind of sheet a studio actually keeps: a title row above the
/// column titles, numbers stored as numbers, dates both as Excel dates and
/// as typed text, and a few mistakes.
fn messy_sheet() -> Vec<u8> {
    let mut book = Workbook::new();
    let ws = book.add_worksheet();
    let dmy = Format::new().set_num_format("dd/mm/yyyy");
    ws.write_string(0, 0, "Rhythm House students 2026").unwrap();
    let titles = [
        "Student Name",
        "Mobile No",
        "Mother's Name",
        "DOB",
        "Date of Joining",
        "Branch",
        "Remarks",
        "Father Phone",
        "Favourite song",
    ];
    for (c, t) in titles.iter().enumerate() {
        ws.write_string(1, c as u16, *t).unwrap();
    }
    // Row 3: a child; the mobile is the mother's (named in the next column).
    ws.write_string(2, 0, "Aarav  Sharma").unwrap();
    ws.write_number(2, 1, 9876543210.0).unwrap();
    ws.write_string(2, 2, "Neha Sharma").unwrap();
    ws.write_datetime_with_format(2, 3, ExcelDateTime::from_ymd(2015, 3, 4).unwrap(), &dmy)
        .unwrap();
    ws.write_string(2, 4, "03/04/2024").unwrap();
    ws.write_string(2, 5, "noida 18").unwrap();
    ws.write_string(2, 6, "Shy at first").unwrap();
    ws.write_number(2, 7, 9811100099.0).unwrap();
    ws.write_string(2, 8, "Kesariya").unwrap();
    // Row 4: a sibling with the same mother, number typed differently.
    ws.write_string(3, 0, "Anya Sharma").unwrap();
    ws.write_string(3, 1, "+91 98765 43210").unwrap();
    ws.write_string(3, 2, "Neha Sharma").unwrap();
    ws.write_string(3, 5, "Noida 18").unwrap();
    // Row 5: no name.
    ws.write_number(4, 1, 9811100009.0).unwrap();
    // Row 6: Aarav again.
    ws.write_string(5, 0, "aarav sharma").unwrap();
    ws.write_string(5, 1, "098765-43210").unwrap();
    // Row 7: already in the studio.
    ws.write_string(6, 0, "Diya Nair").unwrap();
    ws.write_number(6, 1, 9811100004.0).unwrap();
    // Row 8: nothing readable but the name.
    ws.write_string(7, 0, "Esha Reddy").unwrap();
    ws.write_string(7, 1, "12345").unwrap();
    ws.write_string(7, 3, "31/02/2015").unwrap();
    ws.write_string(7, 4, "next week").unwrap();
    ws.write_string(7, 5, "Delhi").unwrap();
    // Row 9: an adult with a joining date in the future.
    ws.write_string(8, 0, "Farhan Ali").unwrap();
    ws.write_number(8, 1, 9811100006.0).unwrap();
    ws.write_string(8, 4, "2099-01-01").unwrap();
    book.save_to_buffer().unwrap()
}

async fn add_diya(pool: &PgPool, org: &Organization, desk: &Staff) {
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    students::create(
        &mut tx,
        actor(desk),
        NewStudent {
            name: "Diya Nair".into(),
            date_of_birth: None,
            branch_id: None,
            notes: None,
            joined_on: None,
            contacts: vec![NewContact {
                name: "Diya Nair".into(),
                phone: "9811100004".into(),
                relation: Relation::Myself,
                is_primary: true,
            }],
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn count_students(pool: &PgPool, org: &Organization) -> i64 {
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    sqlx::query_scalar("SELECT count(*) FROM students")
        .fetch_one(tx.conn())
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_messy_real_sheet_previews_imports_and_imports_only_once(pool: PgPool) {
    let (org, desk, _, hauz_khas) = studio(&pool, "alpha").await;
    add_diya(&pool, &org, &desk).await;
    let sheet = import::read_sheet(&messy_sheet()).unwrap();
    let options = ImportOptions {
        branch_id: Some(hauz_khas),
    };

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let today = tx.studio_today().await.unwrap();
    let preview = import::preview(&mut tx, actor(&desk), &sheet, &options)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        count_students(&pool, &org).await,
        1,
        "a preview writes nothing"
    );

    assert_eq!(preview.ignored_columns, vec!["Favourite song".to_string()]);
    assert!(preview
        .columns
        .iter()
        .any(|c| c.title == "Date of Joining" && c.used_as == "Joined on"));
    assert_eq!(
        (
            preview.total,
            preview.ready,
            preview.already_here,
            preview.repeated,
            preview.cannot_import
        ),
        (7, 4, 1, 1, 1)
    );
    let row = |n: u32| preview.rows.iter().find(|r| r.row == n).unwrap();

    let aarav = row(3);
    assert_eq!(aarav.name, "Aarav Sharma", "spaces tidied");
    assert_eq!(aarav.outcome, Outcome::Ready);
    assert_eq!(aarav.date_of_birth, Some(date!(2015 - 03 - 04)));
    assert_eq!(
        aarav.joined_on,
        date!(2024 - 04 - 03),
        "typed dates are day first"
    );
    assert_eq!(aarav.branch_name.as_deref(), Some("Noida 18"));
    assert_eq!(aarav.contacts.len(), 2);
    assert_eq!(aarav.contacts[0].name, "Neha Sharma");
    assert_eq!(aarav.contacts[0].relation, Relation::Mother);
    assert_eq!(aarav.contacts[0].phone, "+919876543210");
    assert_eq!(aarav.contacts[1].relation, Relation::Father);
    assert_eq!(aarav.contacts[1].name, "Father of Aarav");

    assert!(row(4)
        .notes
        .iter()
        .any(|n| n == "Shares a contact with Aarav Sharma."));
    assert_eq!(row(5).outcome, Outcome::CannotImport);
    assert_eq!(row(6).outcome, Outcome::Repeated);
    assert_eq!(row(6).notes[0], "Same student as row 3.");
    assert_eq!(row(7).outcome, Outcome::AlreadyHere);

    let esha = row(8);
    assert_eq!(esha.outcome, Outcome::Ready);
    assert_eq!(esha.joined_on, today);
    assert_eq!(esha.branch_name.as_deref(), Some("Hauz Khas"));
    assert!(esha.contacts.is_empty());
    for expected in [
        "\"12345\" isn't a phone number we can message, so it was left out.",
        "Couldn't read the date of birth \"31/02/2015\", so it was left out.",
        "Couldn't read the joining date \"next week\", so today was used.",
        "There's no branch called \"Delhi\", so Hauz Khas was used.",
        "No phone number, so reminders can't reach anyone yet.",
    ] {
        assert!(
            esha.notes.iter().any(|n| n == expected),
            "missing: {expected}\n{:?}",
            esha.notes
        );
    }
    assert_eq!(
        row(9).joined_on,
        today,
        "a future joining date becomes today"
    );
    assert_eq!(
        row(9).contacts[0].relation,
        Relation::Myself,
        "no birth date: an adult"
    );

    // Commit: exactly the ready rows.
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let done = import::commit(&mut tx, actor(&desk), &sheet, &options)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!((done.added, done.skipped), (4, 3));
    assert_eq!(count_students(&pool, &org).await, 5);

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let page = students::list(&mut tx, Default::default()).await.unwrap();
    let id_of = |name: &str| page.items.iter().find(|s| s.name == name).unwrap().id;
    let aarav = students::get(&mut tx, id_of("Aarav Sharma")).await.unwrap();
    assert_eq!(aarav.student.joined_on, date!(2024 - 04 - 03));
    assert!(
        aarav.student.entered_on > aarav.student.joined_on,
        "imported join dates are backdated"
    );
    assert_eq!(aarav.contacts[0].student_count, 2, "Anya shares her mother");
    assert_eq!(aarav.student.notes.as_deref(), Some("Shy at first"));
    let via: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE action IN ('student.created', 'students.imported')
           AND (action = 'students.imported' OR detail->>'via' = 'import')",
    )
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(via.iter().filter(|a| *a == "student.created").count(), 4);
    assert_eq!(via.iter().filter(|a| *a == "students.imported").count(), 1);
    tx.commit().await.unwrap();

    // The same file again adds nobody.
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let again = import::commit(&mut tx, actor(&desk), &sheet, &options)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(again.added, 0);
    assert_eq!(count_students(&pool, &org).await, 5);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn instructors_cant_import_and_studios_dont_see_each_other(pool: PgPool) {
    let (alpha, desk, teacher, _) = studio(&pool, "alpha").await;
    add_diya(&pool, &alpha, &desk).await;
    let (beta, beta_desk, _, beta_branch) = studio(&pool, "beta").await;
    let sheet = import::read_sheet(b"Name,Phone\nDiya Nair,9811100004\n").unwrap();

    let mut tx = TenantTx::begin(&pool, alpha.id).await.unwrap();
    let r = import::preview(&mut tx, actor(&teacher), &sheet, &ImportOptions::default()).await;
    assert!(matches!(r, Err(Error::Forbidden)), "{r:?}");
    drop(tx);

    // Diya is in alpha, not beta: for beta she's new.
    let mut tx = TenantTx::begin(&pool, beta.id).await.unwrap();
    let preview = import::preview(
        &mut tx,
        actor(&beta_desk),
        &sheet,
        &ImportOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(preview.rows[0].outcome, Outcome::Ready);
    assert!(
        preview.rows[0].notes.is_empty(),
        "{:?}",
        preview.rows[0].notes
    );
    // A branch that isn't this studio's, as the default, is refused.
    let r = import::preview(
        &mut tx,
        actor(&beta_desk),
        &sheet,
        &ImportOptions {
            branch_id: Some(Uuid::new_v4()),
        },
    )
    .await;
    assert!(matches!(r, Err(Error::Invalid(_))), "{r:?}");
    let ok = import::preview(
        &mut tx,
        actor(&beta_desk),
        &sheet,
        &ImportOptions {
            branch_id: Some(beta_branch),
        },
    )
    .await
    .unwrap();
    assert_eq!(ok.rows[0].branch_name.as_deref(), Some("Hauz Khas"));
}

#[test]
fn a_zip_bomb_is_refused_before_it_is_read() {
    // 40 MB of zeros compresses to a few kilobytes.
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("xl/worksheets/sheet1.xml", options).unwrap();
        let block = vec![b'0'; 1024 * 1024];
        for _ in 0..40 {
            zip.write_all(&block).unwrap();
        }
        zip.finish().unwrap();
    }
    let bytes = buf.into_inner();
    assert!(
        bytes.len() < 200 * 1024,
        "the bomb itself is small: {}",
        bytes.len()
    );
    let e = import::read_sheet(&bytes).unwrap_err();
    assert!(e.to_string().contains("unpacks to far more"), "{e}");
}

#[test]
fn far_apart_cells_are_refused_not_allocated() {
    let build = |far_down: bool| {
        let mut book = Workbook::new();
        let ws = book.add_worksheet();
        ws.write_string(0, 0, "Name").unwrap();
        ws.write_string(1, 0, "Aarav").unwrap();
        // A cell far to the right is ignored; one far below is too many rows.
        ws.write_string(1, 16_000, "far right").unwrap();
        if far_down {
            ws.write_string(1_000_000, 0, "far down").unwrap();
        }
        book.save_to_buffer().unwrap()
    };
    let ok = import::read_sheet(&build(false)).unwrap();
    assert!(ok.rows.iter().all(|(_, cells)| cells.len() <= 40));
    let e = import::read_sheet(&build(true)).unwrap_err();
    assert!(e.to_string().contains("more than 2,000"), "{e}");
}

#[test]
fn row_limits_are_exact() {
    let make = |n: usize| {
        let mut s = String::from("Name\n");
        for i in 0..n {
            s.push_str(&format!("Student {i}\n"));
        }
        s
    };
    let sheet = import::read_sheet(make(import::MAX_ROWS).as_bytes()).unwrap();
    assert_eq!(sheet.rows.len(), import::MAX_ROWS + 1);
    assert!(import::read_sheet(make(import::MAX_ROWS + 5).as_bytes()).is_err());
    let big = vec![b'a'; import::MAX_FILE_BYTES + 1];
    assert!(import::read_sheet(&big)
        .unwrap_err()
        .to_string()
        .contains("2 MB"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_template_we_hand_out_reads_back_exactly(pool: PgPool) {
    let (org, desk, _, _) = studio(&pool, "alpha").await;
    for bytes in [
        &include_bytes!("../../../web/public/templates/ncrm-students.xlsx")[..],
        &include_bytes!("../../../web/public/templates/ncrm-students.csv")[..],
    ] {
        let sheet = import::read_sheet(bytes).unwrap();
        let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
        let p = import::preview(&mut tx, actor(&desk), &sheet, &ImportOptions::default())
            .await
            .unwrap();
        assert!(p.ignored_columns.is_empty(), "{:?}", p.ignored_columns);
        assert_eq!(p.total, 1, "styled empty rows aren't students");
        let row = &p.rows[0];
        assert_eq!(row.name, "Aarav Sharma");
        assert_eq!(row.date_of_birth, Some(date!(2015 - 03 - 04)));
        assert_eq!(row.joined_on, date!(2024 - 04 - 03));
        assert_eq!(row.branch_name.as_deref(), Some("Hauz Khas"));
        assert_eq!(row.contacts[0].name, "Neha Sharma");
        assert_eq!(row.contacts[0].relation, Relation::Mother);
        assert!(row.notes.is_empty(), "{:?}", row.notes);
    }
}

/// A CSV file as bytes.
fn csv(lines: &[&str]) -> Vec<u8> {
    lines.join("\n").into_bytes()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_class_column_puts_students_in_classes_and_a_fee_column_prices_them(pool: PgPool) {
    use ncrm_core::batches::{self, NewBatch, Slot};
    use ncrm_core::plans::{self, NewPlan, PlanKind};
    use time::macros::time;

    let (org, desk, _, hauz_khas) = studio(&pool, "classes").await;
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let owner = staff::create(
        &mut tx,
        NewStaff {
            name: "Owner".into(),
            phone: "+919800000001".into(),
            role: StaffRole::Owner,
            branch_id: None,
        },
    )
    .await
    .unwrap();
    let noida: Uuid = sqlx::query_scalar("SELECT id FROM branches WHERE name = 'Noida 18'")
        .fetch_one(tx.conn())
        .await
        .unwrap();
    let class = |name: &str, branch_id, capacity| NewBatch {
        name: name.into(),
        style: None,
        branch_id,
        room_id: None,
        instructor_id: None,
        capacity,
        starts_on: None,
        ends_on: None,
        slots: vec![Slot {
            weekday: 1,
            starts_at: time!(18:00),
            ends_at: time!(19:00),
        }],
    };
    for new in [
        class("Hip-hop Beginners", hauz_khas, None),
        class("Kathak", hauz_khas, Some(1)),
        // The same name at two branches.
        class("Zumba", hauz_khas, None),
        class("Zumba", noida, None),
    ] {
        batches::create(&mut tx, actor(&owner), new).await.unwrap();
    }
    for (name, price_paise) in [("Twice a week", 250_000), ("Thrice a week", 320_000)] {
        plans::create(
            &mut tx,
            actor(&owner),
            NewPlan {
                name: name.into(),
                kind: PlanKind::Monthly,
                price_paise,
                classes: None,
                valid_days: None,
            },
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let sheet = import::read_sheet(&csv(&[
        "Name,Phone,Branch,Class,Fees",
        // Two classes, spelt loosely, priced by amount.
        "Aarav,9811100001,Hauz Khas,\"hip-hop  beginners, Kathak\",\"Rs. 2,500\"",
        // Kathak holds one, and Aarav has it.
        "Bela,9811100002,,Kathak,Thrice a week",
        // Zumba is at two branches: the row's branch decides.
        "Chirag,9811100003,Noida 18,Zumba,",
        // No branch, so we can't tell which Zumba. A class nobody has.
        "Dev,9811100004,,\"Zumba; Salsa\",",
        // A fee nobody charges, and a fee with no class.
        "Esha,9811100005,,Hip-hop Beginners,999",
        "Farhan,9811100006,,,2500",
    ]))
    .unwrap();

    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let preview = import::preview(&mut tx, actor(&desk), &sheet, &ImportOptions::default())
        .await
        .unwrap();
    assert!(
        preview.ignored_columns.is_empty(),
        "{:?}",
        preview.ignored_columns
    );
    assert_eq!(preview.ready, 6);
    let row = |name: &str| preview.rows.iter().find(|r| r.name == name).unwrap();
    assert_eq!(row("Aarav").classes, ["Hip-hop Beginners", "Kathak"]);
    assert_eq!(row("Aarav").fee.as_deref(), Some("Twice a week"));
    assert!(row("Aarav").notes.is_empty(), "{:?}", row("Aarav").notes);
    assert!(row("Bela").classes.is_empty());
    assert!(row("Bela").notes[0].contains("Kathak is full"));
    assert_eq!(row("Chirag").classes, ["Zumba"]);
    assert!(row("Dev").classes.is_empty());
    assert!(row("Dev")
        .notes
        .iter()
        .any(|n| n.contains("2 classes are called \"Zumba\"")));
    assert!(row("Dev")
        .notes
        .iter()
        .any(|n| n.contains("no class called \"Salsa\"")));
    assert_eq!(row("Esha").classes, ["Hip-hop Beginners"]);
    assert_eq!(row("Esha").fee, None);
    assert!(row("Esha").notes[0].contains("No monthly price matches \"999\""));
    assert!(row("Farhan")
        .notes
        .iter()
        .any(|n| n.contains("no class to charge it for")));
    // Looking changes nothing.
    assert_eq!(count_students(&pool, &org).await, 0);

    let done = import::commit(&mut tx, actor(&desk), &sheet, &ImportOptions::default())
        .await
        .unwrap();
    assert_eq!((done.added, done.placed), (6, 4));
    tx.commit().await.unwrap();

    // Who is where, and what they pay.
    let places: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT s.name, b.name, br.name, p.name
           FROM enrollments e
           JOIN students s ON s.id = e.student_id
           JOIN batches b ON b.id = e.batch_id
           JOIN branches br ON br.id = b.branch_id
           LEFT JOIN plans p ON p.id = e.plan_id
          WHERE e.org_id = $1
          ORDER BY s.name, b.name",
    )
    .bind(org.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    let as_strs: Vec<(&str, &str, &str, Option<&str>)> = places
        .iter()
        .map(|(a, b, c, d)| (a.as_str(), b.as_str(), c.as_str(), d.as_deref()))
        .collect();
    assert_eq!(
        as_strs,
        [
            (
                "Aarav",
                "Hip-hop Beginners",
                "Hauz Khas",
                Some("Twice a week")
            ),
            ("Aarav", "Kathak", "Hauz Khas", Some("Twice a week")),
            ("Chirag", "Zumba", "Noida 18", None),
            ("Esha", "Hip-hop Beginners", "Hauz Khas", None),
        ]
    );

    // Importing the same file again adds nobody and places nobody.
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    let again = import::commit(&mut tx, actor(&desk), &sheet, &ImportOptions::default())
        .await
        .unwrap();
    assert_eq!((again.added, again.placed), (0, 0));
}
