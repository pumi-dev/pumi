import { expect, test } from "bun:test"
import { notInstalled, STARTERS } from "./starters.ts"

test("a starter is offered until a plugin of its name is installed", () => {
  expect(notInstalled([]).map((s) => s.name)).toEqual(["terminal", "agents", "browser"])
  expect(notInstalled([{ spec: { name: "terminal" } }, { spec: { name: "git" } }]).map((s) => s.name)).toEqual(["agents", "browser"])
  expect(notInstalled(STARTERS.map((s) => ({ spec: { name: s.name } })))).toEqual([])
})
