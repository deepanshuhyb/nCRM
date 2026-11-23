/** Shapes returned by the Rust API. Keep in step with `crates/ncrm-api`. */

export type Role = "owner" | "manager" | "front_desk" | "instructor";

export type Capability =
  | "view_studio"
  | "manage_branches"
  | "manage_staff"
  | "manage_students"
  | "manage_classes"
  | "manage_fees"
  | "adjust_fees"
  | "mark_any_attendance"
  | "mark_own_attendance"
  | "view_financials"
  | "manage_billing";

export type Membership = {
  org_id: string;
  org_name: string;
  staff_id: string;
  staff_name: string;
  role: Role;
};

export type CurrentStudio = Membership & { capabilities: Capability[] };

export type Me = {
  phone: string;
  studio: CurrentStudio | null;
  memberships: Membership[];
};

export type Branch = {
  id: string;
  org_id: string;
  name: string;
  address: string | null;
  timezone: string | null;
  active: boolean;
  created_at: string;
};

export type ApiErrorBody = { error: { code: string; message: string } };

export const ROLE_LABEL: Record<Role, string> = {
  owner: "Owner",
  manager: "Manager",
  front_desk: "Front desk",
  instructor: "Instructor",
};

export type StaffMember = {
  id: string;
  org_id: string;
  branch_id: string | null;
  phone: string;
  name: string;
  role: Role;
  active: boolean;
  created_at: string;
};

export type StudentStatus = "active" | "paused" | "left";
export type Relation = "self" | "mother" | "father" | "guardian" | "other";

export const STATUS_LABEL: Record<StudentStatus, string> = {
  active: "Active",
  paused: "On a break",
  left: "Left",
};

export const RELATION_LABEL: Record<Relation, string> = {
  self: "Themselves",
  mother: "Mother",
  father: "Father",
  guardian: "Guardian",
  other: "Other",
};

export type ContactLink = {
  contact_id: string;
  name: string;
  phone: string;
  relation: Relation;
  is_primary: boolean;
  student_count: number;
};

export type Student = {
  id: string;
  org_id: string;
  branch_id: string | null;
  name: string;
  date_of_birth: string | null;
  status: StudentStatus;
  notes: string | null;
  joined_on: string;
  /** The studio's date when the record was entered; later than joined_on means backdated. */
  entered_on: string;
  created_at: string;
  updated_at: string;
};

export type StudentDetail = Student & { contacts: ContactLink[] };

export type StudentListItem = {
  id: string;
  name: string;
  status: StudentStatus;
  branch_id: string | null;
  branch_name: string | null;
  joined_on: string;
  primary_contact_name: string | null;
  primary_contact_phone: string | null;
};

export type Page<T> = { items: T[]; total: number; limit: number; offset: number };

export type Summary = {
  branches: number;
  open_branches: number;
  active_students: number;
  active_team: number;
  classes: number;
  plans: number;
  classes_today: number;
  /** Classes in the last week that are over and not fully marked. */
  unmarked: number;
  /** Fees still to collect; null for roles that don't handle fees. */
  dues_paise: number | null;
  students_owing: number | null;
  /** The studio's date, YYYY-MM-DD. */
  today: string;
};

export type Room = {
  id: string;
  org_id: string;
  branch_id: string;
  name: string;
  capacity: number | null;
  active: boolean;
  created_at: string;
};

/** One weekly meeting. 1 = Monday … 7 = Sunday; times are "HH:MM". */
export type Slot = { weekday: number; starts_at: string; ends_at: string };

export type Batch = {
  id: string;
  org_id: string;
  branch_id: string;
  room_id: string | null;
  instructor_id: string | null;
  name: string;
  style: string | null;
  capacity: number | null;
  starts_on: string;
  ends_on: string | null;
  active: boolean;
  created_at: string;
  updated_at: string;
};

export type BatchSummary = Batch & {
  branch_name: string;
  room_name: string | null;
  instructor_name: string | null;
  enrolled: number;
  slots: Slot[];
};

export type RosterEntry = {
  enrollment_id: string;
  student_id: string;
  student_name: string;
  student_status: StudentStatus;
  plan_id: string | null;
  plan_name: string | null;
  starts_on: string;
  entered_on: string;
};

/** `roster` is null when the person may not see who's in the class. */
export type BatchDetail = BatchSummary & { roster: RosterEntry[] | null };

export type PlanKind = "monthly" | "pack" | "drop_in";

export const PLAN_KIND_LABEL: Record<PlanKind, string> = {
  monthly: "Monthly fee",
  pack: "Class pack",
  drop_in: "Drop-in",
};

export type Plan = {
  id: string;
  org_id: string;
  name: string;
  kind: PlanKind;
  price_paise: number;
  classes: number | null;
  valid_days: number | null;
  active: boolean;
  created_at: string;
  updated_at: string;
};

export type StudentEnrollment = {
  id: string;
  batch_id: string;
  batch_name: string;
  style: string | null;
  branch_name: string;
  plan_id: string | null;
  plan_name: string | null;
  plan_price_paise: number | null;
  starts_on: string;
  ended_on: string | null;
  entered_on: string;
  slots: Slot[];
};

export type CreditLot = {
  id: string;
  plan_id: string | null;
  plan_name: string | null;
  classes: number;
  remaining: number;
  price_paise: number;
  expires_on: string | null;
  expired: boolean;
  note: string | null;
  created_at: string;
};

export type Credits = {
  available: number;
  next_expiry: { on: string; classes: number } | null;
  lots: CreditLot[];
};

export type ImportOutcome = "ready" | "already_here" | "repeated" | "cannot_import";

export type ImportPreviewRow = {
  row: number;
  name: string;
  outcome: ImportOutcome;
  notes: string[];
  branch_name: string | null;
  joined_on: string;
  date_of_birth: string | null;
  contacts: { name: string; phone: string; relation: Relation }[];
};

export type ImportPreview = {
  sheet_name: string | null;
  columns: { title: string; used_as: string }[];
  ignored_columns: string[];
  total: number;
  ready: number;
  already_here: number;
  repeated: number;
  cannot_import: number;
  ready_with_notes: number;
  rows: ImportPreviewRow[];
};

export type ImportResult = { added: number; skipped: number };

export type AttendanceStatus = "present" | "late" | "absent" | "excused";

export const ATTENDANCE_LABEL: Record<AttendanceStatus, string> = {
  present: "Present",
  late: "Late",
  absent: "Absent",
  excused: "Excused",
};

export type SessionSummary = {
  id: string;
  batch_id: string;
  batch_name: string;
  branch_id: string;
  branch_name: string;
  room_name: string | null;
  on_date: string;
  starts_at: string;
  ends_at: string;
  status: "scheduled" | "cancelled";
  cancel_reason: string | null;
  instructor_id: string | null;
  instructor_name: string | null;
  teacher_present: boolean | null;
  expected: number;
  came: number;
  away: number;
  unmarked: number;
  ended: boolean;
};

export type RosterMark = {
  student_id: string;
  student_name: string;
  status: AttendanceStatus | null;
  drop_in: boolean;
  pays_with_credits: boolean;
  credits_left: number;
  owes_credit: boolean;
  absences_in_a_row: number;
};

export type SessionDetail = SessionSummary & { can_mark: boolean; roster: RosterMark[] | null };

export type AttendanceAlerts = {
  unmarked: SessionSummary[];
  streaks: { student_id: string; student_name: string; batch_id: string; batch_name: string; absences: number }[];
};

export type StudentAttendance = {
  came: number;
  missed: number;
  recent: { on_date: string; batch_name: string; status: AttendanceStatus; drop_in: boolean }[];
};

// ------------------------------------------------------------------ fees

export type ChargeKind = "monthly" | "pack" | "drop_in" | "other";
export type PaymentMethod = "cash" | "upi" | "card" | "bank_transfer" | "cheque" | "other";

export const PAYMENT_METHOD_LABEL: Record<PaymentMethod, string> = {
  cash: "Cash",
  upi: "UPI",
  card: "Card",
  bank_transfer: "Bank transfer",
  cheque: "Cheque",
  other: "Other",
};
export const PAYMENT_METHODS = Object.keys(PAYMENT_METHOD_LABEL) as PaymentMethod[];

export type Charge = {
  id: string;
  kind: ChargeKind;
  description: string;
  amount_paise: number;
  paid_paise: number;
  due_on: string;
  period_start: string | null;
  overdue: boolean;
  created_at: string;
  voided_at: string | null;
  void_reason: string | null;
};

export type Payment = {
  id: string;
  student_id: string;
  student_name: string;
  receipt_no: number;
  amount_paise: number;
  /** Paid ahead: not used by any charge yet. */
  unallocated_paise: number;
  method: PaymentMethod;
  reference: string | null;
  note: string | null;
  received_on: string;
  received_by_name: string;
  created_at: string;
  voided_at: string | null;
  void_reason: string | null;
};

export type StudentFees = {
  due_paise: number;
  overdue_paise: number;
  advance_paise: number;
  charges: Charge[];
  payments: Payment[];
};

export type Receipt = {
  payment: Payment;
  studio_name: string;
  lines: { description: string; amount_paise: number }[];
  still_due_paise: number;
};

export type DueRow = {
  student_id: string;
  student_name: string;
  branch_name: string | null;
  due_paise: number;
  overdue_paise: number;
  oldest_due_on: string;
};

export type FeesSummary = {
  /** First day of the first month monthly fees are created for; null when off. */
  fees_from: string | null;
  fee_due_day: number;
  due_paise: number;
  overdue_paise: number;
  students_owing: number;
  /** Null for roles that can't see financial reports. */
  collected_this_month_paise: number | null;
};

export type PaymentsList = {
  from: string;
  to: string;
  items: Payment[];
  total: number;
  limit: number;
  offset: number;
  by_method: { method: PaymentMethod; amount_paise: number; payments: number }[];
  collected_paise: number;
};

// --------------------------------------------------------------- reports

export type DayAttendance = { date: string; came: number; away: number };
export type MonthStudents = { month: string; joined: number; left: number };
export type AgeBucket = { key: "not_due" | "week" | "month" | "older"; due_paise: number; students: number };
export type FeesReport = {
  month: string;
  ageing: AgeBucket[];
  /** Null for roles that can't see financial reports. */
  collection: { charged_paise: number; collected_paise: number; days: { date: string; collected_paise: number }[] } | null;
};
export type MeetingCount = { on_date: string; starts_at: string; status: "scheduled" | "cancelled"; came: number; away: number };
