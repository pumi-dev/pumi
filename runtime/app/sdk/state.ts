// The window's state as a plugin's `ui.tsx` reads it: the same models `main.ts` has, over one live
// cache. As in a worker, the cache fills a model with `<kind>.get` the first time it is read and
// keeps it current from the models' public events (`*.opened`, `*.closed`, `*.changed`).
//
// A hook re-renders its component when the cache changes; `useWatchable` only when the keys it
// names do.
import { useCallback, useContext, useEffect, useReducer, useState, useSyncExternalStore } from "react"
import { call as engineCall, store } from "../app/engine.ts"
import { useApp, WindowContext } from "../app/state.ts"
import { listen, subscribeEvent, unlisten, type WindowEvent } from "./events.ts"
import { cache, type Model } from "../worker/sdk/model.ts"
import { Request, Session, Tab, View } from "../worker/sdk/models.gen.ts"
import type { Wire } from "../worker/sdk/transport.ts"
import { connect, wireFill, workspace, type RequestFilter, type TabFilter } from "../worker/sdk/workspace.ts"

/** The engine, as this window's SDK wire: its calls go to this server or another by their name. */
const engine: Wire = {
  call: (name, args) => engineCall(name, ...args),
  on: (events, opts, fn) => listen(events, opts, (e) => fn(e)),
  off: (handle) => unlisten(handle),
}

/** The wire this window's SDK speaks. */
export const wire: Wire = engine

/** The caller's own objects: the window's focus, where it is. */
function caller() {
  const snapshot = store.get().snapshot
  return {
    session: snapshot?.focused_workspace_id ?? null,
    view: snapshot?.focused_view_id ?? null,
    tab: snapshot?.focused_tab_id ?? null,
  }
}

connect(wire, wireFill(wire), caller)

/** The window's focus, as one string: a hook re-renders when it moves. */
function focus(): string {
  const own = caller()
  return `${own.session}|${own.view}|${own.tab}`
}

function subscribeCache(fn: () => void): () => void {
  const offs = [cache.subscribe(fn), store.subscribe(fn)]
  return () => offs.forEach((off) => off())
}

/** Selects from the cache (filling it first); re-renders on a change to it or to the focus. */
function useCached<T>(select: () => T): T {
  useSyncExternalStore(subscribeCache, () => `${cache.version}/${focus()}`)
  workspace.ready.catch(() => {})
  return select()
}

const EMPTY: never[] = []

/** A call, as `main.ts`'s `call` is: the export's function names, prefixed ids. */
export function call(name: string, ...args: unknown[]): Promise<any> {
  return wire.call(name, args)
}

/** What a block's renderer is given: the tab, its block, and whether the tab has the focus. */
export interface RendererProps {
  tab: Tab
  block: string
  focused: boolean
}

/** The window a tree draws: its id, the session it shows, and its own controls. */
export interface WindowInfo {
  id: number
  /** The session it shows. */
  session: Session | null
  show(session: string | null): void
  close(): void
  minimize(): void
  zoom(): void
  /** Toggles full screen. */
  fullscreen(): void
  active: boolean
  /** Whether it is full screen now (no traffic lights then). */
  fullscreened: boolean
  /** What shows where the tree paints nothing: `"blurred"` is the system's vibrancy. */
  setBackground(mode: "opaque" | "transparent" | "blurred"): void
  /** How much what shows through a background that is not opaque is blurred, in px; 0 is not at all. */
  setBlur(radius: number): void
}

/** The window this tree draws, and its controls. */
export function useWindow(): WindowInfo {
  const window = useContext(WindowContext)
  if (!window) throw new Error("pumi: useWindow outside a window")
  return {
    id: window.id,
    session: cache.model<Session>("session", window.session),
    show: (session) => window.show(session ?? ""),
    close: window.close,
    minimize: window.minimize,
    zoom: window.zoom,
    fullscreen: window.fullscreen,
    active: window.active,
    fullscreened: window.fullscreened,
    setBackground: window.setBackground,
    setBlur: window.setBlur,
  }
}

/** Every session, in creation order. */
export function useSessions(): Session[] {
  return useCached(() => workspace.sessions)
}

/** A session: `id`, or the caller's own. */
export function useSession(id?: string): Session | undefined {
  const window = useContext(WindowContext)
  return useCached(() => workspace.getSession(id ?? window?.session ?? caller().session ?? "") ?? undefined)
}

/** A session's views, in order (the caller's session without one). */
export function useViews(session?: string): View[] {
  const shown = useSession(session)
  return useCached(() => shown?.views ?? EMPTY)
}

/** A view: `id`, or the one in front of the caller's session. */
export function useView(id?: string): View | undefined {
  const shown = useSession()
  return useCached(() => workspace.getView(id ?? caller().view ?? shown?.activeView?.id ?? "") ?? undefined)
}

/** A tab: the model its id names, or the caller's own. */
export function useTab(id?: string | null): Tab | undefined {
  return useCached(() => workspace.getTab(id ?? caller().tab ?? "") ?? undefined)
}

/** The keys of a `tab.changed` that may move a tab in or out of a filter. */
const FILTERED = (key: string) => key === "place" || key === "block" || key.startsWith("traits")

/** The tabs matching `filter`: of a session or view, holding a block, with a trait. */
export function useTabs(filter: TabFilter = {}): Tab[] {
  const at = JSON.stringify(filter)
  const [ids, setIds] = useState<string[]>(EMPTY)
  useEffect(() => {
    const want = JSON.parse(at) as TabFilter
    let live = true
    const load = () => {
      wire.call("tab.list", [want]).then(
        (found: string[]) => live && setIds(found ?? EMPTY),
        () => {},
      )
    }
    const moved = (e: WindowEvent<{ keys?: string[] }>) => {
      if ((e.data?.keys ?? []).some(FILTERED)) load()
    }
    load()
    const offs = [subscribeEvent("tab.opened", undefined, load), subscribeEvent("tab.closed", undefined, load), subscribeEvent("tab.changed", undefined, moved)]
    return () => {
      live = false
      offs.forEach((off) => off())
    }
  }, [at])
  return useCached(() => cache.models<Tab>("tab", ids))
}

/** Re-renders the component that reads `model` when one of `keys` of it changes (any without). */
export function useWatchable<M extends Model | null>(model: M, keys?: string[]): M {
  const [, bump] = useReducer((n: number) => n + 1, 0)
  const at = keys === undefined ? undefined : JSON.stringify(keys)
  useEffect(() => {
    if (!model) return
    const kind = (model.constructor as typeof Model).kind
    const off = cache.watch(kind, model.id, at === undefined ? undefined : JSON.parse(at), bump)
    return () => off()
  }, [model, at])
  // Reading it fills it.
  void model?.data
  return model
}

/** The app's own state, for a theme: the theme kit's hook. */
export { useApp }

/** Today's engine `call` with its own arguments, for a caller that has a raw id. */
export { engineCall as callRaw }

/** Which object an option is on: the global one by default, or a scope and its id. */
export interface OptionScope {
  scope?: "global" | "session" | "view" | "tab" | "plugin"
  /** The object (or plugin, `owner/name`) the scope is about. */
  id?: string
}

/** Sets the option `name` in `scope`; `null` forgets it. */
export type OptionSetter<T = unknown> = (value: T | null) => Promise<null>

/** The event that says an option in `where` changed, and its match: an object's own `changed`. */
function optionEvent(name: string, where: OptionScope): { event: string; match?: string; hears: (e: WindowEvent) => boolean } {
  const scope = where.scope ?? "global"
  if (scope === "global" || scope === "plugin") return { event: "option.changed", match: name, hears: () => true }
  return {
    event: `${scope}.changed`,
    match: where.id,
    hears: (e) => ((e.data?.keys ?? []) as string[]).includes(`options.${name}`),
  }
}

/** `where` as `option.get`/`set` take it: a plugin's store is named by `plugin`, an object by `id`. */
const wired = (where: OptionScope) => (where.scope === "plugin" ? { scope: "plugin", plugin: where.id } : where)

/**
 * An option's value and its setter, as Airtable's `useSynced` is: read with `option.get`, and
 * again when it changes (`option.changed`, or its object's `changed` for an object's option).
 */
export function useOption<T = unknown>(name: string, opts: OptionScope = {}): [T | undefined, OptionSetter<T>] {
  const scope = opts.scope ?? "global"
  const where: OptionScope = scope === "global" && !opts.id ? {} : { scope, id: opts.id }
  const at = JSON.stringify([name, where])
  const [value, setValue] = useState<T | undefined>(undefined)
  useEffect(() => {
    const [option, object] = JSON.parse(at) as [string, OptionScope]
    let live = true
    const load = () => {
      wire.call("option.get", [option, wired(object)]).then(
        (got) => live && setValue((got ?? undefined) as T | undefined),
        () => {},
      )
    }
    load()
    const { event, match, hears } = optionEvent(option, object)
    const off = subscribeEvent(event, match, (e) => hears(e) && load())
    return () => {
      live = false
      off()
    }
  }, [at])
  const set = useCallback<OptionSetter<T>>((value) => {
    const [option, object] = JSON.parse(at) as [string, OptionScope]
    return wire.call("option.set", [option, value ?? null, wired(object)])
  }, [at])
  return [value, set]
}

/** The requests open now, and the ones that open or close while the component is mounted. */
export function useRequests(filter: RequestFilter = {}): Request[] {
  const at = JSON.stringify(filter)
  const [ids, setIds] = useState<string[]>([])
  useEffect(() => {
    const want = JSON.parse(at) as RequestFilter
    let live = true
    const load = () => {
      wire.call("request.list", [want]).then(
        (found: string[]) => live && setIds(found ?? []),
        () => {},
      )
    }
    load()
    const offs = [subscribeEvent("request.opened", undefined, load), subscribeEvent("request.closed", undefined, load)]
    return () => {
      live = false
      offs.forEach((off) => off())
    }
  }, [at])
  return cache.models<Request>("request", ids)
}

/** A registry's entries (`fn`, `block`, `option`, `plugin`, …), as `registry.list` has them. */
export function useRegistry<T = unknown>(kind: string, opts: Record<string, unknown> = {}): T[] {
  const at = JSON.stringify([kind, opts])
  const [entries, setEntries] = useState<T[]>([])
  useEffect(() => {
    const [which, what] = JSON.parse(at) as [string, Record<string, unknown>]
    let live = true
    const load = () => {
      wire.call("registry.list", [which, what]).then(
        (found: T[]) => live && setEntries(found ?? []),
        () => {},
      )
    }
    load()
    const off = subscribeEvent("registry.changed", undefined, load)
    return () => {
      live = false
      off()
    }
  }, [at])
  return entries
}

/** The root object: the caller's own objects and the calls that make more of them. */
export { workspace }
