import { expect, test } from "bun:test"
import { NOTHING, newTabChoices } from "./new-tab.tsx"

const entry = (id: string, title: string, blocks: { id: string; title?: string }[], active = true) => ({
  id,
  title,
  prefix: `${id.replace("/", ".")}.`,
  active,
  manifest: { name: id.split("/")[1]!, contributes: { blocks } },
})

test("new tabs can open nothing, or a block of a plugin that is on", () => {
  const entries = [
    entry("pumi_dev/launcher", "Launcher", [{ id: "new", title: "New Tab" }]),
    entry("pumi_dev/terminal", "Terminal", [{ id: "shell", title: "Terminal" }]),
    entry("pumi_dev/browser", "Browser", [{ id: "page", title: "Browser" }], false),
  ]
  expect(newTabChoices(entries, "")).toEqual([
    { value: "", label: NOTHING },
    { value: "pumi_dev.launcher.new", label: "New Tab · Launcher" },
    { value: "pumi_dev.terminal.shell", label: "Terminal" },
  ])
})

test("a block no plugin that is on declares is still shown when it is set", () => {
  expect(newTabChoices([], "pumi_dev.terminal.shell").map((o) => o.value)).toEqual(["", "pumi_dev.terminal.shell"])
})
