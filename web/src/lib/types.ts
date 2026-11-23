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
