import "server-only";

import { cookies } from "next/headers";
import { cache } from "react";
import { notFound, redirect } from "next/navigation";

import { apiOrigin } from "@/lib/env";
import type {
  ApiErrorBody,
  AttendanceAlerts,
  BatchDetail,
  BatchSummary,
  Branch,
  Conversation,
  Credits,
  DayAttendance,
  DueRow,
  FeesReport,
  FeesSummary,
  Me,
  MeetingCount,
  MessagingSettings,
  MonthStudents,
  Page,
  PaymentsList,
  Plan,
  Receipt,
  Room,
  SessionDetail,
  SessionSummary,
  StaffMember,
  StudentAttendance,
  StudentDetail,
  StudentEnrollment,
  StudentFees,
  StudentListItem,
  Summary,
  Thread,
} from "@/lib/types";

/**
 * Calls the Rust API from Server Components, on behalf of the person whose
 * request is being rendered. Only their session cookie is forwarded; the
 * API decides what they may see.
 *
 * Failures that mean "go somewhere else" (not signed in, no studio picked)
 * become redirects. Anything else throws and lands on the error page,
 * which shows a generic message.
 */
const SESSION_COOKIES = ["__Host-ncrm_session", "ncrm_session"];

async function sessionCookieHeader(): Promise<string | null> {
  const jar = await cookies();
  for (const name of SESSION_COOKIES) {
    const value = jar.get(name)?.value;
    if (value) return `${name}=${value}`;
  }
  return null;
}

export class ApiRequestError extends Error {
  constructor(
    public status: number,
    public code: string,
  ) {
    super(`API request failed: ${status} ${code}`);
  }
}

async function apiGet<T>(path: string): Promise<T> {
  const cookie = await sessionCookieHeader();
  if (!cookie) redirect("/sign-in");

  const res = await fetch(new URL(path, apiOrigin()), {
    headers: { cookie, accept: "application/json" },
    cache: "no-store",
    redirect: "manual",
    signal: AbortSignal.timeout(15_000),
  });

  if (res.ok) return (await res.json()) as T;

  const code = ((await res.json().catch(() => null)) as ApiErrorBody | null)?.error?.code ?? "unknown";
  if (code === "not_signed_in") redirect("/sign-in");
  if (code === "choose_studio") redirect("/choose-studio");
  if (code === "not_found") notFound();
  throw new ApiRequestError(res.status, code);
}

/**
 * The signed-in person, or a redirect to sign-in. `cache` makes repeated
 * calls during one render (layout + page) share a single API request.
 */
export const getMe = cache((): Promise<Me> => apiGet<Me>("/auth/me"));

/** Branches of the current studio. */
export function listBranches(): Promise<Branch[]> {
  return apiGet<Branch[]>("/branches");
}

/** Home-screen counts. */
export function getSummary(): Promise<Summary> {
  return apiGet<Summary>("/summary");
}

/** The studio's team. Owners and managers only. */
export function listStaff(): Promise<StaffMember[]> {
  return apiGet<StaffMember[]>("/staff");
}

export type StudentQuery = { q?: string; status?: string; offset?: number };

/** A page of students. Values are passed as query parameters, never as path. */
export function listStudents(query: StudentQuery): Promise<Page<StudentListItem>> {
  const params = new URLSearchParams();
  if (query.q) params.set("q", query.q.slice(0, 80));
  if (query.status) params.set("status", query.status);
  if (query.offset) params.set("offset", String(query.offset));
  params.set("limit", "50");
  return apiGet<Page<StudentListItem>>(`/students?${params.toString()}`);
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Ids come from the URL: anything that isn't a UUID is a 404 without asking the API. */
function checkedId(id: string): string {
  if (!UUID.test(id)) notFound();
  return id;
}

/** One student. */
export function getStudent(id: string): Promise<StudentDetail> {
  return apiGet<StudentDetail>(`/students/${checkedId(id)}`);
}

/** A student's classes, current first. */
export function getStudentEnrollments(id: string): Promise<StudentEnrollment[]> {
  return apiGet<StudentEnrollment[]>(`/students/${checkedId(id)}/enrollments`);
}

/** A student's class credits. */
export function getStudentCredits(id: string): Promise<Credits> {
  return apiGet<Credits>(`/students/${checkedId(id)}/credits`);
}

/** Every room in the studio. */
export function listRooms(): Promise<Room[]> {
  return apiGet<Room[]>("/rooms");
}

/** Classes for the timetable. */
export function listBatches(query: { branchId?: string; archived?: boolean } = {}): Promise<BatchSummary[]> {
  const params = new URLSearchParams();
  if (query.branchId && UUID.test(query.branchId)) params.set("branch_id", query.branchId);
  if (query.archived) params.set("archived", "true");
  const qs = params.toString();
  return apiGet<BatchSummary[]>(`/batches${qs ? `?${qs}` : ""}`);
}

/** One class, with its roster when the person may see it. */
export function getBatch(id: string): Promise<BatchDetail> {
  return apiGet<BatchDetail>(`/batches/${checkedId(id)}`);
}

/** Prices. Front desk and up. */
export function listPlans(): Promise<Plan[]> {
  return apiGet<Plan[]>("/plans");
}

/** One day's class meetings (today when no date is given). */
export function listSessions(query: { date?: string; branchId?: string } = {}): Promise<SessionSummary[]> {
  const params = new URLSearchParams();
  if (query.date && /^\d{4}-\d{2}-\d{2}$/.test(query.date)) params.set("date", query.date);
  if (query.branchId && UUID.test(query.branchId)) params.set("branch_id", query.branchId);
  const qs = params.toString();
  return apiGet<SessionSummary[]>(`/sessions${qs ? `?${qs}` : ""}`);
}

/** One class meeting, with its roster when the person may mark it. */
export function getSession(id: string): Promise<SessionDetail> {
  return apiGet<SessionDetail>(`/sessions/${checkedId(id)}`);
}

/** Unmarked classes and absence streaks. */
export function getAttendanceAlerts(): Promise<AttendanceAlerts> {
  return apiGet<AttendanceAlerts>("/attendance/alerts");
}

/** A student's recent attendance. */
export function getStudentAttendance(id: string): Promise<StudentAttendance> {
  return apiGet<StudentAttendance>(`/students/${checkedId(id)}/attendance`);
}

// ------------------------------------------------------------------ fees

export const getFeesSummary = (): Promise<FeesSummary> => apiGet<FeesSummary>("/fees/summary");

export function listDues(filters: { branchId?: string; limit?: number; offset?: number } = {}): Promise<Page<DueRow>> {
  const params = new URLSearchParams();
  if (filters.branchId) params.set("branch_id", filters.branchId);
  if (filters.limit) params.set("limit", String(filters.limit));
  if (filters.offset) params.set("offset", String(filters.offset));
  const qs = params.toString();
  return apiGet<Page<DueRow>>(`/fees/dues${qs ? `?${qs}` : ""}`);
}

/** Payments received on one day, with totals by method. */
export function listPayments(day: string): Promise<PaymentsList> {
  const params = new URLSearchParams({ from: day, to: day, limit: "200" });
  return apiGet<PaymentsList>(`/payments?${params.toString()}`);
}

export const getReceipt = (paymentId: string): Promise<Receipt> =>
  apiGet<Receipt>(`/payments/${encodeURIComponent(paymentId)}`);

export const getStudentFees = (studentId: string): Promise<StudentFees> =>
  apiGet<StudentFees>(`/students/${encodeURIComponent(studentId)}/fees`);

// --------------------------------------------------------------- reports

/** Marks per day across the studio for the last `days` days, oldest first. */
export const getAttendanceReport = (days = 28): Promise<DayAttendance[]> => apiGet<DayAttendance[]>(`/reports/attendance?days=${days}`);

/** Joined and left per month, last six months, oldest first. */
export const getStudentsReport = (): Promise<MonthStudents[]> => apiGet<MonthStudents[]>("/reports/students");

/** Dues by age; this month's collection for roles allowed to see it. */
export const getFeesReport = (): Promise<FeesReport> => apiGet<FeesReport>("/reports/fees");

/** A class's last eight marked meetings, oldest first. */
export const getClassHistory = (batchId: string): Promise<MeetingCount[]> =>
  apiGet<MeetingCount[]>(`/batches/${encodeURIComponent(batchId)}/history`);

// ---------------------------------------------------------------- WhatsApp

export const listInbox = (): Promise<Conversation[]> => apiGet<Conversation[]>("/inbox");
export const getThread = (id: string): Promise<Thread> => apiGet<Thread>(`/inbox/${encodeURIComponent(id)}`);
export const getMessagingSettings = (): Promise<MessagingSettings> => apiGet<MessagingSettings>("/messaging/settings");
