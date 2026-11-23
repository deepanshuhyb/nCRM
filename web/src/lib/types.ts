/** Shapes returned by the Rust API. Keep in step with `crates/ncrm-api`. */

export type Role = "owner" | "manager" | "front_desk" | "instructor";

export type Capability =
  | "view_studio"
  | "manage_branches"
  | "manage_staff"
  | "manage_students"
  | "manage_classes"
  | "manage_fees"
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
