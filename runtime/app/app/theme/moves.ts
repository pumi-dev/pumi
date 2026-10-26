// How the default theme moves tabs, views and sessions by dragging them: a tab by its chip in a
// pane's strip or its row in the side bar, a view or a session by its row. What a drag carries, where on its
// target it lands, and what that does, as the server's calls. Nothing here draws (`default.tsx`
// and `tiles.tsx` do) or calls (they run what `plan` says).
import type { PaneLeaf, Snapshot } from "../engine.ts"
import { serverOf } from "../ids.ts"

/** What a drag carries. */
export type Carried = { kind: "pumi.tab"; tab: string } | { kind: "pumi.view"; view: string } | { kind: "pumi.session"; session: string }

export function carried(data: unknown): Carried | undefined {
  const it = data as Carried | undefined
  if (!it || typeof it !== "object") return undefined
  if (it.kind === "pumi.tab" && typeof it.tab === "string") return it
  if (it.kind === "pumi.view" && typeof it.view === "string") return it
  if (it.kind === "pumi.session" && typeof it.session === "string") return it
  return undefined
}

/** A side of a pane, as `tab.update`'s `place.split` names it. */
export type Side = "left" | "right" | "up" | "down"
/** Where on a pane a tab dropped lands: beside it, on one of its sides, or into it, among its tabs (`center`). Tabs never trade places. */
export type Region = Side | "center"
export const REGIONS: readonly Region[] = ["left", "right", "up", "down", "center"]

/** How far in from a side a drop still lands beside the pane, in shares of it, while its middle takes tabs. */
const EDGE = 0.25

/**
 * The region of a pane a point is in: the edge it is nearest, or, while panes hold tabs (`tabs`),
 * its middle when no edge is near. `fx`, `fy`: the point in shares of the pane's width and
 * height, from its left and top.
 */
export function regionAt(fx: number, fy: number, tabs = false): Region {
  const near: [Side, number][] = [
    ["left", fx],
    ["right", 1 - fx],
    ["up", fy],
    ["down", 1 - fy],
  ]
  const [side, by] = near.reduce((a, b) => (b[1] < a[1] ? b : a))
  return tabs && by > EDGE ? "center" : side
}

/** The part of a pane a region shows the pane will take, in shares of it. */
export function regionBox(region: Region): { x: number; y: number; w: number; h: number } {
  switch (region) {
    case "center":
      return { x: 0, y: 0, w: 1, h: 1 }
    case "left":
      return { x: 0, y: 0, w: 0.5, h: 1 }
    case "right":
      return { x: 0.5, y: 0, w: 0.5, h: 1 }
    case "up":
      return { x: 0, y: 0, w: 1, h: 0.5 }
    case "down":
      return { x: 0, y: 0.5, w: 1, h: 0.5 }
  }
}

/**
 * A row of the side bar's tree, as a drop target. A `tab` row that is `alone` is a view of one
 * tab, drawn in the view's place: it stands for the view too.
 */
export type Row =
  | { kind: "session"; session: string }
  | { kind: "view"; session: string; view: string }
  | { kind: "tab"; session: string; view: string; tab: string; alone: boolean }

/** What a drop does. `after`: on the far side of `anchor`, not the near one. */
export type Move =
  | { kind: "session.order"; session: string; anchor: string; after: boolean }
  | { kind: "view.move"; view: string; session: string; anchor?: string; after?: boolean }
  | { kind: "tab.newView"; tab: string; session: string }
  | { kind: "tab.join"; tab: string; view: string }
  | { kind: "tab.beside"; tab: string; target: string; side: Side }
  /** Into a pane, among its tabs: at `index` of its strip, or after the one in front. */
  | { kind: "tab.into"; tab: string; pane: string; index?: number }
  /** In a pane of its own, split from `pane` on `side`. */
  | { kind: "tab.split"; tab: string; pane: string; side: Side }

/** What the moves need to know of where things are now. */
export interface Where {
  viewOf(tab: string): string | undefined
  sessionOf(view: string): string | undefined
  /** How many tiled tabs a view holds. */
  tabsIn(view: string): number
}

export function whereIn(snapshot: Snapshot | undefined): Where {
  const tabs = snapshot?.tabs ?? []
  const views = snapshot?.views ?? []
  const tiled = (p: (typeof tabs)[number]) => {
    const kind = (p.place as { kind?: string } | undefined)?.kind
    return kind === undefined || kind === "tiled"
  }
  return {
    viewOf: (tab) => tabs.find((p) => p.tab_id === tab)?.view_id || undefined,
    sessionOf: (view) => views.find((t) => t.view_id === view)?.workspace_id,
    tabsIn: (view) => tabs.filter((p) => p.view_id === view && tiled(p)).length,
  }
}

/** Whether every id is on the same server: nothing here moves a thing to another one. */
function together(...ids: (string | undefined)[]): boolean {
  const servers = new Set(ids.filter((id) => id !== undefined).map((id) => serverOf(id) ?? ""))
  return servers.size <= 1
}

/**
 * A tab dropped on a pane in the tiled layout, in `region` of it: into it (while panes hold tabs,
 * `tabs`), or in a pane of its own on that side. Nothing for a tab dropped on its own pane's
 * middle, or beside a pane it is alone in.
 */
export function tabDrop(it: Carried, leaf: PaneLeaf, region: Region, tabs = true): Move | undefined {
  if (it.kind !== "pumi.tab" || !together(it.tab, leaf.pane)) return undefined
  const own = leaf.tabs.includes(it.tab)
  if (region === "center") return tabs && !own ? { kind: "tab.into", tab: it.tab, pane: leaf.pane } : undefined
  if (own && leaf.tabs.length < 2) return undefined
  return { kind: "tab.split", tab: it.tab, pane: leaf.pane, side: region }
}

/**
 * A tab dropped on a pane's strip, `index` along it (before the tab at `index`; its length is
 * the end): into the pane there, or along it when it is the tab's own. Nothing where it already is.
 */
export function stripDrop(it: Carried, leaf: PaneLeaf, index: number): Move | undefined {
  if (it.kind !== "pumi.tab" || !together(it.tab, leaf.pane)) return undefined
  const at = leaf.tabs.indexOf(it.tab)
  if (at < 0) return { kind: "tab.into", tab: it.tab, pane: leaf.pane, index }
  // Its own strip: the index it ends at, once it is out of its old one.
  const to = index > at ? index - 1 : index
  return to === at ? undefined : { kind: "tab.into", tab: it.tab, pane: leaf.pane, index: to }
}

/** Where down a row a point is (`fy`, a share of its height): its first or last quarter, or between. */
function slot(fy: number): "before" | "into" | "after" {
  return fy < 0.25 ? "before" : fy > 0.75 ? "after" : "into"
}

/**
 * What `it` dropped on a row of the side bar's tree does, `fy` of the way down it; nothing when
 * it does nothing there. A session sorts among the sessions; a view among the views, into another
 * session too; a tab joins a view, goes beside a tab, or is a new view of a
 * session. A view of one tab is that tab's row: dragged to another view's edge, the view moves.
 * A tab dropped on another tab's row goes before it or after it, never in its place.
 */
export function rowDrop(it: Carried, row: Row, fy: number, where: Where): Move | undefined {
  const here = slot(fy)
  const after = fy >= 0.5
  if (it.kind === "pumi.session") {
    if (row.kind !== "session" || row.session === it.session) return undefined
    return { kind: "session.order", session: it.session, anchor: row.session, after }
  }
  if (it.kind === "pumi.view") {
    if (!together(it.view, row.session)) return undefined
    if (row.kind === "session") {
      return where.sessionOf(it.view) === row.session ? undefined : { kind: "view.move", view: it.view, session: row.session }
    }
    // A tab inside a view of several is not where a view goes.
    if (row.view === it.view || (row.kind === "tab" && !row.alone)) return undefined
    return { kind: "view.move", view: it.view, session: row.session, anchor: row.view, after }
  }
  const tab = it.tab
  const from = where.viewOf(tab)
  if (!together(tab, row.session)) return undefined
  if (row.kind === "session") {
    // Already a view of its own in that session.
    if (from && where.sessionOf(from) === row.session && where.tabsIn(from) === 1) return undefined
    return { kind: "tab.newView", tab, session: row.session }
  }
  // A tab alone in its view, dropped at another view's edge: its view is what moves.
  const viewRow = row.kind === "view" || row.alone
  if (viewRow && here !== "into" && from && from !== row.view && where.tabsIn(from) === 1) {
    return { kind: "view.move", view: from, session: row.session, anchor: row.view, after }
  }
  if (row.kind === "view") return from === row.view ? undefined : { kind: "tab.join", tab, view: row.view }
  if (row.tab === tab) return undefined
  if (row.alone) return { kind: "tab.join", tab, view: row.view }
  return { kind: "tab.beside", tab, target: row.tab, side: after ? "right" : "left" }
}

/** `order` with `id` moved beside `anchor` (before it, or `after` it); at the end without one. */
export function reorder(order: readonly string[], id: string, anchor?: string, after = false): string[] {
  const rest = order.filter((x) => x !== id)
  const at = anchor === undefined ? -1 : rest.indexOf(anchor)
  rest.splice(at < 0 ? rest.length : at + (after ? 1 : 0), 0, id)
  return rest
}

/** Sessions in the order the side bar keeps (`order`); one it has not sorted keeps its place after them. */
export function sortSessions<T extends { workspace_id: string }>(sessions: readonly T[], order: readonly string[]): T[] {
  const rank = (s: T) => {
    const at = order.indexOf(s.workspace_id)
    return at < 0 ? order.length : at
  }
  return sessions.map((s, i) => [s, i] as const).sort((a, b) => rank(a[0]) - rank(b[0]) || a[1] - b[1]).map(([s]) => s)
}

/** One call to the server: `method` on the server `on` is on. */
export interface Step {
  on: string
  method: string
  params: unknown[]
}

/** The calls a move makes, in order (each after the last has answered). Sessions are sorted here, not on the server. */
export function plan(move: Move, snapshot: Snapshot | undefined): Step[] {
  const place = (tab: string, to: Record<string, unknown>): Step => ({ on: tab, method: "tab.update", params: [tab, { place: to }] })
  switch (move.kind) {
    case "session.order":
      return []
    case "tab.newView":
      return [place(move.tab, { kind: "view", session: move.session })]
    case "tab.join":
      return [place(move.tab, { kind: "tiled", view: move.view, split: "right" })]
    case "tab.beside":
      return [place(move.tab, { kind: "tiled", beside: move.target, split: move.side })]
    case "tab.into":
      return [place(move.tab, { kind: "tiled", pane: move.pane, ...(move.index === undefined ? {} : { index: move.index }) })]
    case "tab.split":
      return [place(move.tab, { kind: "tiled", pane: move.pane, split: move.side })]
    case "view.move": {
      const views = (snapshot?.views ?? []).filter((v) => v.workspace_id === move.session).sort((a, b) => a.number - b.number)
      const moving = (snapshot?.views ?? []).find((v) => v.view_id === move.view)
      if (!moving) return []
      const steps: Step[] = []
      const order = views.map((v) => v.view_id)
      const viewPlace = (to: Record<string, unknown>): Step => ({ on: move.view, method: "view.update", params: [move.view, to] })
      if (moving.workspace_id !== move.session) {
        // In at the end, then to its place: an index another view holds is the server's to refuse.
        steps.push(viewPlace({ session: move.session, index: order.length }))
        order.push(move.view)
      }
      // TODO(api-server): `view.update` refuses an index another view holds, so a view cannot be
      // inserted between two others: this asks for the index the order puts it at.
      const index = reorder(order, move.view, move.anchor, move.after).indexOf(move.view)
      if (index >= 0 && index !== order.indexOf(move.view)) steps.push(viewPlace({ index }))
      return steps
    }
  }
}
