import { describe, expect, test } from "bun:test"
import type { Snapshot } from "./engine.ts"
import { viewName } from "./names.ts"

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
