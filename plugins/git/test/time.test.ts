// When a commit was, in words.
import { expect, test } from "bun:test";
import { absolute, relative, when } from "../src/time.ts";

/** `YYYY-MM-DD HH:MM` in this machine's time zone, from what a Date says of it. */
function local(year: number, month: number, day: number, hour = 12, minute = 0): number {
  return new Date(year, month - 1, day, hour, minute).getTime() / 1000;
}

/** The moment the words are measured from, in milliseconds as `Date.now()` gives it. */
const NOW = local(2026, 3, 4, 12, 0) * 1000;

test("under a day is a duration", () => {
  expect(relative(local(2026, 3, 4, 12, 0), NOW)).toBe("Just now");
  expect(relative(local(2026, 3, 4, 11, 59), NOW)).toBe("1 minute ago");
  expect(relative(local(2026, 3, 4, 11, 45), NOW)).toBe("15 minutes ago");
  expect(relative(local(2026, 3, 4, 11, 0), NOW)).toBe("1 hour ago");
  expect(relative(local(2026, 3, 4, 2, 0), NOW)).toBe("10 hours ago");
});

test("past a day is a date", () => {
  expect(relative(local(2026, 3, 3, 2, 0), NOW)).toBe("Yesterday");
  expect(relative(local(2026, 3, 1, 12, 0), NOW)).toBe("3 days ago");
  expect(relative(local(2026, 2, 25, 12, 0), NOW)).toBe("1 week ago");
  expect(relative(local(2026, 2, 18, 12, 0), NOW)).toBe("2 weeks ago");
  expect(relative(local(2026, 1, 28, 12, 0), NOW)).toBe("2 months ago");
  expect(relative(local(2025, 12, 28, 12, 0), NOW)).toBe("3 months ago");
});

test("a year or more carries its months", () => {
  expect(relative(local(2025, 3, 4, 12, 0), NOW)).toBe("1 year ago");
  expect(relative(local(2024, 5, 4, 12, 0), NOW)).toBe("1 year, 10 months ago");
  expect(relative(local(2024, 3, 4, 12, 0), NOW)).toBe("2 years ago");
  expect(relative(local(2024, 2, 4, 12, 0), NOW)).toBe("2 years, 1 month ago");
});

test("months count by the month, not the days between", () => {
  const then = local(2026, 1, 3, 12, 0);
  expect(relative(then, local(2026, 2, 20, 12, 0) * 1000)).toBe("1 month ago");
  expect(relative(then, local(2026, 3, 20, 12, 0) * 1000)).toBe("2 months ago");
  expect(relative(then, local(2026, 12, 3, 12, 0) * 1000)).toBe("11 months ago");
  // Under five weeks it is still weeks.
  expect(relative(then, local(2026, 2, 1, 12, 0) * 1000)).toBe("4 weeks ago");
});

test("absolute is the local day and minute", () => {
  expect(absolute(local(2025, 10, 9, 8, 53))).toBe("2025-10-09 08:53");
});

test("the option chooses which of the two a commit shows", () => {
  const then = local(2025, 10, 9, 8, 53);
  expect(when(then, "relative", NOW)).toBe("5 months ago");
  expect(when(then, "absolute", NOW)).toBe("2025-10-09 08:53");
});
