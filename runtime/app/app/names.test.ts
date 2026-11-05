import { describe, expect, test } from "bun:test"
import type { Snapshot } from "./engine.ts"
import { tabLabel, viewName } from "./names.ts"
import { entries, type Entry } from "./plugins.ts"

const tab = (id: string, more: Partial<Snapshot["tabs"][number]> = {}) => ({ tab_id: id, block: "terminal", view_id: "@1", workspace_id: "$1", status: "", ...more })

function snapshot(label: string | null, focused?: string): Snapshot {
  return {
    workspaces: [],
    views: [{ view_id: "@1", workspace_id: "$1", number: 1, label, status: "" }],
    tabs: [tab("%1", { terminal_title: "zsh" }), tab("%2", { terminal_title: "π - pumi" })],
    layouts: [{ view_id: "@1", zoomed: false, focused_tab_id: focused }],
  } as unknown as Snapshot
}
// Two panes, one tab each: the leaves are panes, and each names the tabs it holds.
const trees = {
  "@1": { axis: "row", ratio: 0.5, first: { pane: ":1", tabs: ["%1"], active: "%1" }, second: { pane: ":2", tabs: ["%2"], active: "%2" } },
} as any

describe("viewName", () => {
  test("a view the user named goes by that name", () => {
    expect(viewName(snapshot("build", "%2"), trees, "@1")).toBe("build")
  })

  test("an unnamed view goes by its last active tab", () => {
    expect(viewName(snapshot(null, "%2"), trees, "@1")).toBe("π - pumi")
  })

  test("without an active tab, by its first", () => {
    expect(viewName(snapshot(null), trees, "@1")).toBe("zsh")
  })

  test("never by its index", () => {
    expect(viewName(snapshot(null), {}, "@9")).not.toMatch(/\d/)
  })
})

describe("tabLabel", () => {
  const launcher: Entry = {
    id: "pumi_dev/launcher",
    prefix: "pumi_dev.launcher.",
    dir: "/nowhere",
    manifest: { name: "launcher", contributes: { blocks: [{ id: "new", title: "New Tab" }] } },
    state: "running",
    active: true,
    sent: true,
  }
  entries.set(launcher.id, launcher)

  test("a tab of a plugin block goes by the block's name in its manifest", () => {
    expect(tabLabel({ block: "pumi_dev.launcher.new" })).toBe("New Tab")
  })

  test("a terminal's title, its directory and its own name still come first", () => {
    expect(tabLabel({ block: "pumi_dev.launcher.new", terminal_title: "zsh" })).toBe("zsh")
    expect(tabLabel({ block: "pumi_dev.launcher.new", cwd: "/src/pumi" })).toBe("pumi")
    expect(tabLabel({ block: "pumi_dev.launcher.new", label: "tabs", terminal_title: "zsh" })).toBe("tabs")
  })

  test("a block no manifest of ours names keeps its id", () => {
    expect(tabLabel({ block: "someone.else.board" })).toBe("someone.else.board")
  })
})
