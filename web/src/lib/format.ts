/** Display helpers shared by server and client components. */

/** "+919811122233" → "+91 98111 22233". Other countries are left as stored. */
export function formatPhone(e164: string): string {
  const m = /^\+91(\d{5})(\d{5})$/.exec(e164);
  return m ? `+91 ${m[1]} ${m[2]}` : e164;
}

/** Whole years between a YYYY-MM-DD birth date and today. */
export function ageFrom(dateOfBirth: string, today = new Date()): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateOfBirth);
  if (!m) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  let age = today.getFullYear() - y;
  if (today.getMonth() + 1 < mo || (today.getMonth() + 1 === mo && today.getDate() < d)) age -= 1;
  return age >= 0 ? age : null;
}

/** "2026-11-23" → "23 Nov 2026". */
export function formatDate(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  return `${Number(m[3])} ${months[Number(m[2]) - 1]} ${m[1]}`;
}

const WEEKDAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
export const WEEKDAY_LONG = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/** 1 = Monday … 7 = Sunday → "Mon". */
export function weekdayShort(weekday: number): string {
  return WEEKDAY_SHORT[weekday - 1] ?? "";
}

/** ISO weekday (1 = Monday) of a YYYY-MM-DD date, without time zones. */
export function isoWeekday(iso: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!m) return null;
  const day = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]))).getUTCDay();
  return day === 0 ? 7 : day;
}

/** "18:00" → minutes after midnight. */
export function minutesOf(hhmm: string): number {
  const [h, m] = hhmm.split(":").map(Number);
  return h * 60 + m;
}

/**
 * A class's week in a few words, grouping days that share a time:
 * "Mon, Wed 18:00–19:00 · Sat 10:00–11:30".
 */
export function scheduleSummary(slots: { weekday: number; starts_at: string; ends_at: string }[]): string {
  const groups = new Map<string, number[]>();
  for (const s of slots) {
    const key = `${s.starts_at}–${s.ends_at}`;
    groups.set(key, [...(groups.get(key) ?? []), s.weekday]);
  }
  return [...groups.entries()]
    .sort((a, b) => Math.min(...a[1]) - Math.min(...b[1]))
    .map(([time, days]) => `${days.map(weekdayShort).join(", ")} ${time}`)
    .join(" · ");
}

const rupeeFormat = new Intl.NumberFormat("en-IN", { style: "currency", currency: "INR", maximumFractionDigits: 2 });
const rupeeWhole = new Intl.NumberFormat("en-IN", { style: "currency", currency: "INR", maximumFractionDigits: 0 });

/** 12450000 paise → "₹1,24,500". Paise shown only when there are some. */
export function formatRupees(paise: number): string {
  return paise % 100 === 0 ? rupeeWhole.format(paise / 100) : rupeeFormat.format(paise / 100);
}

/** "1,500" or "1500.50" typed by a person → paise, or null when it isn't a price. */
export function parseRupees(input: string): number | null {
  const cleaned = input.replace(/[₹,\s]/g, "");
  if (!/^\d{1,7}(\.\d{1,2})?$/.test(cleaned)) return null;
  const [whole, fraction = ""] = cleaned.split(".");
  return Number(whole) * 100 + Number(fraction.padEnd(2, "0"));
}

/** Add days to a YYYY-MM-DD date. */
export function addDays(iso: string, days: number): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!m) return iso;
  const d = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]) + days));
  return d.toISOString().slice(0, 10);
}
