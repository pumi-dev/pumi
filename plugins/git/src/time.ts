// When a commit was: as long ago as it reads well ("3 days ago", "2 months ago"), or the day and
// minute it was, in this machine's time zone — the two the `date_format` option chooses between.

/** `timestamp` (seconds since the epoch) as a person says it, measured from `now` (milliseconds,
 * as `Date.now()` gives it). */
export function relative(timestamp: number, now: number = Date.now()): string {
  const then = new Date(timestamp * 1000);
  if (Number.isNaN(then.getTime())) return "";
  const today = new Date(now);
  const elapsed = elapsedWords(then, today);
  return elapsed ?? dateWords(then, today);
}

/** `timestamp` as the day and minute it was: `2025-10-09 08:53`, in this machine's time zone. */
export function absolute(timestamp: number): string {
  const then = new Date(timestamp * 1000);
  if (Number.isNaN(then.getTime())) return "";
  return `${pad(then.getFullYear(), 4)}-${pad(then.getMonth() + 1, 2)}-${pad(then.getDate(), 2)} ${pad(
    then.getHours(),
    2,
  )}:${pad(then.getMinutes(), 2)}`;
}

/** A commit's own timestamp, as the option says to read it. */
export function when(timestamp: number, format: "relative" | "absolute", now: number = Date.now()): string {
  return format === "absolute" ? absolute(timestamp) : relative(timestamp, now);
}

function pad(value: number, width: number): string {
  return String(value).padStart(width, "0");
}

/** Under a day is a duration; past it a date is what a person counts in. */
function elapsedWords(then: Date, now: Date): string | null {
  const minutes = Math.trunc((now.getTime() - then.getTime()) / 60_000);
  if (minutes === 0) return "Just now";
  if (minutes === 1) return "1 minute ago";
  if (minutes >= 2 && minutes <= 59) return `${minutes} minutes ago`;
  const hours = Math.trunc(minutes / 60);
  if (hours === 1) return "1 hour ago";
  if (hours >= 2 && hours <= 23) return `${hours} hours ago`;
  return null;
}

function dateWords(then: Date, now: Date): string {
  const days = daysBetween(then, now);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days >= 2 && days <= 6) return `${days} days ago`;
  const weeks = Math.trunc(days / 7);
  if (weeks === 1) return "1 week ago";
  if (weeks >= 2 && weeks <= 4) return `${weeks} weeks ago`;
  const months = wholeMonths(then, now);
  if (months <= 1) return "1 month ago";
  if (months <= 11) return `${months} months ago`;
  if (months <= 60) return yearsAndMonths(months);
  return `${Math.trunc((months + 6) / 12)} years ago`;
}

function yearsAndMonths(months: number): string {
  const years = Math.floor(months / 12);
  const rest = months % 12;
  const yearUnit = years === 1 ? "year" : "years";
  if (rest === 0) return `${years} ${yearUnit} ago`;
  const monthUnit = rest === 1 ? "month" : "months";
  return `${years} ${yearUnit}, ${rest} ${monthUnit} ago`;
}

/** The days from one calendar date to the other, as a whole number. */
function daysBetween(then: Date, now: Date): number {
  return dayNumber(now) - dayNumber(then);
}

/** A date as a count of days, so two of them can be subtracted. */
function dayNumber(date: Date): number {
  return Math.floor(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000);
}

/** The months from one calendar month to the other: 3 January to 2 March is two. */
function wholeMonths(then: Date, now: Date): number {
  const thenMonth = then.getMonth() + 1;
  const nowMonth = now.getMonth() + 1;
  const years = Math.max(0, now.getFullYear() - then.getFullYear());
  if (years === 0) return Math.max(0, nowMonth - thenMonth);
  return thenMonth > nowMonth ? (years - 1) * 12 + (12 - thenMonth + nowMonth) : years * 12 + (nowMonth - thenMonth);
}
