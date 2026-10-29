// The window's state, as hooks: what the server says (sessions, views, tabs, layouts,
// options), the window this tree draws, and the theme's tokens. Every hook reads one store and
// re-renders its component only when what it selects changed.
import { createContext, useContext, useRef, useSyncExternalStore } from "react"
import { store, type PaneLeaf, type Snapshot, type State, type Tree } from "./engine.ts"

export type { PaneLeaf, Snapshot, State, Tree }
export type Session = Snapshot["workspaces"][number]
export type View = Snapshot["views"][number]
export type Tab = Snapshot["tabs"][number]

/** Selects from the app's state; re-renders when the selection changes (shallowly). */
export function useApp<T>(select: (state: State) => T): T {
  const last = useRef<{ value: T } | undefined>(undefined)
  return useSyncExternalStore(store.subscribe, () => {
    const next = select(store.get())
    if (last.current && shallowEqual(last.current.value, next)) return last.current.value
    last.current = { value: next }
    return next
  })
}

function shallowEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true
  if (typeof a !== "object" || typeof b !== "object" || !a || !b) return false
  if (Array.isArray(a) !== Array.isArray(b)) return false
  const ka = Object.keys(a)
  const kb = Object.keys(b)
  if (ka.length !== kb.length) return false
  return ka.every((k) => Object.is((a as any)[k], (b as any)[k]))
}

/** The window a tree draws: its id, the session it shows, and its own controls. */
export interface WindowInfo {
  id: number
  /** The session shown (`$1`), if any. */
  session?: string
  show(session: string): void
  close(): void
  minimize(): void
  zoom(): void
  /** Toggles full screen. */
  fullscreen(): void
  active: boolean
  /** Whether it is full screen now (no traffic lights then). */
  fullscreened: boolean
  /** The traffic lights' box, in window px, while they show. */
  trafficLights?: { x: number; y: number; width: number; height: number }
  /** What shows where the tree paints nothing: `"blurred"` is the system's vibrancy. */
  setBackground(mode: "opaque" | "transparent" | "blurred"): void
  /** How much what shows through a background that is not opaque is blurred, in px; 0 is not at all. */
  setBlur(radius: number): void
  /** How much of what is behind the window every background hides, 0–1 (1 is all of it) while
   *  the background is not opaque: each keeps its own colour, and text and images stay opaque. */
  setGlass(opacity: number): void
  /** The modifiers held down now, what `useHeldModifiers` reads. */
  held?: HeldModifiers
}

/**
 * The modifiers held down in a window, as a chord writes them before its key (`cmd`,
 * `shift+cmd`; `""` for none). The window clears them when it loses focus, which takes their
 * release with it.
 */
export class HeldModifiers {
  private now = ""
  private listeners = new Set<() => void>()

  readonly get = (): string => this.now

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  set(modifiers: { ctrl: boolean; alt: boolean; shift: boolean; cmd: boolean }) {
    // The keymap's order (`normalizeChord`): ctrl, alt, shift, cmd.
    const next = (["ctrl", "alt", "shift", "cmd"] as const).filter((m) => modifiers[m]).join("+")
    if (next === this.now) return
    this.now = next
    for (const listener of this.listeners) listener()
  }

  clear() {
    this.set({ ctrl: false, alt: false, shift: false, cmd: false })
  }
}

const NO_HELD = new HeldModifiers()

/** The modifiers held down in this window (`cmd`, `shift+cmd`), `""` while none are. */
export function useHeldModifiers(): string {
  const held = useContext(WindowContext)?.held ?? NO_HELD
  return useSyncExternalStore(held.subscribe, held.get)
}

export const WindowContext = createContext<WindowInfo | null>(null)

export function useWindow(): WindowInfo {
  const window = useContext(WindowContext)
  if (!window) throw new Error("pumi: useWindow outside a window")
  return window
}

const EMPTY: never[] = []

export function useSessions(): Session[] {
  return useApp((s) => s.snapshot?.workspaces ?? EMPTY)
}

/** A session: `id`, or the one the window shows. */
export function useSession(id?: string): Session | undefined {
  const window = useContext(WindowContext)
  const want = id ?? window?.session
  return useApp((s) => s.snapshot?.workspaces.find((w) => w.workspace_id === want))
}

export function useViews(session?: string): View[] {
  const shown = useSession(session)?.workspace_id
  return useApp((s) => s.snapshot?.views.filter((v) => v.workspace_id === shown) ?? EMPTY)
}

/** A view: `id`, or the one in front of the window's session. */
export function useView(id?: string): View | undefined {
  const session = useSession()
  const want = id ?? session?.active_view_id ?? undefined
  return useApp((s) => s.snapshot?.views.find((v) => v.view_id === want))
}

/** A view's layout: a split's two sides, a leaf's pane and its tabs. */
export function useLayout(view?: string): Tree | undefined {
  const shown = useView(view)?.view_id
  return useApp((s) => (shown ? s.trees[shown] : undefined))
}

export function useTab(id: string | undefined): Tab | undefined {
  return useApp((s) => s.snapshot?.tabs.find((t) => t.tab_id === id))
}

/** The tab in focus in a view (the window's front view by default). */
export function useFocusedTab(view?: string): string | undefined {
  const shown = useView(view)?.view_id
  return useApp((s) => s.snapshot?.layouts.find((l) => l.view_id === shown)?.focused_tab_id ?? undefined)
}

/** The tabs floating over `view` (its own), by zindex: what the theme draws over its layout. */
export function useFloats(view?: string): Tab[] {
  return useApp((s) => {
    const floats: Tab[] = []
    for (const tab of s.snapshot?.tabs ?? []) {
      const place = (tab as any).place
      // `place.view` is the core's number; `view_id` is the `@N` the app goes by.
      if (place?.kind === "float" && (!view || tab.view_id === view)) floats.push(tab)
    }
    return floats.sort((a: any, b: any) => (a.place.zindex ?? 0) - (b.place.zindex ?? 0))
  })
}

/** Whether a view's layout is zoomed to its active pane. */
export function useZoomed(view?: string): boolean {
  const shown = useView(view)?.view_id
  return useApp((s) => s.snapshot?.layouts.find((l) => l.view_id === shown)?.zoomed ?? false)
}

/** A global option's value. */
export function useOption<T = unknown>(name: string): T | undefined {
  return useApp((s) => s.options[name] as T | undefined)
}

/** Whether the server could be reached, and why not. */
export function useConnection(): { connected: boolean; error?: string } {
  return useApp((s) => ({ connected: !!s.snapshot, error: s.error }))
}
