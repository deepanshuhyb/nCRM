//! Bringing an existing student list in from a spreadsheet (.xlsx or .csv).
//!
//! Two steps read the same file. `preview` says what would happen to every
//! row and writes nothing. `commit` does exactly that, in one transaction,
//! so an import either happens completely or not at all. Rows already in
//! the studio are skipped, so importing the same file twice adds nobody
//! the second time.
//!
//! A file from a stranger is hostile until read. The guards:
//! - at most 2 MB uploaded and 2,000 students per file;
//! - an .xlsx file is a zip. Before the spreadsheet reader touches it, every
//!   entry is actually unpacked (and thrown away) with a running cap of
//!   30 MB, so a "zip bomb" (a small file that unpacks to gigabytes) is
//!   refused. Declared sizes inside a zip can lie; the bytes read can't;
//! - cells are read one at a time (`worksheet_cells_reader`) and anything
//!   beyond 40 columns or 2,010 rows is a refusal, never a dense grid.
//!   Reading a whole sheet as a grid would let two cells at A1 and XFD1048576
//!   allocate billions of empty cells;
//! - old binary .xls files (and password-protected .xlsx, which use the
//!   same container) are refused with a plain way out;
//! - everything is checked by the same rules as the "Add a student" form.
//!
//! Reading the file is pure CPU, so the API runs `read_sheet` on a blocking
//! thread.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Cursor, Read};

use calamine::{DataRef, ExcelDateTime, Reader, Xlsx, XlsxError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, Duration, Month};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::students::{self, NewContact, NewStudent, Relation};
use crate::{phone, Error, Result};

/// Largest file accepted.
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
/// Most students in one file.
pub const MAX_ROWS: usize = 2000;
const MAX_COLUMNS: usize = 40;
/// The column titles must be in one of the first rows.
const HEADER_SEARCH_ROWS: usize = 10;
const MAX_UNZIPPED_BYTES: u64 = 30 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 1000;
const MAX_SHEETS: usize = 20;
const MAX_CELL_CHARS: usize = 2000;

// ------------------------------------------------------------- reading

/// One cell, reduced to what a student list needs.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Empty,
    Text(String),
    Number(f64),
    Date(Date),
}

impl Value {
    fn is_empty(&self) -> bool {
        matches!(self, Value::Empty)
    }

    /// The cell as a person would read it, for messages.
    fn shown(&self) -> String {
        match self {
            Value::Empty => String::new(),
            Value::Text(t) => t.clone(),
            Value::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{n:.0}"),
            Value::Number(n) => n.to_string(),
            Value::Date(d) => d.to_string(),
        }
    }
}

/// A sheet as rows of cells. Row numbers are the spreadsheet's own
/// (1 = the first row), so messages can say "row 14".
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub name: Option<String>,
    pub rows: Vec<(u32, Vec<Value>)>,
}

fn refuse(message: &str) -> Error {
    Error::Invalid(message.into())
}

const UNREADABLE: &str =
    "We couldn't read that file. Open it in Excel, save it again as .xlsx, and upload that.";
const TOO_MANY_ROWS: &str =
    "That sheet has more than 2,000 students. Split it into smaller files and import them one by one.";

/// Read an uploaded file into rows. Works out the kind of file from its
/// first bytes, not from its name or the type the browser claimed.
pub fn read_sheet(bytes: &[u8]) -> Result<Sheet> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(refuse(
            "That file is larger than 2 MB. Split it into smaller files.",
        ));
    }
    if bytes.is_empty() {
        return Err(refuse("That file is empty."));
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return read_xlsx(bytes);
    }
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err(refuse(
            "This is an old .xls file, or one with a password. Open it in Excel, save it as .xlsx without a password, and upload that.",
        ));
    }
    read_csv(bytes)
}

/// Unpack every entry, counting real bytes, and refuse past the cap.
fn check_zip(bytes: &[u8]) -> Result<()> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| refuse(UNREADABLE))?;
    if zip.len() > MAX_ZIP_ENTRIES {
        return Err(refuse(UNREADABLE));
    }
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|_| refuse(UNREADABLE))?;
        let room = MAX_UNZIPPED_BYTES - total;
        let read = std::io::copy(&mut (&mut entry).take(room + 1), &mut std::io::sink())
            .map_err(|_| refuse(UNREADABLE))?;
        total += read;
        if total > MAX_UNZIPPED_BYTES {
            return Err(refuse(
                "That file unpacks to far more than a student list would. Save a smaller copy with just the students and upload that.",
            ));
        }
    }
    Ok(())
}

fn read_xlsx(bytes: &[u8]) -> Result<Sheet> {
    check_zip(bytes)?;
    let mut book: Xlsx<_> = Xlsx::new(Cursor::new(bytes)).map_err(|e| match e {
        XlsxError::Password => {
            refuse("That file has a password. Save a copy without one and upload that.")
        }
        _ => refuse(UNREADABLE),
    })?;
    let names: Vec<String> = book.sheet_names().into_iter().take(MAX_SHEETS).collect();
    for name in names {
        let rows = read_xlsx_rows(&mut book, &name)?;
        if let Some(header) = find_header(&rows) {
            check_row_count(&rows, &header)?;
            return Ok(Sheet {
                name: Some(name),
                rows,
            });
        }
    }
    Err(refuse(NO_NAME_COLUMN))
}

fn read_xlsx_rows<RS: Read + std::io::Seek>(
    book: &mut Xlsx<RS>,
    name: &str,
) -> Result<Vec<(u32, Vec<Value>)>> {
    let mut reader = book
        .worksheet_cells_reader(name)
        .map_err(|_| refuse(UNREADABLE))?;
    let last_row = (HEADER_SEARCH_ROWS + MAX_ROWS) as u32;
    let mut rows: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
    while let Some(cell) = reader.next_cell().map_err(|_| refuse(UNREADABLE))? {
        let (row, col) = cell.get_position();
        let value = from_cell(cell.get_value());
        if value.is_empty() || col as usize >= MAX_COLUMNS {
            continue;
        }
        if row > last_row {
            return Err(refuse(TOO_MANY_ROWS));
        }
        let cells = rows.entry(row).or_default();
        if cells.len() <= col as usize {
            cells.resize(col as usize + 1, Value::Empty);
        }
        cells[col as usize] = value;
    }
    Ok(rows.into_iter().map(|(r, cells)| (r + 1, cells)).collect())
}

fn from_cell(v: &DataRef<'_>) -> Value {
    match v {
        DataRef::Int(i) => Value::Number(*i as f64),
        DataRef::Float(f) => Value::Number(*f),
        DataRef::String(s) => text(s),
        DataRef::SharedString(s) => text(s),
        DataRef::Bool(b) => text(if *b { "TRUE" } else { "FALSE" }),
        DataRef::DateTime(dt) if dt.is_datetime() => {
            excel_date(dt).map(Value::Date).unwrap_or(Value::Empty)
        }
        DataRef::DateTimeIso(s) => text(s),
        _ => Value::Empty,
    }
}

fn text(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() {
        Value::Empty
    } else {
        Value::Text(t.chars().take(MAX_CELL_CHARS).collect())
    }
}

fn excel_date(dt: &ExcelDateTime) -> Option<Date> {
    let (y, m, d, ..) = dt.to_ymd_hms_milli();
    Date::from_calendar_date(i32::from(y), Month::try_from(m).ok()?, d).ok()
}

fn read_csv(bytes: &[u8]) -> Result<Sheet> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let content = std::str::from_utf8(bytes).map_err(|_| {
        refuse("Save the file as \"CSV UTF-8\" in Excel, or upload the .xlsx file itself.")
    })?;
    if content.contains('\0') {
        return Err(refuse(
            "That doesn't look like a spreadsheet. Upload a .xlsx or .csv file.",
        ));
    }
    // Excel writes semicolons or tabs instead of commas in some regions.
    let first = content.lines().next().unwrap_or("");
    let count = |c: char| first.matches(c).count();
    let delimiter = if count(';') > count(',') && count(';') >= count('\t') {
        b';'
    } else if count('\t') > count(',') {
        b'\t'
    } else {
        b','
    };

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(content.as_bytes());
    let last_row = HEADER_SEARCH_ROWS + MAX_ROWS + 1;
    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|_| {
            Error::Invalid(format!(
                "Line {} of the file can't be read. Check its quotes.",
                i + 1
            ))
        })?;
        let cells: Vec<Value> = record.iter().take(MAX_COLUMNS).map(text).collect();
        if cells.iter().all(Value::is_empty) {
            continue;
        }
        if i >= last_row {
            return Err(refuse(TOO_MANY_ROWS));
        }
        rows.push(((i + 1) as u32, cells));
    }
    let header = find_header(&rows).ok_or_else(|| refuse(NO_NAME_COLUMN))?;
    check_row_count(&rows, &header)?;
    Ok(Sheet { name: None, rows })
}

fn check_row_count(rows: &[(u32, Vec<Value>)], header: &Header) -> Result<()> {
    if rows.len() - header.index - 1 > MAX_ROWS {
        return Err(refuse(TOO_MANY_ROWS));
    }
    Ok(())
}

// ------------------------------------------------------- column titles

const NO_NAME_COLUMN: &str = "We couldn't find a column called Name. Put column titles in the first row, like Name, Phone and Joined on.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Field {
    Name,
    DateOfBirth,
    Branch,
    JoinedOn,
    Notes,
    Phone,
    ContactName,
    Relation,
    MotherPhone,
    MotherName,
    FatherPhone,
    FatherName,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::Name => "Name",
            Field::DateOfBirth => "Date of birth",
            Field::Branch => "Branch",
            Field::JoinedOn => "Joined on",
            Field::Notes => "Notes",
            Field::Phone => "Phone",
            Field::ContactName => "Contact name",
            Field::Relation => "Relation",
            Field::MotherPhone => "Mother's phone",
            Field::MotherName => "Mother's name",
            Field::FatherPhone => "Father's phone",
            Field::FatherName => "Father's name",
        }
    }
}

/// Titles people actually use, after `normalize_title`.
const TITLES: &[(Field, &[&str])] = &[
    (
        Field::Name,
        &[
            "name",
            "student name",
            "student",
            "full name",
            "students name",
            "name of student",
            "child name",
            "childs name",
            "student full name",
        ],
    ),
    (
        Field::DateOfBirth,
        &[
            "date of birth",
            "dob",
            "d o b",
            "birth date",
            "birthday",
            "birthdate",
        ],
    ),
    (
        Field::Branch,
        &["branch", "centre", "center", "location", "branch name"],
    ),
    (
        Field::JoinedOn,
        &[
            "joined on",
            "joined",
            "joining date",
            "date of joining",
            "doj",
            "admission date",
            "date of admission",
            "start date",
            "joined date",
            "join date",
        ],
    ),
    (
        Field::Notes,
        &["notes", "note", "remarks", "remark", "comments", "comment"],
    ),
    (
        Field::Phone,
        &[
            "phone",
            "mobile",
            "whatsapp",
            "phone number",
            "mobile number",
            "whatsapp number",
            "contact number",
            "contact no",
            "mobile no",
            "phone no",
            "contact",
            "number",
        ],
    ),
    (
        Field::ContactName,
        &[
            "contact name",
            "parent",
            "parent name",
            "parents name",
            "guardian",
            "guardian name",
        ],
    ),
    (
        Field::Relation,
        &["relation", "relationship", "relation to student"],
    ),
    (
        Field::MotherPhone,
        &[
            "mother phone",
            "mothers phone",
            "mother mobile",
            "mothers mobile",
            "mother number",
            "mothers number",
            "mother whatsapp",
            "mothers whatsapp",
            "mother contact",
            "mother phone no",
            "mothers phone no",
            "mother mobile no",
            "mothers mobile no",
        ],
    ),
    (
        Field::MotherName,
        &["mother", "mother name", "mothers name"],
    ),
    (
        Field::FatherPhone,
        &[
            "father phone",
            "fathers phone",
            "father mobile",
            "fathers mobile",
            "father number",
            "fathers number",
            "father whatsapp",
            "fathers whatsapp",
            "father contact",
            "father phone no",
            "fathers phone no",
            "father mobile no",
            "fathers mobile no",
        ],
    ),
    (
        Field::FatherName,
        &["father", "father name", "fathers name"],
    ),
];

/// "Mother's Phone No." → "mothers phone no".
fn normalize_title(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| *c != '\'' && *c != '’')
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn field_for(title: &str) -> Option<Field> {
    let t = normalize_title(title);
    TITLES
        .iter()
        .find(|(_, names)| names.contains(&t.as_str()))
        .map(|(f, _)| *f)
}

struct Header {
    /// Index into `Sheet::rows` of the title row.
    index: usize,
    columns: HashMap<usize, Field>,
    used: Vec<ColumnUse>,
    ignored: Vec<String>,
}

fn find_header(rows: &[(u32, Vec<Value>)]) -> Option<Header> {
    for (index, (_, cells)) in rows.iter().enumerate().take(HEADER_SEARCH_ROWS) {
        let titles: Vec<String> = cells.iter().map(Value::shown).collect();
        if !titles.iter().any(|t| field_for(t) == Some(Field::Name)) {
            continue;
        }
        let mut columns = HashMap::new();
        let mut seen = HashSet::new();
        let mut used = Vec::new();
        let mut ignored = Vec::new();
        for (col, title) in titles.iter().enumerate() {
            if title.is_empty() {
                continue;
            }
            match field_for(title) {
                Some(field) if seen.insert(field) => {
                    columns.insert(col, field);
                    used.push(ColumnUse {
                        title: title.clone(),
                        used_as: field.label().into(),
                    });
                }
                _ => ignored.push(title.clone()),
            }
        }
        return Some(Header {
            index,
            columns,
            used,
            ignored,
        });
    }
    None
}

// ----------------------------------------------------- reading values

/// Dates the way people type them in India: day first.
fn parse_date(value: &Value, today: Date) -> Option<Date> {
    match value {
        Value::Date(d) => Some(*d),
        // A date cell that lost its format is a serial day count since
        // 1899-12-30 (Excel's 1900 calendar, after its leap-year bug).
        Value::Number(n) if (61.0..80_000.0).contains(n) => {
            let base = Date::from_calendar_date(1899, Month::December, 30).ok()?;
            base.checked_add(Duration::days(n.floor() as i64))
        }
        Value::Text(t) => parse_date_text(t, today),
        _ => None,
    }
}

fn parse_date_text(raw: &str, today: Date) -> Option<Date> {
    // "2015-03-04 00:00:00" → keep the date.
    let raw = raw
        .split(['T', ' '])
        .next()
        .filter(|first| first.len() >= 8)
        .unwrap_or(raw);
    let parts: Vec<&str> = raw
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 3 {
        return None;
    }
    let month_name = |p: &str| -> Option<u8> {
        let p = p.to_ascii_lowercase();
        let names = [
            "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
        ];
        if p.len() < 3 || !p.chars().all(|c| c.is_ascii_alphabetic()) {
            return None;
        }
        names
            .iter()
            .position(|n| p.starts_with(n))
            .map(|i| i as u8 + 1)
    };
    let num = |p: &str| p.parse::<i32>().ok();
    let year = |y: i32, digits: usize| -> i32 {
        if digits <= 2 {
            let this = today.year() % 100;
            if y <= this {
                2000 + y
            } else {
                1900 + y
            }
        } else {
            y
        }
    };
    let make = |y: i32, m: i32, d: i32| -> Option<Date> {
        Date::from_calendar_date(
            y,
            Month::try_from(u8::try_from(m).ok()?).ok()?,
            u8::try_from(d).ok()?,
        )
        .ok()
    };

    let (a, b, c) = (parts[0], parts[1], parts[2]);
    // 4 Mar 2015 / 4-March-15
    if let Some(m) = month_name(b) {
        return make(year(num(c)?, c.len()), m.into(), num(a)?);
    }
    // Mar 4 2015
    if let Some(m) = month_name(a) {
        return make(year(num(c)?, c.len()), m.into(), num(b)?);
    }
    let (na, nb, nc) = (num(a)?, num(b)?, num(c)?);
    // 2015-03-04
    if a.len() == 4 {
        return make(na, nb, nc);
    }
    let y = year(nc, c.len());
    // 04/03/2015 is 4 March; 03/25/2015 can only be 25 March.
    make(y, nb, na).or_else(|| make(y, na, nb))
}

fn parse_phone(value: &Value) -> Option<std::result::Result<String, String>> {
    let raw = match value {
        Value::Empty => return None,
        Value::Number(n) if n.fract() == 0.0 && *n > 0.0 && *n < 1e16 => format!("{n:.0}"),
        other => other.shown(),
    };
    // "98765 43210 / 98765 43211": the first number.
    let first = raw
        .split(['/', ',', ';', '&'])
        .next()
        .unwrap_or("")
        .split(" or ")
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if first.is_empty() {
        return None;
    }
    Some(phone::normalize(&first).map_err(|_| raw.clone()))
}

fn relation_word(r: Relation) -> &'static str {
    match r {
        Relation::Myself => "Self",
        Relation::Mother => "Mother",
        Relation::Father => "Father",
        Relation::Guardian => "Guardian",
        Relation::Other => "Contact",
    }
}

/// Whole years from a birth date to `on`.
fn age_on(born: Date, on: Date) -> i32 {
    let mut age = on.year() - born.year();
    if (on.month() as u8, on.day()) < (born.month() as u8, born.day()) {
        age -= 1;
    }
    age
}

fn parse_relation(value: &Value) -> Option<Relation> {
    let t = normalize_title(&value.shown());
    if t.is_empty() {
        return None;
    }
    Some(match t.as_str() {
        "mother" | "mom" | "mum" | "mummy" | "maa" | "ma" | "mama" => Relation::Mother,
        "father" | "dad" | "papa" | "daddy" | "pa" => Relation::Father,
        "guardian" | "legal guardian" => Relation::Guardian,
        "self" | "student" | "own" | "themselves" | "myself" => Relation::Myself,
        _ => Relation::Other,
    })
}

// ------------------------------------------------------------- planning

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImportOptions {
    /// Where students go when their row names no branch.
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColumnUse {
    /// The title in the file.
    pub title: String,
    /// What we read it as.
    pub used_as: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Will be added.
    Ready,
    /// Already in the studio; skipped.
    AlreadyHere,
    /// The same student appears earlier in the file; skipped.
    Repeated,
    /// Can't be added as it is (no name); skipped.
    CannotImport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewContact {
    pub name: String,
    pub phone: String,
    pub relation: Relation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewRow {
    /// The row number in the spreadsheet.
    pub row: u32,
    pub name: String,
    pub outcome: Outcome,
    /// Anything worth knowing: what was left out or changed, and why.
    pub notes: Vec<String>,
    pub branch_name: Option<String>,
    pub joined_on: Date,
    pub date_of_birth: Option<Date>,
    pub contacts: Vec<PreviewContact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub sheet_name: Option<String>,
    pub columns: Vec<ColumnUse>,
    pub ignored_columns: Vec<String>,
    pub total: usize,
    pub ready: usize,
    pub already_here: usize,
    pub repeated: usize,
    pub cannot_import: usize,
    /// Ready rows that come with notes.
    pub ready_with_notes: usize,
    pub rows: Vec<PreviewRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportResult {
    pub added: usize,
    pub skipped: usize,
}

struct Planned {
    preview: PreviewRow,
    branch_id: Option<Uuid>,
    notes_field: Option<String>,
}

/// What would happen to every row. Writes nothing.
pub async fn preview(
    tx: &mut TenantTx,
    actor: Actor,
    sheet: &Sheet,
    options: &ImportOptions,
) -> Result<Preview> {
    actor.require(Capability::ManageStudents)?;
    let (preview, _) = plan(tx, sheet, options).await?;
    Ok(preview)
}

/// Add every ready row, in one transaction.
pub async fn commit(
    tx: &mut TenantTx,
    actor: Actor,
    sheet: &Sheet,
    options: &ImportOptions,
) -> Result<ImportResult> {
    actor.require(Capability::ManageStudents)?;
    // Two imports at once would each think the other's students are new.
    tx.lock_studio("import").await?;
    let (preview, planned) = plan(tx, sheet, options).await?;

    let mut added = 0;
    for p in planned
        .into_iter()
        .filter(|p| p.preview.outcome == Outcome::Ready)
    {
        let row = p.preview;
        let contacts = row
            .contacts
            .iter()
            .enumerate()
            .map(|(i, c)| NewContact {
                name: c.name.clone(),
                phone: c.phone.clone(),
                relation: c.relation,
                is_primary: i == 0,
            })
            .collect();
        students::create_via(
            tx,
            actor,
            NewStudent {
                name: row.name,
                date_of_birth: row.date_of_birth,
                branch_id: p.branch_id,
                notes: p.notes_field,
                joined_on: Some(row.joined_on),
                contacts,
            },
            "import",
        )
        .await?;
        added += 1;
    }

    let skipped = preview.total - added;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "students.imported".into(),
            entity: "student".into(),
            entity_id: None,
            detail: json!({ "rows": preview.total, "added": added, "skipped": skipped }),
        },
    )
    .await?;
    Ok(ImportResult { added, skipped })
}

async fn plan(
    tx: &mut TenantTx,
    sheet: &Sheet,
    options: &ImportOptions,
) -> Result<(Preview, Vec<Planned>)> {
    let header = find_header(&sheet.rows).ok_or_else(|| refuse(NO_NAME_COLUMN))?;
    let data = &sheet.rows[header.index + 1..];
    if data.len() > MAX_ROWS {
        return Err(refuse(TOO_MANY_ROWS));
    }
    let today = tx.studio_today().await?;

    // The studio as it is now.
    let branches: Vec<(Uuid, String, bool)> =
        sqlx::query_as("SELECT id, name, active FROM branches")
            .fetch_all(tx.conn())
            .await?;
    let default_branch = match options.branch_id {
        None => None,
        Some(id) => match branches.iter().find(|(b, _, _)| *b == id) {
            Some((id, name, true)) => Some((*id, name.clone())),
            Some((_, name, false)) => {
                return Err(Error::Invalid(format!(
                    "{name} is closed. Choose an open branch."
                )));
            }
            None => return Err(refuse("Choose one of your branches.")),
        },
    };
    let existing: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT s.name, c.phone
           FROM students s
           LEFT JOIN student_contacts sc ON sc.org_id = s.org_id AND sc.student_id = s.id
           LEFT JOIN contacts c ON c.org_id = sc.org_id AND c.id = sc.contact_id",
    )
    .fetch_all(tx.conn())
    .await?;
    let mut known_names = HashSet::new();
    let mut known_pairs = HashSet::new();
    let mut phone_owner: HashMap<String, String> = HashMap::new();
    for (name, phone) in existing {
        let key = name.trim().to_lowercase();
        if let Some(p) = phone {
            known_pairs.insert((key.clone(), p.clone()));
            phone_owner.entry(p).or_insert(name.clone());
        }
        known_names.insert(key);
    }

    let mut seen_in_file: HashMap<(String, Option<String>), u32> = HashMap::new();
    let mut planned = Vec::with_capacity(data.len());

    for (row_number, cells) in data {
        let get = |field: Field| -> Value {
            header
                .columns
                .iter()
                .find(|(_, f)| **f == field)
                .and_then(|(col, _)| cells.get(*col).cloned())
                .unwrap_or(Value::Empty)
        };
        let mut notes = Vec::new();
        let name = get(Field::Name).shown();
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");

        // Date of birth: left out if unreadable or impossible.
        let dob_cell = get(Field::DateOfBirth);
        let mut date_of_birth = parse_date(&dob_cell, today);
        if !dob_cell.is_empty() && date_of_birth.is_none() {
            notes.push(format!(
                "Couldn't read the date of birth \"{}\", so it was left out.",
                dob_cell.shown()
            ));
        }
        if date_of_birth.is_some_and(|d| d > today || d.year() < 1900) {
            notes.push("The date of birth isn't possible, so it was left out.".into());
            date_of_birth = None;
        }

        // Joined on: today when missing or unusable.
        let joined_cell = get(Field::JoinedOn);
        let mut joined_on = today;
        if !joined_cell.is_empty() {
            match parse_date(&joined_cell, today) {
                None => notes.push(format!(
                    "Couldn't read the joining date \"{}\", so today was used.",
                    joined_cell.shown()
                )),
                Some(d) if d > today => {
                    notes.push("The joining date is in the future, so today was used.".into());
                }
                Some(d) if d.year() < 2000 => {
                    notes.push("The joining date is before 2000, so today was used.".into());
                }
                Some(d) if date_of_birth.is_some_and(|b| d < b) => {
                    notes.push(
                        "The joining date is before the date of birth, so today was used.".into(),
                    );
                }
                Some(d) => joined_on = d,
            }
        }

        // Branch: by name, else the chosen default.
        let branch_cell = get(Field::Branch).shown();
        let mut branch = default_branch.clone();
        if !branch_cell.is_empty() {
            let wanted = branch_cell.trim().to_lowercase();
            match branches
                .iter()
                .find(|(_, n, _)| n.trim().to_lowercase() == wanted)
            {
                Some((id, n, true)) => branch = Some((*id, n.clone())),
                found => {
                    let instead = default_branch
                        .as_ref()
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("no branch");
                    notes.push(if found.is_some() {
                        format!("{branch_cell} is closed, so {instead} was used.")
                    } else {
                        format!(
                            "There's no branch called \"{branch_cell}\", so {instead} was used."
                        )
                    });
                }
            }
        }

        // Notes column.
        let mut notes_field = Some(get(Field::Notes).shown()).filter(|n| !n.is_empty());
        if notes_field
            .as_ref()
            .is_some_and(|n| n.chars().count() > 2000)
        {
            notes_field = notes_field.map(|n| n.chars().take(2000).collect());
            notes.push("The notes were cut to 2,000 characters.".into());
        }

        // Contacts: the main phone, then mother's and father's.
        let first_name = name.split(' ').next().unwrap_or("").to_string();
        let mother_name = Some(get(Field::MotherName).shown()).filter(|n| !n.is_empty());
        let father_name = Some(get(Field::FatherName).shown()).filter(|n| !n.is_empty());
        let has_mother_phone = !get(Field::MotherPhone).is_empty();
        let has_father_phone = !get(Field::FatherPhone).is_empty();
        let is_child = date_of_birth.is_some_and(|d| age_on(d, today) < 18);
        let mut contacts: Vec<PreviewContact> = Vec::new();
        for phone_field in [Field::Phone, Field::MotherPhone, Field::FatherPhone] {
            let Some(parsed) = parse_phone(&get(phone_field)) else {
                continue;
            };
            let number = match parsed {
                Ok(n) => n,
                Err(raw) => {
                    notes.push(format!(
                        "\"{raw}\" isn't a phone number we can message, so it was left out."
                    ));
                    continue;
                }
            };
            if contacts.iter().any(|c| c.phone == number) {
                continue;
            }
            let (contact_name, relation) = match phone_field {
                Field::MotherPhone => (
                    mother_name
                        .clone()
                        .unwrap_or(format!("Mother of {first_name}")),
                    Relation::Mother,
                ),
                Field::FatherPhone => (
                    father_name
                        .clone()
                        .unwrap_or(format!("Father of {first_name}")),
                    Relation::Father,
                ),
                _ => {
                    let given = Some(get(Field::ContactName).shown()).filter(|n| !n.is_empty());
                    match (given, parse_relation(&get(Field::Relation))) {
                        (Some(n), r) => (n, r.unwrap_or(Relation::Other)),
                        (None, Some(Relation::Myself)) => (name.clone(), Relation::Myself),
                        (None, Some(r)) => (format!("{} of {first_name}", relation_word(r)), r),
                        // A single phone column next to a mother's or
                        // father's name is that parent's number.
                        (None, None) if mother_name.is_some() && !has_mother_phone => {
                            (mother_name.clone().unwrap_or_default(), Relation::Mother)
                        }
                        (None, None) if father_name.is_some() && !has_father_phone => {
                            (father_name.clone().unwrap_or_default(), Relation::Father)
                        }
                        // A child's own number is rare; a parent's is usual.
                        (None, None) if is_child => {
                            (format!("Parent of {first_name}"), Relation::Guardian)
                        }
                        (None, None) => (name.clone(), Relation::Myself),
                    }
                }
            };
            contacts.push(PreviewContact {
                name: contact_name.chars().take(120).collect(),
                phone: number,
                relation,
            });
        }
        contacts.truncate(students::MAX_CONTACTS);

        // What happens to the row.
        let key = name.to_lowercase();
        let first_phone = contacts.first().map(|c| c.phone.clone());
        let mut outcome = Outcome::Ready;
        if name.is_empty() {
            outcome = Outcome::CannotImport;
            notes.insert(0, "There's no name in this row.".into());
        } else if name.chars().count() > 120 {
            outcome = Outcome::CannotImport;
            notes.insert(0, "The name is longer than 120 characters.".into());
        } else if let Some(earlier) = seen_in_file.get(&(key.clone(), first_phone.clone())) {
            outcome = Outcome::Repeated;
            notes.insert(0, format!("Same student as row {earlier}."));
        } else if contacts
            .iter()
            .any(|c| known_pairs.contains(&(key.clone(), c.phone.clone())))
            || (contacts.is_empty() && known_names.contains(&key))
        {
            outcome = Outcome::AlreadyHere;
            notes.insert(0, format!("{name} is already in your students."));
        } else {
            if known_names.contains(&key) {
                notes.push(format!(
                    "Another student called {name} is already here, with a different number."
                ));
            }
            if contacts.is_empty() {
                notes.push("No phone number, so reminders can't reach anyone yet.".into());
            }
            for c in &contacts {
                if let Some(owner) = phone_owner.get(&c.phone) {
                    if owner.to_lowercase() != key {
                        notes.push(format!("Shares a contact with {owner}."));
                    }
                }
            }
        }
        if outcome == Outcome::Ready {
            seen_in_file.insert((key.clone(), first_phone.clone()), *row_number);
            // Later rows in this file see this one as known too, so a
            // sibling row shares the contact and a repeat is caught.
            for c in &contacts {
                phone_owner.entry(c.phone.clone()).or_insert(name.clone());
            }
        }

        planned.push(Planned {
            preview: PreviewRow {
                row: *row_number,
                name,
                outcome,
                notes,
                branch_name: branch.as_ref().map(|(_, n)| n.clone()),
                joined_on,
                date_of_birth,
                contacts,
            },
            branch_id: branch.map(|(id, _)| id),
            notes_field,
        });
    }

    let count = |o: Outcome| planned.iter().filter(|p| p.preview.outcome == o).count();
    let preview = Preview {
        sheet_name: sheet.name.clone(),
        columns: header.used,
        ignored_columns: header.ignored,
        total: planned.len(),
        ready: count(Outcome::Ready),
        already_here: count(Outcome::AlreadyHere),
        repeated: count(Outcome::Repeated),
        cannot_import: count(Outcome::CannotImport),
        ready_with_notes: planned
            .iter()
            .filter(|p| p.preview.outcome == Outcome::Ready && !p.preview.notes.is_empty())
            .count(),
        rows: planned.iter().map(|p| p.preview.clone()).collect(),
    };
    Ok((preview, planned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    const TODAY: Date = date!(2026 - 11 - 23);

    #[test]
    fn dates_are_read_day_first() {
        let t = |s: &str| parse_date_text(s, TODAY);
        assert_eq!(t("04/03/2015"), Some(date!(2015 - 03 - 04)));
        assert_eq!(t("4-3-15"), Some(date!(2015 - 03 - 04)));
        assert_eq!(
            t("03/25/2015"),
            Some(date!(2015 - 03 - 25)),
            "only one reading works"
        );
        assert_eq!(t("2015-03-04"), Some(date!(2015 - 03 - 04)));
        assert_eq!(t("2015-03-04 00:00:00"), Some(date!(2015 - 03 - 04)));
        assert_eq!(t("4 Mar 2015"), Some(date!(2015 - 03 - 04)));
        assert_eq!(t("04-March-2015"), Some(date!(2015 - 03 - 04)));
        assert_eq!(t("March 4, 2015"), Some(date!(2015 - 03 - 04)));
        assert_eq!(t("12/12/99"), Some(date!(1999 - 12 - 12)));
        assert_eq!(t("31/02/2015"), None);
        assert_eq!(t("soon"), None);
        assert_eq!(
            parse_date(&Value::Number(42067.0), TODAY),
            Some(date!(2015 - 03 - 04)),
            "an Excel serial day"
        );
    }

    #[test]
    fn phones_come_from_numbers_and_messy_text() {
        let p = |v: Value| parse_phone(&v);
        assert_eq!(
            p(Value::Number(9876543210.0)),
            Some(Ok("+919876543210".into()))
        );
        assert_eq!(
            p(Value::Number(919876543210.0)),
            Some(Ok("+919876543210".into()))
        );
        assert_eq!(
            p(Value::Text("98765 43210 / 98765 43211".into())),
            Some(Ok("+919876543210".into()))
        );
        assert_eq!(p(Value::Text("12345".into())), Some(Err("12345".into())));
        assert_eq!(p(Value::Empty), None);
    }

    #[test]
    fn column_titles_are_matched_loosely() {
        assert_eq!(field_for("Student Name"), Some(Field::Name));
        assert_eq!(field_for("  MOTHER'S Phone No. "), Some(Field::MotherPhone));
        assert_eq!(field_for("Mother’s phone"), Some(Field::MotherPhone));
        assert_eq!(field_for("D.O.B"), Some(Field::DateOfBirth));
        assert_eq!(field_for("Date of Joining"), Some(Field::JoinedOn));
        assert_eq!(field_for("Favourite colour"), None);
    }

    #[test]
    fn csv_with_semicolons_and_a_bom_is_read() {
        let sheet = read_sheet("\u{feff}Name;Phone\nAarav;9876543210\n;\n".as_bytes()).unwrap();
        assert_eq!(sheet.rows.len(), 2, "the blank line is dropped");
        assert_eq!(sheet.rows[1].1[1], Value::Text("9876543210".into()));
    }

    #[test]
    fn files_that_arent_student_lists_are_refused_plainly() {
        let e = read_sheet(&[0xD0, 0xCF, 0x11, 0xE0, 0, 0]).unwrap_err();
        assert!(e.to_string().contains(".xls"), "{e}");
        let e = read_sheet(&[0xff, 0xfe, 0x00, 0x41]).unwrap_err();
        assert!(e.to_string().contains("CSV UTF-8"), "{e}");
        let e = read_sheet(b"Fruit,Colour\nApple,Red\n").unwrap_err();
        assert!(e.to_string().contains("column called Name"), "{e}");
        let e = read_sheet(b"PK\x03\x04garbage").unwrap_err();
        assert!(e.to_string().contains("couldn't read"), "{e}");
    }
}
