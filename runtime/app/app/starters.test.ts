import { expect, test } from "bun:test"
import { newTabBlock, notInstalled, STARTERS } from "./starters.ts"

const named = (...names: string[]) => names.map((name) => ({ spec: { name } }))

test("the starters are offered in order, but those installed", () => {
  expect(notInstalled([]).map((s) => s.name)).toEqual(["launcher", "terminal", "agents", "browser"])
  expect(notInstalled(named("terminal", "git")).map((s) => s.name)).toEqual(["launcher", "agents", "browser"])
  expect(notInstalled(named(...STARTERS.map((s) => s.name)))).toEqual([])
})

test("new tabs open the first installed: Launcher, then Terminal, Agents, Browser", () => {
  expect(newTabBlock([])).toBeUndefined()
  expect(newTabBlock(named("git"))).toBeUndefined()
  expect(newTabBlock(named("browser", "terminal", "launcher"))).toBe("pumi_dev.launcher.new")
  expect(newTabBlock(named("browser", "terminal"))).toBe("pumi_dev.terminal.shell")
  expect(newTabBlock(named("browser", "agents"))).toBe("pumi_dev.agents.term")
  expect(newTabBlock(named("browser"))).toBe("pumi_dev.browser.page")
})
