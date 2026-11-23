import { formatPhone } from "@/lib/format";
import type { Conversation } from "@/lib/types";

/** What to call the other person: the studio's name for them, else their number. */
export function whoIs(c: Pick<Conversation, "contact_name" | "phone">): string {
  return c.contact_name ?? formatPhone(c.phone);
}

/** "23 Nov", from YYYY-MM-DD. */
export function dayLabel(day: string): string {
  const [, m, d] = day.split("-").map(Number);
  return `${d} ${["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][m - 1]}`;
}

/**
 * For a list: the time if it was today, otherwise the date. Both come
 * from the API already in the studio's own time, so nothing here depends
 * on where the server or the browser happens to be.
 */
export function whenShort(c: Pick<Conversation, "last_on" | "last_at">, today: string): string {
  return c.last_on === today ? c.last_at : dayLabel(c.last_on);
}
