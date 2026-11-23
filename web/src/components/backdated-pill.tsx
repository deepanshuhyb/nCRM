import { formatDate } from "@/lib/format";

/**
 * Marks a date that was set earlier than the day it was entered: a join
 * date typed in after the fact, an imported student, a class start set in
 * the past. Shows nothing when the date isn't backdated.
 */
export function BackdatedPill({ date, enteredOn }: { date: string; enteredOn: string }) {
  if (!(date < enteredOn)) return null;
  return (
    <span className="tag tag-quiet" title={`Entered on ${formatDate(enteredOn)}`}>
      Backdated<span className="sr-only">, entered on {formatDate(enteredOn)}</span>
    </span>
  );
}
