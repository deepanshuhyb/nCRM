/** Shapes returned by the Rust API. Keep in step with `crates/ncrm-api`. */

export type Role = "owner" | "manager" | "front_desk" | "instructor";

export type Capability =
  | "view_studio"
  | "manage_branches"
  | "manage_staff"
  | "manage_students"
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
};
