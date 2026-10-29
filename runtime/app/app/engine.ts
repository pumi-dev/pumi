// The native engine and the server's state, as one store React reads.
import { Engine } from "../native/addon.js"
import { safe } from "./mode.ts"
import { countCall, countEvent } from "./diagnostics.ts"

/** A pane of a view's layout: its id (`:4`), its tabs in its strip's order, and the one in front. */
export type PaneLeaf = { pane: string; tabs: string[]; active?: string | null }
/** A view's layout: a pane, an app, or a split of two. */
export type Tree = PaneLeaf | { app: number } | { axis: "row" | "column"; ratio: number; first: Tree; second: Tree }

export interface Snapshot {
  focused_workspace_id?: string | null
  focused_view_id?: string | null
  focused_tab_id?: string | null
  workspaces: { workspace_id: string; label: string; active_view_id?: string | null; status: string; cwd?: string }[]
  views: { view_id: string; workspace_id: string; number: number; label: string | null; status: string; worktree?: string }[]
  tabs: { tab_id: string; block: string; traits?: string[]; view_id: string; workspace_id: string; status: string; cwd?: string | null; terminal_title?: string | null; label?: string; agent?: string; agent_session?: string; agent_model?: string; agent_transcript?: string; agent_conversation?: string; place?: unknown; options?: Record<string, unknown> }[]
  layouts: { view_id: string; zoomed: boolean; focused_tab_id?: string | null }[]
}

export interface State {
  snapshot?: Snapshot
  trees: Record<string, Tree>
  options: Record<string, unknown>
  /** The server's menus (`registry.list("menu")`), by point. */
  menus?: unknown[]
  error?: string
  /** How bringing the server up goes, while it does (`setup` events). */
  setup?: Setup
}

export interface Setup {
  title: string
  detail: string
  gaveUp: boolean
  /** The last lines its steps printed. */
  lines: string[]
}

type Listener = () => void
type Handler = (data: any) => void

let state: State = { trees: {}, options: {} }
const listeners = new Set<Listener>()
const handlers = new Map<string, Set<Handler>>()

/**
 * `next`, keeping each part of `prev` that is equal to it: what didn't change keeps its identity,
 * so a hook reading one tab re-renders when that tab changes, not when any does.
 */
export function share<T>(prev: unknown, next: T): T {
  if (Object.is(prev, next) || typeof prev !== "object" || typeof next !== "object" || !prev || !next) return next
  if (Array.isArray(prev) !== Array.isArray(next)) return next
  if (Array.isArray(next)) {
    const old = prev as unknown[]
    let same = old.length === next.length
    const out = next.map((item, i) => {
      const kept = share(old[i], item)
      if (kept !== old[i]) same = false
      return kept
    })
    return (same ? old : out) as T
  }
  const old = prev as Record<string, unknown>
  const keys = Object.keys(next)
  let same = keys.length === Object.keys(old).length
  const out: Record<string, unknown> = {}
  for (const key of keys) {
    const kept = share(old[key], (next as Record<string, unknown>)[key])
    if (kept !== old[key]) same = false
    out[key] = kept
  }
  return (same ? old : out) as T
}

const trace = process.env.PUMI_TRACE ? { events: 0, ms: 0, since: performance.now() } : undefined

function set(next: State) {
  const started = trace ? performance.now() : 0
  next = share(state, next)
  if (next === state) return
  state = next
  for (const listener of listeners) listener()
  if (trace) {
    trace.events++
    trace.ms += performance.now() - started
    if (performance.now() - trace.since > 10_000) {
      console.error(`pumi-trace state: ${trace.events} changes in 10 s, ${(trace.ms / Math.max(1, trace.events)).toFixed(2)} ms each`)
      Object.assign(trace, { events: 0, ms: 0, since: performance.now() })
    }
  }
}

export const engine = Engine.start({ dataDir: process.env.PUMI_DATA_DIR, safe }, (event: { kind: string; data: any }) => {
  const started = performance.now()
  try {
    receive(event)
  } finally {
    countEvent(event.kind, performance.now() - started)
  }
})

function receive(event: { kind: string; data: any }) {
  if (event.kind === "state") {
    const data = event.data
    if (data.error) return set({ ...state, error: data.error })
    const trees = { ...state.trees, ...data.trees }
    for (const view of Object.keys(trees)) if (!data.snapshot.layouts.some((l: any) => l.view_id === view)) delete trees[view]
    set({
      snapshot: data.snapshot,
      trees,
      options: data.options ?? state.options,
      menus: data.menus ?? state.menus,
      setup: undefined,
    })
  }
  if (event.kind === "setup") {
    const data = event.data
    const was = state.setup ?? { title: "", detail: "", gaveUp: false, lines: [] }
    if (data.ready) set({ ...state, setup: undefined })
    else if (data.line !== undefined) set({ ...state, setup: { ...was, lines: [...was.lines, String(data.line)].slice(-12) } })
    else set({ ...state, setup: { ...was, title: data.title, detail: data.detail, gaveUp: data.gaveUp } })
  }
  for (const handler of handlers.get(event.kind) ?? []) handler(event.data)
}

export const store = {
  get: () => state,
  subscribe(listener: Listener) {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  },
}

/** Hears the engine's `kind` events (`notice`, `request`, `terminal`, `remote`, `notification`). */
export function on(kind: string, handler: Handler): () => void {
  const set = handlers.get(kind) ?? new Set()
  set.add(handler)
  handlers.set(kind, set)
  return () => {
    set.delete(handler)
  }
}

/** Starts the app again: in safe mode (`--safe`: no plugin runs, the default theme) or out of it. */
export function restartApp({ safe }: { safe: boolean }): never {
  const argv = process.argv.slice(1).filter((a) => a !== "--safe" && a !== "--safe-mode")
  if (safe) argv.push("--safe")
  Bun.spawn([process.execPath, ...argv], { env: { ...process.env, PUMI_SAFE_MODE: "" }, stdio: ["ignore", "inherit", "inherit"] }).unref()
  process.exit(0)
}

export function call(method: string, ...params: unknown[]): Promise<any> {
  const started = performance.now()
  const result = engine.call(method, params)
  const done = () => countCall(method, performance.now() - started)
  result?.then?.(done, done)
  return result
}

export { idOn, num, serverOf, via } from "./ids.ts"
