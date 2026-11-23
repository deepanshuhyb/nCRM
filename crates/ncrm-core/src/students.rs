//! Students and their contacts.
//!
//! A contact is someone with a phone number we message: an adult student
//! themselves (`relation = self`) or a parent or guardian. Contacts are
//! unique per phone within a studio, so adding a second child with the same
//! parent number links both children to one contact.
//!
//! Every change is written to the audit log in the same transaction.

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::{phone, Error, Result};

/// Most contacts one student can have.
pub const MAX_CONTACTS: usize = 4;
/// Most students one list page returns.
pub const MAX_PAGE: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "student_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum StudentStatus {
    Active,
    /// On a break; keeps their place but isn't expected in class.
    Paused,
    /// Stopped coming. Kept for history; never deleted by the app.
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "contact_relation", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// The student is an adult and is their own contact.
    #[sqlx(rename = "self")]
    #[serde(rename = "self")]
    Myself,
    Mother,
    Father,
    Guardian,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Student {
    pub id: Uuid,
    pub org_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub name: String,
    pub date_of_birth: Option<Date>,
    pub status: StudentStatus,
    pub notes: Option<String>,
    /// When they joined the studio. Staff can backdate it.
    pub joined_on: Date,
    /// The studio's date when this record was entered. `joined_on` earlier
    /// than this means the join date was backdated.
    pub entered_on: Date,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// A contact as linked to one student.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct ContactLink {
    pub contact_id: Uuid,
    pub name: String,
    pub phone: String,
    pub relation: Relation,
    pub is_primary: bool,
    /// How many students this contact is linked to (siblings).
    pub student_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StudentDetail {
    #[serde(flatten)]
    pub student: Student,
    pub contacts: Vec<ContactLink>,
}

/// One row of the students list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct StudentListItem {
    pub id: Uuid,
    pub name: String,
    pub status: StudentStatus,
    pub branch_id: Option<Uuid>,
    pub branch_name: Option<String>,
    pub joined_on: Date,
    pub primary_contact_name: Option<String>,
    pub primary_contact_phone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewContact {
    pub name: String,
    /// As typed; normalized before saving.
    pub phone: String,
    pub relation: Relation,
    #[serde(default)]
    pub is_primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewStudent {
    pub name: String,
    pub date_of_birth: Option<Date>,
    pub branch_id: Option<Uuid>,
    pub notes: Option<String>,
    /// `None` means today. An earlier date backdates the join.
    #[serde(default)]
    pub joined_on: Option<Date>,
    #[serde(default)]
    pub contacts: Vec<NewContact>,
}

/// The full editable profile, sent whole (PUT).
#[derive(Debug, Clone, Deserialize)]
pub struct StudentEdit {
    pub name: String,
    pub date_of_birth: Option<Date>,
    pub branch_id: Option<Uuid>,
    pub status: StudentStatus,
    pub notes: Option<String>,
    pub joined_on: Date,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    /// Part of a name, or at least 4 digits of a contact's phone.
    pub q: Option<String>,
    pub status: Option<StudentStatus>,
    pub branch_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const STUDENT_COLUMNS: &str =
    "id, org_id, branch_id, name, date_of_birth, status, notes, joined_on,
     studio_date(org_id, created_at) AS entered_on, created_at, updated_at";

/// Add a student with their contacts.
pub async fn create(tx: &mut TenantTx, actor: Actor, new: NewStudent) -> Result<StudentDetail> {
    let id = create_via(tx, actor, new, "form").await?;
    get(tx, id).await
}

/// Create a student and return its id. `via` ("form", "import") is
/// recorded in the audit log.
pub(crate) async fn create_via(
    tx: &mut TenantTx,
    actor: Actor,
    new: NewStudent,
    via: &str,
) -> Result<Uuid> {
    actor.require(Capability::ManageStudents)?;
    let name = clean_name(&new.name)?;
    let today = tx.studio_today().await?;
    let joined_on = new.joined_on.unwrap_or(today);
    check_dates(new.date_of_birth, joined_on, today)?;
    let notes = clean_notes(new.notes)?;
    let contacts = clean_contacts(new.contacts)?;

    let org_id = tx.org_id();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO students (org_id, branch_id, name, date_of_birth, notes, joined_on)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(org_id)
    .bind(new.branch_id)
    .bind(&name)
    .bind(new.date_of_birth)
    .bind(notes)
    .bind(joined_on)
    .fetch_one(tx.conn())
    .await?;

    for c in &contacts {
        link_contact(tx, id, c).await?;
    }

    let mut detail = json!({ "contacts": contacts.len(), "via": via });
    if joined_on < today {
        detail["backdated"] = json!(true);
    }
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "student.created".into(),
            entity: "student".into(),
            entity_id: Some(id),
            detail,
        },
    )
    .await?;
    Ok(id)
}

/// One student with their contacts.
pub async fn get(tx: &mut TenantTx, id: Uuid) -> Result<StudentDetail> {
    let student = sqlx::query_as::<_, Student>(sqlx::AssertSqlSafe(format!(
        "SELECT {STUDENT_COLUMNS} FROM students WHERE id = $1"
    )))
    .bind(id)
    .fetch_one(tx.conn())
    .await?;

    let contacts = sqlx::query_as::<_, ContactLink>(
        "SELECT c.id AS contact_id, c.name, c.phone, sc.relation, sc.is_primary,
                (SELECT count(*) FROM student_contacts x
                  WHERE x.org_id = c.org_id AND x.contact_id = c.id) AS student_count
           FROM student_contacts sc
           JOIN contacts c ON c.org_id = sc.org_id AND c.id = sc.contact_id
          WHERE sc.student_id = $1
          ORDER BY sc.is_primary DESC, sc.created_at, c.id",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;

    Ok(StudentDetail { student, contacts })
}

/// Search and page through students.
pub async fn list(tx: &mut TenantTx, query: ListQuery) -> Result<Page<StudentListItem>> {
    let limit = query.limit.unwrap_or(50).clamp(1, MAX_PAGE);
    let offset = query.offset.unwrap_or(0).clamp(0, 100_000);

    let q = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if q.is_some_and(|s| s.chars().count() > 80) {
        return Err(Error::Invalid("Search for at most 80 characters.".into()));
    }
    // Wildcards typed by the person are matched literally.
    let name_pattern = q.map(|s| escape_like(&s.to_lowercase()));
    let digits: Option<String> = q
        .map(|s| s.chars().filter(char::is_ascii_digit).collect::<String>())
        .filter(|d| d.len() >= 4);

    const FILTER: &str = "
        ($1::text IS NULL
          OR lower(s.name) LIKE '%' || $1 || '%' ESCAPE '\\'
          OR ($2::text IS NOT NULL AND EXISTS (
                SELECT 1 FROM student_contacts x
                  JOIN contacts c ON c.org_id = x.org_id AND c.id = x.contact_id
                 WHERE x.org_id = s.org_id AND x.student_id = s.id
                   AND c.phone LIKE '%' || $2 || '%')))
        AND ($3::student_status IS NULL OR s.status = $3)
        AND ($4::uuid IS NULL OR s.branch_id = $4)";

    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM students s WHERE {FILTER}"
    )))
    .bind(&name_pattern)
    .bind(&digits)
    .bind(query.status)
    .bind(query.branch_id)
    .fetch_one(tx.conn())
    .await?;

    let items = sqlx::query_as::<_, StudentListItem>(sqlx::AssertSqlSafe(format!(
        "SELECT s.id, s.name, s.status, s.branch_id, b.name AS branch_name, s.joined_on,
                pc.name AS primary_contact_name, pc.phone AS primary_contact_phone
           FROM students s
           LEFT JOIN branches b ON b.org_id = s.org_id AND b.id = s.branch_id
           LEFT JOIN student_contacts sc
                  ON sc.org_id = s.org_id AND sc.student_id = s.id AND sc.is_primary
           LEFT JOIN contacts pc ON pc.org_id = sc.org_id AND pc.id = sc.contact_id
          WHERE {FILTER}
          ORDER BY lower(s.name), s.id
          LIMIT $5 OFFSET $6"
    )))
    .bind(&name_pattern)
    .bind(&digits)
    .bind(query.status)
    .bind(query.branch_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(tx.conn())
    .await?;

    Ok(Page {
        items,
        total,
        limit,
        offset,
    })
}

/// Replace a student's profile.
pub async fn update(
    tx: &mut TenantTx,
    actor: Actor,
    id: Uuid,
    edit: StudentEdit,
) -> Result<StudentDetail> {
    actor.require(Capability::ManageStudents)?;
    let name = clean_name(&edit.name)?;
    let today = tx.studio_today().await?;
    check_dates(edit.date_of_birth, edit.joined_on, today)?;
    let notes = clean_notes(edit.notes)?;

    let before = get(tx, id).await?.student;
    sqlx::query(
        "UPDATE students
            SET name = $2, date_of_birth = $3, branch_id = $4, status = $5, notes = $6,
                joined_on = $7, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&name)
    .bind(edit.date_of_birth)
    .bind(edit.branch_id)
    .bind(edit.status)
    .bind(&notes)
    .bind(edit.joined_on)
    .execute(tx.conn())
    .await?;

    // Someone who leaves the studio leaves their classes too. (On a break,
    // they keep their place.)
    let mut classes_left = 0;
    if edit.status == StudentStatus::Left && before.status != StudentStatus::Left {
        classes_left = crate::enrollments::end_all_for_student(tx, id).await?;
    }

    let mut changed = Vec::new();
    let mut detail = serde_json::Map::new();
    if before.name != name {
        changed.push("name");
    }
    if before.date_of_birth != edit.date_of_birth {
        changed.push("date_of_birth");
    }
    if before.branch_id != edit.branch_id {
        changed.push("branch");
    }
    if before.status != edit.status {
        changed.push("status");
        detail.insert(
            "status".into(),
            json!({ "from": before.status, "to": edit.status }),
        );
    }
    if before.notes != notes {
        changed.push("notes");
    }
    if before.joined_on != edit.joined_on {
        changed.push("joined_on");
    }
    if classes_left > 0 {
        detail.insert("classes_left".into(), json!(classes_left));
    }
    detail.insert("changed".into(), json!(changed));
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "student.updated".into(),
            entity: "student".into(),
            entity_id: Some(id),
            detail: serde_json::Value::Object(detail),
        },
    )
    .await?;
    get(tx, id).await
}

/// Link a contact to a student. An existing contact with the same number is
/// reused (and keeps its name), which is how siblings share a parent.
pub async fn add_contact(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    contact: NewContact,
) -> Result<StudentDetail> {
    actor.require(Capability::ManageStudents)?;
    let mut cleaned = clean_contacts(vec![contact])?;
    let mut contact = cleaned.remove(0);

    let existing = get(tx, student_id).await?;
    if existing.contacts.len() >= MAX_CONTACTS {
        return Err(Error::Invalid(format!(
            "A student can have at most {MAX_CONTACTS} contacts."
        )));
    }
    if existing.contacts.iter().any(|c| c.phone == contact.phone) {
        return Err(Error::Conflict(
            "That number is already a contact for this student.".into(),
        ));
    }
    if existing.contacts.is_empty() {
        contact.is_primary = true;
    }
    if contact.is_primary {
        sqlx::query("UPDATE student_contacts SET is_primary = false WHERE student_id = $1")
            .bind(student_id)
            .execute(tx.conn())
            .await?;
    }
    link_contact(tx, student_id, &contact).await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "student.contact_added".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            detail: json!({ "relation": contact.relation, "primary": contact.is_primary }),
        },
    )
    .await?;
    get(tx, student_id).await
}

/// Unlink a contact from a student. If it was the primary contact, the
/// longest-linked remaining contact becomes primary.
pub async fn remove_contact(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    contact_id: Uuid,
) -> Result<StudentDetail> {
    actor.require(Capability::ManageStudents)?;
    let removed: Option<bool> = sqlx::query_scalar(
        "DELETE FROM student_contacts WHERE student_id = $1 AND contact_id = $2
         RETURNING is_primary",
    )
    .bind(student_id)
    .bind(contact_id)
    .fetch_optional(tx.conn())
    .await?;
    let was_primary = removed.ok_or(Error::NotFound)?;

    if was_primary {
        sqlx::query(
            "UPDATE student_contacts SET is_primary = true
              WHERE (student_id, contact_id) = (
                    SELECT student_id, contact_id FROM student_contacts
                     WHERE student_id = $1 ORDER BY created_at, contact_id LIMIT 1)",
        )
        .bind(student_id)
        .execute(tx.conn())
        .await?;
    }

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "student.contact_removed".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            detail: json!({ "contact_id": contact_id }),
        },
    )
    .await?;
    get(tx, student_id).await
}

/// Count of students by status, for the home screen.
pub async fn count_active(tx: &mut TenantTx) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM students WHERE status = 'active'")
        .fetch_one(tx.conn())
        .await?;
    Ok(n)
}

async fn link_contact(tx: &mut TenantTx, student_id: Uuid, c: &NewContact) -> Result<()> {
    let org_id = tx.org_id();
    // Reuse the contact if this number already exists in the studio. The
    // no-op update makes RETURNING give back the existing row.
    let contact_id: Uuid = sqlx::query_scalar(
        "INSERT INTO contacts (org_id, name, phone) VALUES ($1, $2, $3)
         ON CONFLICT (org_id, phone) DO UPDATE SET phone = EXCLUDED.phone
         RETURNING id",
    )
    .bind(org_id)
    .bind(c.name.trim())
    .bind(&c.phone)
    .fetch_one(tx.conn())
    .await?;

    sqlx::query(
        "INSERT INTO student_contacts (org_id, student_id, contact_id, relation, is_primary)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(org_id)
    .bind(student_id)
    .bind(contact_id)
    .bind(c.relation)
    .bind(c.is_primary)
    .execute(tx.conn())
    .await?;
    Ok(())
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    let len = name.chars().count();
    if len == 0 || len > 120 {
        return Err(Error::Invalid("Names must be 1 to 120 characters.".into()));
    }
    Ok(name.to_string())
}

fn clean_notes(notes: Option<String>) -> Result<Option<String>> {
    let notes = notes
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    if notes.as_ref().is_some_and(|n| n.chars().count() > 2000) {
        return Err(Error::Invalid(
            "Notes can be at most 2000 characters.".into(),
        ));
    }
    Ok(notes)
}

/// Birth and join dates that could be real, judged by the studio's today.
pub(crate) fn check_dates(dob: Option<Date>, joined_on: Date, today: Date) -> Result<()> {
    if let Some(d) = dob {
        if d > today || d.year() < 1900 {
            return Err(Error::Invalid("That date of birth isn't possible.".into()));
        }
        if joined_on < d {
            return Err(Error::Invalid(
                "The joining date can't be before they were born.".into(),
            ));
        }
    }
    if joined_on > today {
        return Err(Error::Invalid(
            "The joining date can't be in the future.".into(),
        ));
    }
    if joined_on.year() < 2000 {
        return Err(Error::Invalid(
            "The joining date has to be in 2000 or later.".into(),
        ));
    }
    Ok(())
}

/// Normalize phones, check limits, and make sure exactly one contact is
/// primary when there are any.
fn clean_contacts(contacts: Vec<NewContact>) -> Result<Vec<NewContact>> {
    if contacts.len() > MAX_CONTACTS {
        return Err(Error::Invalid(format!(
            "A student can have at most {MAX_CONTACTS} contacts."
        )));
    }
    let mut out: Vec<NewContact> = Vec::with_capacity(contacts.len());
    for c in contacts {
        let name = clean_name(&c.name)?;
        let phone = phone::normalize(&c.phone)?;
        if out.iter().any(|o| o.phone == phone) {
            return Err(Error::Invalid("The same number is listed twice.".into()));
        }
        out.push(NewContact {
            name,
            phone,
            relation: c.relation,
            is_primary: c.is_primary,
        });
    }
    let primaries = out.iter().filter(|c| c.is_primary).count();
    if primaries > 1 {
        return Err(Error::Invalid(
            "Only one contact can be the main one.".into(),
        ));
    }
    if primaries == 0 {
        if let Some(first) = out.first_mut() {
            first.is_primary = true;
        }
    }
    Ok(out)
}

/// Escape `\`, `%` and `_` so they match literally in a LIKE pattern.
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(phone: &str, primary: bool) -> NewContact {
        NewContact {
            name: "Parent".into(),
            phone: phone.into(),
            relation: Relation::Mother,
            is_primary: primary,
        }
    }

    #[test]
    fn like_wildcards_are_escaped() {
        assert_eq!(escape_like("100%_a\\b"), "100\\%\\_a\\\\b");
    }

    #[test]
    fn the_first_contact_becomes_primary_when_none_is_chosen() {
        let out = clean_contacts(vec![
            contact("9876543210", false),
            contact("9876543211", false),
        ])
        .unwrap();
        assert!(out[0].is_primary);
        assert!(!out[1].is_primary);
        assert_eq!(out[0].phone, "+919876543210");
    }

    #[test]
    fn contact_lists_are_checked() {
        assert!(clean_contacts(vec![
            contact("9876543210", true),
            contact("9876543211", true)
        ])
        .is_err());
        assert!(clean_contacts(vec![
            contact("9876543210", false),
            contact("+91 98765 43210", false)
        ])
        .is_err());
        let five = (0..5)
            .map(|i| contact(&format!("987654321{i}"), false))
            .collect();
        assert!(clean_contacts(five).is_err());
    }

    #[test]
    fn impossible_birth_and_join_dates_are_refused() {
        let today = Date::from_calendar_date(2026, time::Month::November, 23).unwrap();
        let day = |y, m, d| Date::from_calendar_date(y, m, d).unwrap();
        use time::Month::*;
        let tomorrow = today.next_day().unwrap();
        assert!(check_dates(Some(tomorrow), today, today).is_err());
        assert!(check_dates(Some(day(1899, December, 31)), today, today).is_err());
        assert!(check_dates(Some(day(2015, June, 1)), today, today).is_ok());
        assert!(check_dates(None, today, today).is_ok());
        // Backdating is fine; the future and the distant past aren't.
        assert!(check_dates(None, day(2024, March, 3), today).is_ok());
        assert!(check_dates(None, tomorrow, today).is_err());
        assert!(check_dates(None, day(1999, December, 31), today).is_err());
        // Joining before being born isn't.
        assert!(check_dates(Some(day(2015, June, 1)), day(2014, June, 1), today).is_err());
    }

    #[test]
    fn dates_travel_as_plain_iso_strings() {
        let d = Date::from_calendar_date(2015, time::Month::June, 1).unwrap();
        assert_eq!(serde_json::to_string(&d).unwrap(), "\"2015-06-01\"");
        let back: Date = serde_json::from_str("\"2015-06-01\"").unwrap();
        assert_eq!(back, d);
    }
}
