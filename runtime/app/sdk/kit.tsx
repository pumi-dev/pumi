// What a theme reaches for that is not the server's state: the window's chrome, time, hover,
// dragging, and a layout's splits. Everything here is plain React over the renderer's events,
// so a theme can use it from any part, or not at all.
import { useCallback, useEffect, useRef, useState } from "react"
import type { EventPayload } from "../native/addon"
import { call, store, via, type Tree } from "../app/engine.ts"
import { useWindow } from "../app/state.ts"

// ── The window ───────────────────────────────────────────────────────

export interface WindowChrome {
  /** Full screen: the traffic lights are hidden and the title bar is the theme's to use. */
  fullscreen: boolean
  /** The window is the key window. */
  active: boolean
  /** The traffic lights' box in window px, while they show. */
  trafficLights?: { x: number; y: number; width: number; height: number }
  /** How far from the left a title bar's content starts to clear the traffic lights (0 in full screen). */
  inset: number
}

/** The window's own chrome, for a theme that draws its title bar. */
export function useWindowChrome(): WindowChrome {
  const window = useWindow()
  const lights = window.trafficLights
  return {
    fullscreen: window.fullscreened,
    active: window.active,
    trafficLights: lights,
    inset: lights ? lights.x + lights.width + 12 : 0,
  }
}

// ── Time ─────────────────────────────────────────────────────────────

/** Calls `callback` every `ms` (none while `ms` is null). The latest callback is the one called. */
export function useInterval(callback: () => void, ms: number | null): void {
  const latest = useRef(callback)
  latest.current = callback
  useEffect(() => {
    if (ms === null || !(ms > 0)) return
    const timer = setInterval(() => latest.current(), ms)
    return () => clearInterval(timer)
  }, [ms])
}

/** `Date.now()`, again every `ms`: a clock, a blink, a game tick. */
export function useNow(ms = 1000): number {
  const [now, setNow] = useState(() => Date.now())
  useInterval(() => setNow(Date.now()), ms)
  return now
}

// ── Hover ────────────────────────────────────────────────────────────

/** Whether the mouse is over an element, and the props that say so (spread them on it). With
 *  `delay`, it turns true only once the mouse has stayed that long (a hover card). */
export function useHover(delay = 0): [boolean, { onMouseEnter(): void; onMouseLeave(): void }] {
  const [hovered, setHovered] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  useEffect(() => () => clearTimeout(timer.current), [])
  const onMouseEnter = useCallback(() => {
    clearTimeout(timer.current)
    if (delay > 0) timer.current = setTimeout(() => setHovered(true), delay)
    else setHovered(true)
  }, [delay])
  const onMouseLeave = useCallback(() => {
    clearTimeout(timer.current)
    setHovered(false)
  }, [])
  return [hovered, { onMouseEnter, onMouseLeave }]
}

// ── Drag and drop ────────────────────────────────────────────────────

export { useDragState, useDraggable, useDroppable, type Drag, type DraggableOptions, type DroppableOptions } from "./drag.ts"

// ── A layout's splits ────────────────────────────────────────────────

/** Which way to go from a split to one of its sides: false the first, true the second. */
export type SplitPath = boolean[]

/** The split at `path` in `tree`, if there is one. */
export function splitAt(tree: Tree, path: SplitPath): Extract<Tree, { axis: string }> | undefined {
  let node: Tree = tree
  for (const second of path) {
    if (!("axis" in node)) return undefined
    node = second ? node.second : node.first
  }
  return "axis" in node ? node : undefined
}

/** Which of the sides of the pane holding `tab` (or the pane itself, by its id) face another pane (the rest face the window's edge). */
export function paneEdges(tree: Tree, tab: string): { top: boolean; right: boolean; bottom: boolean; left: boolean } | undefined {
  const found = { top: false, right: false, bottom: false, left: false }
  const walk = (node: Tree): boolean => {
    if ("pane" in node) return node.pane === tab || node.tabs.includes(tab)
    if (!("axis" in node)) return false
    if (walk(node.first)) {
      if (node.axis === "row") found.right = true
      else found.bottom = true
      return true
    }
    if (walk(node.second)) {
      if (node.axis === "row") found.left = true
      else found.top = true
      return true
    }
    return false
  }
  return walk(tree) ? found : undefined
}

/** `tree` in `view.update`'s shape, each side with its share; nothing for an app in it. */
function layoutOf(node: Tree): Exclude<Tree, { app: number }> {
  if ("pane" in node) return { pane: node.pane, tabs: [] }
  if (!("axis" in node)) throw new Error("pumi: a layout with an app in it cannot be resized here")
  return { axis: node.axis, ratio: node.ratio, first: layoutOf(node.first), second: layoutOf(node.second) }
}

function withRatio(node: Tree, path: SplitPath, ratio: number): Tree {
  if (!("axis" in node)) throw new Error("pumi: no split there")
  if (path.length === 0) return { ...node, ratio }
  const [second, ...rest] = path
  return second ? { ...node, second: withRatio(node.second, rest, ratio) } : { ...node, first: withRatio(node.first, rest, ratio) }
}

/**
 * Sets the share of a split's first side (`ratio`, kept within 0.05 to 0.95) in `view`'s layout:
 * what a theme's divider calls while it is dragged.
 */
export async function setSplitRatio(view: string, path: SplitPath, ratio: number): Promise<void> {
  const tree = store.get().trees[view]
  if (!tree) throw new Error(`pumi: no layout for the view ${view}`)
  const next = withRatio(tree, path, Math.min(0.95, Math.max(0.05, ratio)))
  await call(via(view, "view.update"), view, { layout: layoutOf(next) })
}

/** Sends `tree` as `view`'s layout: what `equalize` and a theme's own layout edits use. */
export async function setLayout(view: string, tree: Tree): Promise<void> {
  await call(via(view, "view.update"), view, { layout: layoutOf(tree) })
}

/** How many panes a side of a split holds along an axis: what even shares are counted in. */
function span(node: Tree, axis: "row" | "column"): number {
  return "axis" in node && node.axis === axis ? span(node.first, axis) + span(node.second, axis) : 1
}

/**
 * `tree` with every split's share evened out for the panes it holds (tmux's `equalalways`): each
 * split's first side takes as much as the panes it holds, so the panes end up the same size.
 * `setLayout(view, equalize(tree))` is what a theme does when a tab opens or closes.
 */
export function equalize(tree: Tree): Tree {
  if (!("axis" in tree)) return tree
  const a = span(tree.first, tree.axis)
  const b = span(tree.second, tree.axis)
  const ratio = Math.min(0.9, Math.max(0.1, a / (a + b)))
  return { ...tree, ratio, first: equalize(tree.first), second: equalize(tree.second) }
}

/**
 * A divider for a split the theme draws: spread `props` on the element the user drags, over the
 * edge between the split's sides. `ratio` and `axis` are the split's own (from the theme's tree)
 * and `size` its box along that axis, so no store read is needed to start; `onPreview` is called
 * with every ratio the drag passes, so the theme can draw it before the server answers. Ratios go
 * to the server at most every `throttle` ms while it drags, and once when it lets go.
 */
export function useSplitDrag({
  view,
  path,
  ratio,
  axis,
  size,
  onPreview,
  throttle = 32,
}: {
  view: string | undefined
  path: SplitPath
  ratio: number
  axis: "row" | "column"
  size: number
  onPreview?(ratio: number): void
  throttle?: number
}): { props: { onMouseDown(event: EventPayload): void; onMouseMove(event: EventPayload): void; onMouseUp(event: EventPayload): void }; dragging: boolean } {
  const from = useRef<{ at: number; ratio: number } | null>(null)
  const last = useRef(0)
  const [dragging, setDragging] = useState(false)
  const row = axis === "row"
  const latest = useRef({ view, path, ratio, size, onPreview })
  latest.current = { view, path, ratio, size, onPreview }
  const commit = useCallback((to: number) => {
    const now = latest.current
    if (!now.view) return
    setSplitRatio(now.view, now.path, to).catch((err) => console.error("pumi: resize:", err?.message ?? err))
  }, [])
  const next = useCallback((event: EventPayload) => {
    const start = from.current
    const now = latest.current
    if (!start || now.size <= 0) return undefined
    const delta = ((row ? event.x : event.y) ?? start.at) - start.at
    const to = Math.min(0.95, Math.max(0.05, start.ratio + delta / now.size))
    now.onPreview?.(to)
    return to
  }, [row])
  const onMouseDown = useCallback((event: EventPayload) => {
    from.current = { at: (row ? event.x : event.y) ?? 0, ratio: latest.current.ratio }
    last.current = 0
    setDragging(true)
  }, [row])
  const onMouseMove = useCallback(
    (event: EventPayload) => {
      if (!from.current) return
      // No button down any more: the release was missed (it ended outside the window).
      if (event.pressedButton === undefined || event.pressedButton === null) {
        from.current = null
        setDragging(false)
        return
      }
      const at = Date.now()
      if (at - last.current < throttle) {
        next(event)
        return
      }
      last.current = at
      const to = next(event)
      if (to !== undefined) commit(to)
    },
    [next, commit, throttle],
  )
  const onMouseUp = useCallback(
    (event: EventPayload) => {
      if (!from.current) return
      const to = next(event)
      from.current = null
      setDragging(false)
      if (to !== undefined) commit(to)
    },
    [next, commit],
  )
  return { props: { onMouseDown, onMouseMove, onMouseUp }, dragging }
}
