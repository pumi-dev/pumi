import { expect, test } from "bun:test"
import { byUse, letters } from "./startable.ts"

const AGENTS = ["Pi", "Claude Code", "Codex", "Gemini", "OpenCode", "Cursor Agent", "Kiro", "Hermes"]
const REST = ["Browser", "Diff", "New Tab", "Music Studio", "Simulator", "Terminal", "Video Editor"]

test("each title takes its first letter while it is free, in the list's order", () => {
  expect(letters([...AGENTS, ...REST], ["f"])).toEqual(["P", "C", "E", "G", "O", "A", "K", "H", "B", "D", "N", "M", "S", "T", "V"])
})

test("one whose first letter is taken takes another word's first, then any of its letters", () => {
  // Cursor Agent: C is Claude's, A is its second word's. Codex: C, O and D are taken; E is next.
  expect(letters(["Claude Code", "Cursor Agent", "Open", "Diff", "Codex"])).toEqual(["C", "A", "O", "D", "E"])
  // A camelCase word's capital is a word's first letter too.
  expect(letters(["Opus", "OpenCode"])).toEqual(["O", "C"])
})

test("a reserved letter is no one's", () => {
  expect(letters(["File…", "Film"], ["f"])).toEqual(["I", "L"])
})

test("past every letter of its title, a digit, then none", () => {
  expect(letters(["a", "a", "a"])).toEqual(["A", "1", "2"])
  expect(letters(Array.from({ length: 12 }, () => "x")).slice(-2)).toEqual(["0", ""])
})

test("the most used first; as often used keep their order", () => {
  const list = ["a", "b", "c", "d"].map((id) => ({ id }))
  expect(byUse(list, { c: 3, b: 1, d: 1 }).map((r) => r.id)).toEqual(["c", "b", "d", "a"])
  expect(byUse(list, {}).map((r) => r.id)).toEqual(["a", "b", "c", "d"])
})
