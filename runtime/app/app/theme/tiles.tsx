/**
 * A view's tiled panes, as the default theme draws them: flat, keyed by pane, and moved.
 *
 * The server's layout is a list of panes per split; the app folds it into pairs (`[a, b, c]` is
 * `a | (b | c)`), so opening or closing one pane reshapes the tree around its neighbours. Drawn as
 * nested splits, that reshaping would put every pane under a new parent and React would mount them
 * again: each terminal drawn blank until it attached, and the splits jumping instead of moving.
 * Here every pane is a child of one box wherever the tree puts it, so a pane is never mounted again
 * while it stays in its view; it moves from where it was to where it goes.
 */
import { createContext, useContext, useEffect, useRef, useState } from "react"
import { useDragState } from "../../sdk/drag.ts"
import type { PaneLeaf, Tree } from "../state.ts"
import type { StyleDesc } from "../../native/host.ts"
import { useComponents, useLayoutStyle, useTheme } from "../../sdk/theme.ts"
import { useAnimate } from "../theme-settings.ts"
import { usePaneTabs } from "../layout.ts"
import { useDropBox, within } from "./drop.tsx"
import * as moves from "./moves.ts"

type Branch = Extract<Tree, { axis: unknown }>

/** How a pane moves to its new place: fast, easing out, with nothing faded. */
const PANE_MOTION = { duration: 0.16, ease: [0.33, 1, 0.68, 1] as [number, number, number, number] }
/** When a move has finished: the pane is drawn by its shares again, and a closed one not at all. */
const PANE_MOTION_MS = 190
/** The fewest pixels a side of a split is dragged down to. */
const MIN_SIDE = 48
/** A share smaller than this is nothing: a side shrunk away. */
const NOTHING = 0.0005

/**
 * What the panes of a view move for. `zoom`: the pane growing over the others, which stay where
 * they are under it; `unzoom`, what a press on them (round the zoomed pane) does instead of
 * reaching them.
 */
export interface TileMotion {
  zoom?: string
  unzoom?(): void
}

export const TileMotion = createContext<TileMotion>({})

/** Whether the pane drawn here meets the top of its layout (or covers it, zoomed): its bar is in the title bar's row. */
export const AtTop = createContext(false)

/** What a view's edges do to its layout, and what a tab dropped on one of its panes does. */
export interface TilingEdit {
  resize(node: Branch, ratio: number): Promise<void>
  equalize(): void
  /** Without it, a tab dragged over the view's panes is not dropped there. */
  move?(move: moves.Move): void
}

const pct = (n: number) => `${n * 100}%`
const swallow = (err: unknown) => console.error(String(err))

/** A pane's place in the space its layout has, in shares of it: from the left and the top, and its size. */
interface Box {
  x: number
  y: number
  w: number
  h: number
}

const WHOLE: Box = { x: 0, y: 0, w: 1, h: 1 }

/** A split as it is drawn: where it is in its tree (the sides taken from the root), its box, its first side's share. */
interface Seam {
  node: Branch
  path: string
  box: Box
  ratio: number
}

/** The panes in a tree, in order, by id. */
function leavesIn(tree: Tree, out = new Map<string, PaneLeaf>()): Map<string, PaneLeaf> {
  if ("pane" in tree) out.set(tree.pane, tree)
  else if ("axis" in tree) {
    leavesIn(tree.first, out)
    leavesIn(tree.second, out)
  }
  return out
}

/** The panes' ids in a tree, in order. */
function panesIn(tree: Tree): string[] {
  return [...leavesIn(tree).keys()]
}

/** Where each pane of a tree is, each split at the share `ratioOf` gives it; `seams` collects the splits. */
export function boxesOf(
  tree: Tree,
  ratioOf: (node: Branch, path: string) => number = (node) => node.ratio,
  seams?: Seam[],
  box: Box = WHOLE,
  path = "",
  out = new Map<string, Box>(),
): Map<string, Box> {
  if ("pane" in tree) out.set(tree.pane, box)
  else if ("axis" in tree) {
    const r = ratioOf(tree, path)
    seams?.push({ node: tree, path, box, ratio: r })
    const row = tree.axis === "row"
    const first = row ? { ...box, w: box.w * r } : { ...box, h: box.h * r }
    const second = row ? { ...box, x: box.x + box.w * r, w: box.w * (1 - r) } : { ...box, y: box.y + box.h * r, h: box.h * (1 - r) }
    boxesOf(tree.first, ratioOf, seams, first, `${path}f`, out)
    boxesOf(tree.second, ratioOf, seams, second, `${path}s`, out)
  }
  return out
}

/** The split at `path` in a tree, if it still has one there. */
function seamAt(tree: Tree, path: string): Branch | undefined {
  let node = tree
  for (const step of path) {
    if (!("axis" in node)) return undefined
    node = step === "f" ? node.first : node.second
  }
  return "axis" in node ? node : undefined
}

/**
 * A pane as a sliver of nothing on the seam between it and the panes beside it in `tree`, where
 * `boxes` has those: a pane opened grows out of there, and a closed one shrinks into it. Nothing
 * when no pane beside it is in `boxes`.
 */
export function seamOf(tree: Tree, pane: string, boxes: ReadonlyMap<string, Box>): Box | undefined {
  // The splits from the root down to the pane, and the side of each it is on.
  const chain: { node: Branch; first: boolean }[] = []
  const find = (node: Tree): boolean => {
    if ("pane" in node) return node.pane === pane
    if (!("axis" in node)) return false
    for (const first of [true, false]) {
      chain.push({ node, first })
      if (find(first ? node.first : node.second)) return true
      chain.pop()
    }
    return false
  }
  if (!find(tree)) return undefined
  for (let i = chain.length - 1; i >= 0; i--) {
    const { node, first } = chain[i]
    const near = panesIn(first ? node.second : node.first).flatMap((p) => boxes.get(p) ?? [])
    if (!near.length) continue
    const x = Math.min(...near.map((b) => b.x))
    const y = Math.min(...near.map((b) => b.y))
    const right = Math.max(...near.map((b) => b.x + b.w))
    const bottom = Math.max(...near.map((b) => b.y + b.h))
    return node.axis === "row" ? { x: first ? x : right, y, w: 0, h: bottom - y } : { x, y: first ? y : bottom, w: right - x, h: 0 }
  }
  return undefined
}

const NO_GONE: ReadonlyMap<string, Tree> = new Map()

/**
 * The panes of a tree, side by side as its splits share the space (a percentage, not their
 * content's size), with an edge on each split that drags to move it (a double click makes the
 * view's splits even). Without `edit` the edges are not drawn. A tab dragged over one of them
 * (by its chip in a strip, or its row in the side bar) drops on the side of it nearest the mouse,
 * to split it there, or in its middle, to join its tabs; tabs never trade places.
 *
 * A pane moves from where it was drawn to its new place; one opened grows out of the seam it was
 * split at, one closed shrinks into its neighbours, and a zoomed one grows over the rest, to a
 * little short of the layout's edges, so they show under it where they are.
 */
export function Tiling({ tree, focused, edit }: { tree: Tree; /** The tab with the focus: its pane has it. */ focused?: string; edit?: TilingEdit }) {
  const { zoom, unzoom } = useContext(TileMotion)
  const look = useLayoutStyle()
  // The theme's `animate` off: every pane is drawn where it goes at once, and a closed one goes.
  const animate = useAnimate()
  // The box's size in pixels: what a drag's pixels are a share of, and what a move is played in
  // (a motion moves pixels, not shares).
  const size = useRef<{ w: number; h: number } | undefined>(undefined)
  const [drag, setDrag] = useState<{ path: string; at: number; from: number; to: number; length: number }>()
  // The share a drag ended at, drawn until the server's tree comes back with it: `was` is the
  // split's share then, so a tree that has moved it on drops it.
  const [held, setHeld] = useState<{ path: string; ratio: number; was: number }>()
  const hold = held && seamAt(tree, held.path)?.ratio === held.was ? held : undefined
  useEffect(() => {
    if (held && !hold) setHeld(undefined)
  }, [held, hold])
  // The panes as they are now, and as each closed one last was, for its tabs.
  const leaves = leavesIn(tree)
  const focusedPane = [...leaves.values()].find((p) => focused !== undefined && p.tabs.includes(focused))?.pane
  // Closed panes, still drawn while they shrink away, each with the tree it was last in.
  const [gone, setGone] = useState(NO_GONE)
  // What was drawn last, and where each pane opened since starts from (read as it mounts).
  const drawn = useRef<ReadonlyMap<string, Box>>(new Map())
  const starts = useRef(new Map<string, Box>())
  const [prev, setPrev] = useState(tree)
  if (tree !== prev) {
    setPrev(tree)
    const now = new Set(panesIn(tree))
    const kept = [...gone].filter(([p]) => !now.has(p))
    const closed = animate ? panesIn(prev).filter((p) => !now.has(p)) : []
    if (closed.length || kept.length !== gone.size) setGone(new Map([...kept, ...closed.map((p) => [p, prev] as const)]))
    for (const p of now) {
      const start = !animate || drawn.current.has(p) ? undefined : seamOf(tree, p, drawn.current)
      if (start) starts.current.set(p, start)
    }
  }
  useEffect(() => {
    if (!gone.size) return
    const timer = setTimeout(() => setGone(NO_GONE), PANE_MOTION_MS)
    return () => clearTimeout(timer)
  }, [gone])

  const ratioOf = (node: Branch, path: string) =>
    drag?.path === path ? drag.to : hold?.path === path ? hold.ratio : node.ratio
  const seams: Seam[] = []
  const boxes = boxesOf(tree, ratioOf, seams)
  const all = new Map(boxes)
  for (const [p, last] of gone) all.set(p, seamOf(last, p, boxes) ?? { ...(drawn.current.get(p) ?? WHOLE), w: 0 })
  if (zoom !== undefined && all.has(zoom)) {
    const px = size.current
    const ix = px ? Math.min(0.1, look.zoomInset / px.w) : 0.015
    const iy = px ? Math.min(0.1, look.zoomInset / px.h) : 0.015
    all.set(zoom, { x: ix, y: iy, w: 1 - 2 * ix, h: 1 - 2 * iy })
  }
  useEffect(() => {
    drawn.current = all
    for (const p of starts.current.keys()) if (!all.has(p)) starts.current.delete(p)
  })
  // In id order, not the tree's: a pane keeps its place among the children as the tree reshapes.
  const order = [...all.keys()].sort((a, b) => a.localeCompare(b, undefined, { numeric: true }))

  const end = () => {
    if (!drag) return
    setDrag(undefined)
    const node = seamAt(tree, drag.path)
    if (!edit || !node || Math.abs(drag.to - node.ratio) < 0.001) return
    setHeld({ path: drag.path, ratio: drag.to, was: node.ratio })
    edit.resize(node, drag.to).catch((err) => {
      setHeld(undefined)
      swallow(err)
    })
  }
  return (
    <div style={{ display: "flex", flexGrow: 1, width: "100%", height: "100%", minWidth: 0, minHeight: 0, overflow: "hidden", position: "relative" }}>
    {/* The panes' space: the layout's, less the theme's gap round it. */}
    <div
      onLayout={(e: any) => {
        if (e.width > 0 && e.height > 0) size.current = { w: e.width, h: e.height }
      }}
      style={{ position: "absolute", top: look.around, left: look.around, right: look.around, bottom: look.around, display: "flex" }}
    >
      {order.map((p) => (
        <Tile
          key={p}
          pane={p}
          leaf={leaves.get(p) ?? leavesIn(gone.get(p) ?? tree).get(p) ?? { pane: p, tabs: [] }}
          box={all.get(p)!}
          from={starts.current.get(p)}
          size={size}
          still={drag !== undefined || !animate}
          frozen={gone.has(p)}
          focused={p === focusedPane}
          zoomed={p === zoom}
          gap={look.between}
          divider={look.divider}
          lifted={look.lifted}
          // While one is zoomed it covers the rest: it is the one a tab drops on.
          move={zoom === undefined || p === zoom ? edit?.move : undefined}
        />
      ))}
      {zoom !== undefined && unzoom && all.has(zoom) ? (
        // Between the zoomed pane and the rest: a press beside it gives the view its layout back,
        // and does not reach (or focus) the pane under it.
        <div
          onMouseDown={unzoom}
          style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", zIndex: 1 }}
        />
      ) : null}
      {edit && zoom === undefined
        ? seams.map((seam) => {
            const row = seam.node.axis === "row"
            const at = (e: EdgeEvent) => (row ? e.x : e.y) ?? 0
            const mine = drag?.path === seam.path ? drag : undefined
            return (
              <Edge
                key={seam.path}
                seam={seam}
                gap={look.between}
                dragging={mine !== undefined}
                onMouseDown={(e) => {
                  const px = size.current
                  const length = (row ? seam.box.w * (px?.w ?? 0) : seam.box.h * (px?.h ?? 0)) || 1
                  setDrag({ path: seam.path, at: at(e), from: seam.ratio, to: seam.ratio, length })
                }}
                onMouseMove={(e) => {
                  if (!mine) return
                  // The button came up somewhere the release was not heard.
                  if (e.pressedButton !== 0) return end()
                  const least = Math.max(0.05, MIN_SIDE / mine.length)
                  const r = mine.from + (at(e) - mine.at) / mine.length
                  setDrag({ ...mine, to: least >= 0.5 ? 0.5 : Math.min(1 - least, Math.max(least, r)) })
                }}
                onMouseUp={end}
                onDoubleClick={() => edit.equalize()}
              />
            )
          })
        : null}
    </div>
    </div>
  )
}

let nextFlight = 0

/** Whether two boxes are further apart than half a pixel, at any of their edges. */
function apart(a: Box, b: Box, px: { w: number; h: number }): boolean {
  const dx = Math.max(Math.abs(a.x - b.x), Math.abs(a.w - b.w)) * px.w
  const dy = Math.max(Math.abs(a.y - b.y), Math.abs(a.h - b.h)) * px.h
  return Math.max(dx, dy) > 0.5
}

const same = (a: Box, b: Box) => a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h

/**
 * One pane, placed by its shares: over the whole layout (letting the mouse through), a spacer
 * above it and one before it. A move plays on those and on its size, in pixels; at rest they are
 * percentages again, so a window resized carries the pane with it.
 */
function Tile({
  box,
  from,
  size,
  still,
  frozen,
  move,
  gap,
  divider,
  lifted: liftedStyle,
  ...props
}: {
  pane: string
  /** The pane as the tree has it: its tabs and the one in front. */
  leaf: PaneLeaf
  box: Box
  /** The theme's room between panes, its line between ones that touch, and its frame for one lifted (`ThemeSpec.layout`). */
  gap: number
  divider: number
  lifted: StyleDesc
  from?: Box
  size: { current: { w: number; h: number } | undefined }
  /** Drawn where it is told, unmoved: while an edge drags, and while the theme's `animate` is off. */
  still: boolean
  frozen: boolean
  focused: boolean
  zoomed: boolean
  move?(move: moves.Move): void
}) {
  const t = useTheme()
  // The box last drawn at rest; a move plays from it to the goal.
  const [shown, setShown] = useState(() => from ?? box)
  const px = size.current
  const moving = !still && px !== undefined && apart(shown, box, px)
  if (!moving && !same(shown, box)) setShown(box)
  // The move playing: its keyframes stay the same across renders, so it is not started again,
  // and a new goal turns it from where it is (`null`), not from where it began.
  const flight = useRef<{ goal: Box; from?: Box; generation: number } | undefined>(undefined)
  if (!moving) flight.current = undefined
  else if (!flight.current || !same(flight.current.goal, box)) {
    flight.current = { goal: box, from: flight.current ? undefined : shown, generation: ++nextFlight }
  }
  const f = flight.current
  useEffect(() => {
    if (!f) return
    const timer = setTimeout(() => setShown(f.goal), PANE_MOTION_MS)
    return () => clearTimeout(timer)
  }, [f])
  // One length of one of the boxes, moving. The four start in the same render, so they keep
  // together; at rest there is no motion at all, so nothing holds a pixel size.
  const track = (prop: "width" | "height", key: keyof Box) => {
    if (!f || !px) return {}
    const scale = prop === "width" ? px.w : px.h
    const animate = { [prop]: [f.from ? f.from[key] * scale : null, f.goal[key] * scale] }
    return { motion: { generation: f.generation, animate, transition: PANE_MOTION } as any }
  }
  // Shrunk away and at rest: kept, so it is not attached again, but not painted.
  const hidden = !f && (box.w < NOTHING || box.h < NOTHING)
  // Over the others while it is zoomed, and until it has gone back to its place.
  const [lifted, setLifted] = useState(props.zoomed)
  if (props.zoomed && !lifted) setLifted(true)
  else if (!props.zoomed && lifted && !f) setLifted(false)
  return (
    <div style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", display: "flex", flexDirection: "column", pointerEvents: "none", zIndex: lifted ? 2 : undefined }}>
      <div {...track("height", "y")} style={{ height: pct(shown.y), flexShrink: 0 }} />
      <div {...track("height", "h")} style={{ display: "flex", flexDirection: "row", height: pct(shown.h), flexShrink: 0 }}>
        <div {...track("width", "x")} style={{ width: pct(shown.x), flexShrink: 0 }} />
        <div
          {...track("width", "w")}
          style={{
            display: "flex",
            width: pct(shown.w),
            flexShrink: 0,
            visibility: hidden ? "hidden" : undefined,
            // Half the gap on each side that meets another pane (none on the layout's edges).
            ...(gap > 0 && !lifted ? room(box, gap / 2) : {}),
          }}
        >
          <div
            style={{
              display: "flex",
              flexGrow: 1,
              minWidth: 0,
              minHeight: 0,
              overflow: "hidden",
              position: "relative",
              pointerEvents: "auto",
              borderColor: t.border,
              ...(lifted
                ? // A card over the panes (by default its own background, so a component, which
                  // leaves its background unset, does not show the panes under it through it).
                  liftedStyle
                : gap > 0
                  ? {}
                  : { borderLeftWidth: box.x > NOTHING ? divider : 0, borderTopWidth: box.y > NOTHING ? divider : 0 }),
            }}
          >
            <AtTop.Provider value={lifted || box.y < NOTHING}>
              <Leaf frozen={frozen} lifted={lifted} {...props} />
            </AtTop.Provider>
            {move && !frozen ? <PaneDrop leaf={props.leaf} move={move} /> : null}
          </div>
        </div>
      </div>
    </div>
  )
}

/** Padding of `px` on each side of `box` that meets another pane's. */
function room(box: Box, px: number) {
  return {
    paddingLeft: box.x > NOTHING ? px : 0,
    paddingTop: box.y > NOTHING ? px : 0,
    paddingRight: box.x + box.w < 1 - NOTHING ? px : 0,
    paddingBottom: box.y + box.h < 1 - NOTHING ? px : 0,
  }
}

/** The last size each pane was drawn at, so a pane that shrinks away keeps its content's size. */
const paneSizes = new Map<string, { width: number; height: number }>()

/**
 * A pane's box in the layout. A `frozen` one keeps the size it was last drawn at while its box
 * shrinks around it (clipped), so a terminal is not told every size on the way down to nothing.
 */
function Leaf({ frozen, leaf, ...props }: { pane: string; leaf: PaneLeaf; focused: boolean; zoomed: boolean; lifted: boolean; frozen?: boolean }) {
  const C = useComponents()
  const size = frozen ? paneSizes.get(props.pane) : undefined
  useEffect(() => () => void paneSizes.delete(props.pane), [props.pane])
  // Each pane is drawn on its own (`cache`): a terminal's output redraws its pane, not the window.
  return (
    <div
      cache
      onLayout={(e: any) => {
        if (!frozen && e.width > 0 && e.height > 0) paneSizes.set(props.pane, { width: e.width, height: e.height })
      }}
      style={{ display: "flex", minWidth: 0, minHeight: 0, ...(size ? { ...size, flexShrink: 0 } : { width: "100%", height: "100%" }) }}
    >
      <C.Pane {...props} tabs={leaf.tabs} active={leaf.active ?? leaf.tabs[0]} />
    </div>
  )
}

/** A pane's split zone's priority: below every other drop target (`useDroppable`'s `priority`). */
const PANE_DROP_PRIORITY = -1

/** Whether a tab dragged could drop on `leaf` somewhere: on one of its sides, or into it. */
const takes = (leaf: PaneLeaf, tabs: boolean) => (data: unknown): data is moves.Carried => {
  const it = moves.carried(data)
  return !!it && moves.REGIONS.some((r) => moves.tabDrop(it, leaf, r, tabs) !== undefined)
}

/** A pane's drop zone, over it while a tab that could drop on it drags (and nothing then). */
function PaneDrop({ leaf, move }: { leaf: PaneLeaf; move(move: moves.Move): void }) {
  const drag = useDragState()
  const tabs = usePaneTabs()
  if (!drag || !takes(leaf, tabs)(drag.data)) return null
  return <PaneDropZone leaf={leaf} tabs={tabs} move={move} />
}

/**
 * Over the whole pane; the part of it the tab dropped will take is lit, as Zed's are: a side
 * for a pane of its own there, or the whole pane to join its tabs (while panes hold tabs). Under
 * the mouse with the pane's own targets (its strip's tabs and empty end), it gives way to them.
 */
function PaneDropZone({ leaf, tabs, move }: { leaf: PaneLeaf; tabs: boolean; move(move: moves.Move): void }) {
  const t = useTheme()
  const zone = useDropBox(takes(leaf, tabs), (it, at, box) => {
    const { fx, fy } = within(at, box)
    const m = moves.tabDrop(it, leaf, moves.regionAt(fx, fy, tabs), tabs)
    if (m) move(m)
  }, PANE_DROP_PRIORITY)
  const at = zone.isOver && zone.drag ? within(zone.drag, zone.box()) : undefined
  const region = at ? moves.regionAt(at.fx, at.fy, tabs) : undefined
  const dropped = region && zone.drag ? moves.carried(zone.drag.data) : undefined
  // Lit only where the drop does something (not the middle of the tab's own pane).
  const lit = region && dropped && moves.tabDrop(dropped, leaf, region, tabs) ? moves.regionBox(region) : undefined
  return (
    // Painted, nearly clear: an element that paints nothing is not found under the mouse. The lit
    // part is laid out in it, at its start or its end.
    <div
      ref={zone.ref}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        width: "100%",
        height: "100%",
        zIndex: 60,
        display: "flex",
        flexDirection: region === "up" || region === "down" ? "column" : "row",
        justifyContent: region === "right" || region === "down" ? "flex-end" : "flex-start",
        backgroundColor: "#00000001",
      }}
    >
      {lit ? (
        <div
          style={{
            width: pct(lit.w),
            height: pct(lit.h),
            flexShrink: 0,
            backgroundColor: `${t.accent}29`,
            borderWidth: 1,
            borderColor: `${t.accent}80`,
            pointerEvents: "none",
          }}
        />
      ) : null}
    </div>
  )
}

type EdgeEvent = { x?: number; y?: number; pressedButton?: number }

/**
 * A split's edge, over the border between its sides and after the panes, so it is on top of
 * them. A mouse down with a move handler captures the pointer once it moves, so a drag goes on
 * off it.
 */
function Edge({
  seam,
  dragging,
  gap,
  ...handlers
}: {
  seam: Seam
  dragging: boolean
  /** The room between the panes it parts: it is at least that wide, to fill it. */
  gap: number
  onMouseDown(e: EdgeEvent): void
  onMouseMove(e: EdgeEvent): void
  onMouseUp(): void
  onDoubleClick(): void
}) {
  const C = useComponents()
  const { box, ratio } = seam
  const row = seam.node.axis === "row"
  const across = Math.max(7, gap + 2)
  if (ratio < NOTHING || ratio > 1 - NOTHING) return null
  // Placed by its shares, as a pane is: a spacer above it and one before it.
  const top = row ? box.y : box.y + box.h * ratio
  const left = row ? box.x + box.w * ratio : box.x
  return (
    <div style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", display: "flex", flexDirection: "column", pointerEvents: "none" }}>
      <div style={{ height: pct(top), flexShrink: 0 }} />
      <div style={{ display: "flex", flexDirection: "row", height: row ? pct(box.h) : 0, flexShrink: 0 }}>
        <div style={{ width: pct(left), flexShrink: 0 }} />
        <div
          aria-label={row ? "Resize the panes left and right" : "Resize the panes above and below"}
          {...handlers}
          style={{
            flexShrink: 0,
            pointerEvents: "auto",
            display: "flex",
            ...(row
              ? { width: across, marginLeft: -across / 2, height: "100%", cursor: "col-resize" }
              : { width: pct(box.w), height: across, marginTop: -across / 2, cursor: "row-resize" }),
          }}
        >
          <C.Splitter axis={row ? "row" : "column"} dragging={dragging} />
        </div>
      </div>
    </div>
  )
}
