// A theme's docks, drawn: `<Dock name edge>` is a side bar, a second one, a panel at the foot —
// whatever the theme declared under `docks` — shown while it is open, as big as the user dragged
// it, with the edge that drags it on its inner side. `useDock` is one dock's state (open, size,
// what toggles it) and `useDockPanels` the plugins' `panel` components it holds and which is in front.
// What it shows, and how it looks, is the theme's: the dock is only the box.
import { useContext, useMemo, useState, type ReactNode } from "react"
import { useLocal } from "../app/local.ts"
import { assign } from "../app/dock-panels.ts"
import { DOCK_SIZE, dockCommand, dockKey, dockTitle } from "../app/docks.ts"
import type { StyleDesc } from "../native/host.ts"
import { useSlot, type ComponentItem } from "./contributions.ts"
import { ThemeContext, type DockSpec } from "./theme.ts"

const NO_DOCKS: Record<string, DockSpec> = {}

function useDocks(): Record<string, DockSpec> {
  return useContext(ThemeContext)?.spec.docks ?? NO_DOCKS
}

/** One dock, as `useDock(name)` gives it. */
export interface DockState {
  name: string
  /** Its title (the theme's, or its name's words). */
  title: string
  /** The command that shows and hides it: `toggle_<name>`, for `run` and `shortcut`. */
  command: string
  open: boolean
  setOpen(open: boolean | ((open: boolean) => boolean)): void
  toggle(): void
  /** How big it is across, in px: a side's width, a top or foot's height. */
  size: number
  setSize(size: number): void
  /** Back to the theme's size. */
  resetSize(): void
  min: number
  max: number
}

/** A dock of the theme's (`docks.<name>`), or `sidebar`, the window's own side bar. */
export function useDock(name: string): DockState {
  const spec = useDocks()[name]
  const initial = spec?.size ?? DOCK_SIZE.size
  const min = spec?.min ?? DOCK_SIZE.min
  const max = spec?.max ?? DOCK_SIZE.max
  const [open, setOpen] = useLocal(dockKey(name, "open"), spec?.open ?? true)
  const [size, setSize] = useLocal(dockKey(name, "size"), initial)
  return useMemo(
    () => ({
      name,
      title: dockTitle(name, spec),
      command: dockCommand(name),
      open,
      setOpen,
      toggle: () => setOpen((now) => !now),
      size: Math.min(max, Math.max(min, size)),
      setSize: (next: number) => setSize(Math.round(Math.min(max, Math.max(min, next)))),
      resetSize: () => setSize(initial),
      min,
      max,
    }),
    [name, spec, open, setOpen, size, setSize, initial, min, max],
  )
}

/** A component's key, as docks' `panels` globs match it: `owner/name/component`. */
export const componentKey = (c: Pick<ComponentItem, "plugin" | "id">) => `${c.plugin}/${c.id}`

/** The plugins' `panel` components a dock holds, and the one in front. */
export interface DockPanels {
  components: ComponentItem[]
  /** The component in front, when one is: the theme draws its own (a list of them, say) when none is. */
  shown?: ComponentItem
  /** Puts a component in front (by it, or its key) and opens the dock. */
  show(component: ComponentItem | string): void
  /** The component in front, or the dock hidden when it is already in front: a dock's buttons. */
  toggle(component: ComponentItem | string): void
  /** No component in front. */
  hide(): void
}

/**
 * The `panel` components `docks.<name>.panels` gives this dock (a component goes to the first dock
 * whose globs name it; `*` takes the rest), in the order the plugins came, and which is in front.
 */
export function useDockPanels(name: string): DockPanels {
  const docks = useDocks()
  const { components } = useSlot("panel")
  const dock = useDock(name)
  const [front, setFront] = useLocal<string | undefined>(dockKey(name, "component"), undefined)
  const keys = components.map(componentKey)
  const held = useMemo(() => new Set(assign(Object.entries(docks).map(([n, s]) => [n, s.panels]), keys).get(name) ?? []), [docks, name, keys.join("\n")])
  const mine = useMemo(() => components.filter((c) => held.has(componentKey(c))), [components, held])
  const shown = mine.find((c) => componentKey(c) === front)
  const key = (c: ComponentItem | string) => (typeof c === "string" ? c : componentKey(c))
  return {
    components: mine,
    shown,
    show(c) {
      setFront(key(c))
      dock.setOpen(true)
    },
    toggle(c) {
      if (dock.open && shown && componentKey(shown) === key(c)) dock.setOpen(false)
      else {
        setFront(key(c))
        dock.setOpen(true)
      }
    },
    hide: () => setFront(undefined),
  }
}

/** Which side of what it is in a dock sits on: its drag edge is on the other. */
export type DockEdge = "left" | "right" | "top" | "bottom"

export interface DockProps {
  /** The dock: a name under the theme's `docks`, or `sidebar`. */
  name: string
  /** Where it sits; `left` by default. A left or right dock's size is its width, a top or foot's its height. */
  edge?: DockEdge
  /** Its box's look (a background, a border on its inner side); it is a flex column. */
  style?: StyleDesc
  /** The drag edge's look, over the dock's inner side (6 px across); a faint accent on hover by default. */
  handle?: StyleDesc
  children?: ReactNode
}

/**
 * A dock: nothing while it is hidden, else a box as big as the user made it with what the theme
 * puts in it. Its inner edge drags it bigger or smaller; a double press there gives it back the
 * theme's size.
 */
export function Dock({ name, edge = "left", style, handle, children }: DockProps) {
  const dock = useDock(name)
  const accent = useContext(ThemeContext)?.tokens.accent ?? "#3474f0"
  const [from, setFrom] = useState<{ at: number; size: number } | undefined>()
  if (!dock.open) return null
  const across = edge === "left" || edge === "right"
  // Dragging away from the side it sits on makes it bigger.
  const sign = edge === "left" || edge === "top" ? 1 : -1
  const at = (e: { x?: number; y?: number }) => (across ? e.x : e.y) ?? 0
  const place: StyleDesc =
    edge === "left"
      ? { top: 0, right: -3, width: 6, height: "100%" }
      : edge === "right"
        ? { top: 0, left: -3, width: 6, height: "100%" }
        : edge === "top"
          ? { left: 0, bottom: -3, height: 6, width: "100%" }
          : { left: 0, top: -3, height: 6, width: "100%" }
  return (
    <div cache style={{ display: "flex", flexDirection: "column", flexShrink: 0, position: "relative", minWidth: 0, minHeight: 0, ...(across ? { width: dock.size } : { height: dock.size }), ...style }}>
      {children}
      <div
        aria-label={`Resize ${dock.title}`}
        onMouseDown={(e: any) => setFrom({ at: at(e), size: dock.size })}
        onMouseMove={(e: any) => from && dock.setSize(from.size + sign * (at(e) - from.at))}
        onMouseUp={() => setFrom(undefined)}
        onDoubleClick={dock.resetSize}
        style={{
          position: "absolute",
          ...place,
          cursor: across ? "col-resize" : "row-resize",
          backgroundColor: from ? `${accent}55` : undefined,
          hover: { backgroundColor: `${accent}40` },
          ...handle,
        }}
      />
    </div>
  )
}
