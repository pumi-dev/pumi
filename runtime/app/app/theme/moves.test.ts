import { describe, expect, test } from "bun:test"
import type { Snapshot } from "../engine.ts"
import { carried, plan, regionAt, reorder, rowDrop, sortSessions, stripDrop, tabDrop, whereIn, type Carried } from "./moves.ts"

const tab = (id: string): Carried => ({ kind: "pumi.tab", tab: id })

/** Two sessions: $1 with @1 (%1, %2) and @2 (%3); $2 with @3 (%4). */
const snapshot = {
  workspaces: [],
  layouts: [],
  views: [
    { view_id: "@1", workspace_id: "$1", number: 0, label: "", status: "" },
    { view_id: "@2", workspace_id: "$1", number: 1, label: "", status: "" },
    { view_id: "@3", workspace_id: "$2", number: 4, label: "", status: "" },
  ],
  tabs: [
    { tab_id: "%1", block: "terminal", view_id: "@1", workspace_id: "$1", status: "" },
    { tab_id: "%2", block: "terminal", view_id: "@1", workspace_id: "$1", status: "" },
    { tab_id: "%3", block: "terminal", view_id: "@2", workspace_id: "$1", status: "" },
    { tab_id: "%4", block: "terminal", view_id: "@3", workspace_id: "$2", status: "" },
  ],
} as Snapshot
const where = whereIn(snapshot)

describe("carried", () => {
  test("only what these drags carry", () => {
    expect(carried(tab("%1"))).toEqual(tab("%1"))
    expect(carried({ kind: "pumi.section", point: "panel", section: "agents" })).toBeUndefined()
    expect(carried(null)).toBeUndefined()
  })
})

describe("regionAt", () => {
  test("the nearest edge, anywhere on the pane, while panes hold no tabs", () => {
    expect(regionAt(0.1, 0.5)).toBe("left")
    expect(regionAt(0.9, 0.5)).toBe("right")
    expect(regionAt(0.5, 0.1)).toBe("up")
    expect(regionAt(0.5, 0.95)).toBe("down")
    // The middle is no place of its own: the nearest edge still takes it.
    expect(regionAt(0.5, 0.45)).toBe("up")
    expect(regionAt(0.55, 0.5)).toBe("right")
    expect(regionAt(0.2, 0.05)).toBe("up")
  })
  test("the middle joins the pane's tabs while panes hold them", () => {
    expect(regionAt(0.5, 0.45, true)).toBe("center")
    expect(regionAt(0.1, 0.5, true)).toBe("left")
    expect(regionAt(0.5, 0.9, true)).toBe("down")
  })
})

describe("tabDrop", () => {
  const pane = { pane: ":3", tabs: ["%3", "%4"], active: "%3" }
  const alone = { pane: ":1", tabs: ["%1"], active: "%1" }
  test("a side splits the pane; the middle joins its tabs", () => {
    expect(tabDrop(tab("%1"), pane, "right")).toEqual({ kind: "tab.split", tab: "%1", pane: ":3", side: "right" })
    expect(tabDrop(tab("%1"), pane, "center")).toEqual({ kind: "tab.into", tab: "%1", pane: ":3" })
    expect(tabDrop(tab("%1"), pane, "center", false)).toBeUndefined()
  })
  test("its own pane: a side only while the pane holds another tab, never the middle", () => {
    expect(tabDrop(tab("%4"), pane, "left")).toEqual({ kind: "tab.split", tab: "%4", pane: ":3", side: "left" })
    expect(tabDrop(tab("%4"), pane, "center")).toBeUndefined()
    expect(tabDrop(tab("%1"), alone, "left")).toBeUndefined()
  })
  test("not onto another server, and not a view", () => {
    expect(tabDrop(tab("devbox/%1"), pane, "left")).toBeUndefined()
    expect(tabDrop({ kind: "pumi.view", view: "@1" }, pane, "left")).toBeUndefined()
  })
})

describe("stripDrop", () => {
  const pane = { pane: ":3", tabs: ["%3", "%4", "%5"], active: "%3" }
  test("another pane's tab goes in at the index", () => {
    expect(stripDrop(tab("%1"), pane, 1)).toEqual({ kind: "tab.into", tab: "%1", pane: ":3", index: 1 })
  })
  test("its own moves along the strip, and nowhere when it would stay", () => {
    expect(stripDrop(tab("%3"), pane, 3)).toEqual({ kind: "tab.into", tab: "%3", pane: ":3", index: 2 })
    expect(stripDrop(tab("%5"), pane, 0)).toEqual({ kind: "tab.into", tab: "%5", pane: ":3", index: 0 })
    expect(stripDrop(tab("%4"), pane, 1)).toBeUndefined()
    expect(stripDrop(tab("%4"), pane, 2)).toBeUndefined()
  })
})

describe("rowDrop", () => {
  const session = { kind: "session", session: "$2" } as const
  const view = { kind: "view", session: "$1", view: "@1" } as const
  const inView = { kind: "tab", session: "$1", view: "@1", tab: "%2", alone: false } as const
  const alone = { kind: "tab", session: "$1", view: "@2", tab: "%3", alone: true } as const

  test("sessions sort among sessions", () => {
    const it: Carried = { kind: "pumi.session", session: "$1" }
    expect(rowDrop(it, session, 0.8, where)).toEqual({ kind: "session.order", session: "$1", anchor: "$2", after: true })
    expect(rowDrop(it, { kind: "session", session: "$1" }, 0.2, where)).toBeUndefined()
    expect(rowDrop(it, view, 0.2, where)).toBeUndefined()
  })

  test("views sort among views, and go to another session", () => {
    const it: Carried = { kind: "pumi.view", view: "@1" }
    expect(rowDrop(it, alone, 0.9, where)).toEqual({ kind: "view.move", view: "@1", session: "$1", anchor: "@2", after: true })
    expect(rowDrop(it, session, 0.5, where)).toEqual({ kind: "view.move", view: "@1", session: "$2" })
    expect(rowDrop(it, { kind: "session", session: "$1" }, 0.5, where)).toBeUndefined()
    expect(rowDrop(it, inView, 0.5, where)).toBeUndefined()
  })

  test("a tab joins a view, is a new view of a session, or goes beside a tab", () => {
    expect(rowDrop(tab("%4"), view, 0.5, where)).toEqual({ kind: "tab.join", tab: "%4", view: "@1" })
    expect(rowDrop(tab("%1"), view, 0.5, where)).toBeUndefined()
    expect(rowDrop(tab("%1"), session, 0.5, where)).toEqual({ kind: "tab.newView", tab: "%1", session: "$2" })
    // %4 already is a view of its own in $2.
    expect(rowDrop(tab("%4"), session, 0.5, where)).toBeUndefined()
    expect(rowDrop(tab("%1"), inView, 0.1, where)).toEqual({ kind: "tab.beside", tab: "%1", target: "%2", side: "left" })
    // Never in its place: before it or after it, by the half it lands in.
    expect(rowDrop(tab("%1"), inView, 0.4, where)).toEqual({ kind: "tab.beside", tab: "%1", target: "%2", side: "left" })
    expect(rowDrop(tab("%1"), inView, 0.6, where)).toEqual({ kind: "tab.beside", tab: "%1", target: "%2", side: "right" })
    expect(rowDrop(tab("%1"), alone, 0.5, where)).toEqual({ kind: "tab.join", tab: "%1", view: "@2" })
  })

  test("a tab alone in its view, at another view's edge, moves its view", () => {
    expect(rowDrop(tab("%3"), view, 0.1, where)).toEqual({ kind: "view.move", view: "@2", session: "$1", anchor: "@1", after: false })
    expect(rowDrop(tab("%3"), view, 0.5, where)).toEqual({ kind: "tab.join", tab: "%3", view: "@1" })
    // Not alone: it joins the view wherever it lands.
    expect(rowDrop(tab("%1"), alone, 0.9, where)).toEqual({ kind: "tab.join", tab: "%1", view: "@2" })
  })
})

describe("ordering", () => {
  test("reorder moves one view, the others keeping their order", () => {
    const order = ["a", "b", "c", "d"]
    expect(reorder(order, "a", "c", true)).toEqual(["b", "c", "a", "d"])
    expect(reorder(order, "d", "b")).toEqual(["a", "d", "b", "c"])
    expect(reorder(order, "b", "c")).toEqual(["a", "b", "c", "d"])
    expect(reorder(order, "b")).toEqual(["a", "c", "d", "b"])
  })

  test("sessions sort by the kept order, the rest after in their own", () => {
    const s = (id: string) => ({ workspace_id: id })
    expect(sortSessions([s("$1"), s("$2"), s("$3"), s("$4")], ["$3", "$1"]).map((x) => x.workspace_id)).toEqual(["$3", "$1", "$2", "$4"])
  })
})

describe("plan", () => {
  test("a view into another session goes in at the end, then to its index", () => {
    expect(plan({ kind: "view.move", view: "@2", session: "$2", anchor: "@3", after: false }, snapshot)).toEqual([
      { on: "@2", method: "view.update", params: ["@2", { session: "$2", index: 1 }] },
      { on: "@2", method: "view.update", params: ["@2", { index: 0 }] },
    ])
  })

  test("a tab's moves are one update", () => {
    expect(plan({ kind: "tab.beside", tab: "%1", target: "%3", side: "down" }, snapshot)).toEqual([{ on: "%1", method: "tab.update", params: ["%1", { place: { kind: "tiled", beside: "%3", split: "down" } }] }])
    expect(plan({ kind: "tab.newView", tab: "%1", session: "$2" }, snapshot)).toEqual([{ on: "%1", method: "tab.update", params: ["%1", { place: { kind: "view", session: "$2" } }] }])
    expect(plan({ kind: "tab.into", tab: "%1", pane: ":3", index: 0 }, snapshot)).toEqual([{ on: "%1", method: "tab.update", params: ["%1", { place: { kind: "tiled", pane: ":3", index: 0 } }] }])
    expect(plan({ kind: "tab.split", tab: "%1", pane: ":3", side: "left" }, snapshot)).toEqual([{ on: "%1", method: "tab.update", params: ["%1", { place: { kind: "tiled", pane: ":3", split: "left" } }] }])
  })
})
