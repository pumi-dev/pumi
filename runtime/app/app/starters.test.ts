import { expect, test } from "bun:test"
import { notInstalled, STARTERS } from "./starters.ts"

const named = (...names: string[]) => names.map((name) => ({ spec: { name } }))

test("the starters are offered in order, but those installed", () => {
  expect(notInstalled([]).map((s) => s.name)).toEqual(["terminal", "agents", "browser"])
  expect(notInstalled(named("terminal", "git")).map((s) => s.name)).toEqual(["agents", "browser"])
  expect(notInstalled(named(...STARTERS.map((s) => s.name)))).toEqual([])
})

test("Launchpad is not offered: it ships with pumi, so new tabs open it already", () => {
  expect(notInstalled([]).map((s) => s.name)).not.toContain("launchpad")
  expect(notInstalled([]).map((s) => s.source)).not.toContain("pumi-dev/plugins/launcher")
})
