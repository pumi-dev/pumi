import { expect, test } from "bun:test"
import { newTabChoices } from "./new-tab.tsx"
import { newTabOf, startables, type From } from "../startable.ts"

type Action = { id: string; title: string; command: string; pick?: { kind: "file" } }
const entry = (id: string, title: string, blocks: { id: string; title?: string }[], actions: Action[] = [], error?: string): From => ({
  id,
  title,
  prefix: `${id.replace("/", ".")}.`,
  state: error ? "disabled" : "running",
  error,
  manifest: { name: id.split("/")[1]!, contributes: { blocks, slots: actions.length ? { "tab.new": { actions } } : {} } },
})

const launcher = entry("pumi_dev/launcher", "Launcher", [{ id: "new", title: "New Tab" }])
const terminal = entry("pumi_dev/terminal", "Terminal", [{ id: "shell", title: "Terminal" }], [{ id: "open", title: "Terminal", command: "open" }])
const agents = entry(
  "pumi_dev/agents",
  "Agents",
  [
    { id: "term", title: "Agent" },
    { id: "session", title: "Agent chat" },
  ],
  [
    { id: "pi", title: "Pi", command: "new_pi" },
    { id: "claude", title: "Claude Code", command: "new_claude" },
  ],
)
const film = entry("amresyd/film", "Film", [{ id: "editor", title: "Film" }])
const editor = entry("pumi_dev/code-editor", "Code Editor", [], [{ id: "file", title: "File…", command: "open_file", pick: { kind: "file" } }])
const broken = entry("pumi_dev/browser", "Browser", [{ id: "page", title: "Browser" }], [], "its package.json: bad")

test("a plugin's tab.new actions are what it starts, else its blocks; one that did not load has none", () => {
  const list = startables([agents, film, terminal, broken])
  expect(list.map((s) => [s.value, s.title])).toEqual([
    ["pumi_dev.agents.new_pi", "Pi"],
    ["pumi_dev.agents.new_claude", "Claude Code"],
    ["amresyd.film.editor", "Film"],
    ["pumi_dev.terminal.open", "Terminal"],
  ])
  expect(list.at(-1)!.alias).toBe("pumi_dev.terminal.shell")
  expect(list[0]!.alias).toBeUndefined()
})

test("new tabs open what new_tab names, else the Launcher's list, else a shell, else the first", () => {
  const all = startables([agents, film, terminal, launcher, editor])
  expect(newTabOf(all, "pumi_dev.agents.new_claude")?.title).toBe("Claude Code")
  expect(newTabOf(all, "pumi_dev.terminal.shell")?.value).toBe("pumi_dev.terminal.open")
  expect(newTabOf(all, "")?.value).toBe("pumi_dev.launcher.new")
  expect(newTabOf(all, "gone.plugin.block")?.value).toBe("pumi_dev.launcher.new")
  expect(newTabOf(startables([agents, terminal]), "")?.value).toBe("pumi_dev.terminal.open")
  expect(newTabOf(startables([editor, agents]), "")?.value).toBe("pumi_dev.agents.new_pi")
  expect(newTabOf(startables([editor]), "")).toBeUndefined()
})

test("Settings offers the + menu's list, with no 'nothing', and shows what new tabs open", () => {
  const { options, value } = newTabChoices([agents, film, terminal, editor], "")
  expect(options).toEqual([
    { value: "pumi_dev.agents.new_pi", label: "Pi · Agents" },
    { value: "pumi_dev.agents.new_claude", label: "Claude Code · Agents" },
    { value: "amresyd.film.editor", label: "Film" },
    { value: "pumi_dev.terminal.open", label: "Terminal" },
  ])
  expect(value).toBe("pumi_dev.terminal.open")
  expect(newTabChoices([], "").options).toEqual([])
})

test("a block new_tab names that the list leaves out is still shown, as it is what opens", () => {
  const { options, value } = newTabChoices([agents], "pumi_dev.agents.term")
  expect(options.at(-1)).toEqual({ value: "pumi_dev.agents.term", label: "Agent · Agents" })
  expect(value).toBe("pumi_dev.agents.term")
})
