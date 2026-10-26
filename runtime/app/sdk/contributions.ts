// Contributions: what plugins add to the window's slots (`panel`, `tab.toolbar`, `tab.menu`, …) and
// the renderers that draw blocks. An action is data the theme draws as it likes; a component is
// something the theme places, each one alone, under an error boundary of its own.
import { Component, createContext, createElement, isValidElement, memo, Suspense, useContext, useEffect, useMemo, useSyncExternalStore, type ComponentType, type ReactNode } from "react"
import { useOption, useTab } from "../app/state.ts"
import { call, via } from "../app/engine.ts"
import { drawing } from "../native/blame.ts"
import { fit } from "./files.ts"
import { providersVersion, subscribeProviders, useProvided, type Action as ActionItem } from "./actions.ts"

export interface ComponentItem {
  id: string
  plugin: string
  title?: string
  icon?: string
  /** A renderer of the core's `file` block: the files it draws (`*.md`); every file without. */
  match?: string[]
  Component: ComponentType<any>
}

interface Registry {
  components: Map<string, ComponentItem[]>
  actions: Map<string, ActionItem[]>
  /** What can draw each block (`terminal`, `owner.name.board`): every plugin's renderer of it. */
  renderers: Map<string, ComponentItem[]>
}

const registry: Registry = { components: new Map(), actions: new Map(), renderers: new Map() }
const listeners = new Set<() => void>()
let version = 0

function changed() {
  version++
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** Adds a component to `point`; returns what removes it. */
export function addComponent(point: string, item: ComponentItem): () => void {
  const list = registry.components.get(point) ?? []
  registry.components.set(point, [...list.filter((v) => !(v.plugin === item.plugin && v.id === item.id)), item])
  changed()
  return () => {
    registry.components.set(point, (registry.components.get(point) ?? []).filter((v) => v !== item))
    changed()
  }
}

/** Adds an action to `point`; returns what removes it. */
export function addAction(point: string, item: ActionItem): () => void {
  const list = registry.actions.get(point) ?? []
  registry.actions.set(point, [...list.filter((a) => !(a.plugin === item.plugin && a.id === item.id)), item])
  changed()
  return () => {
    registry.actions.set(point, (registry.actions.get(point) ?? []).filter((a) => a !== item))
    changed()
  }
}

/** `item` can draw `block`; returns what removes it. */
export function addRenderer(block: string, item: ComponentItem): () => void {
  const list = registry.renderers.get(block) ?? []
  registry.renderers.set(block, [...list.filter((r) => r.plugin !== item.plugin), item])
  changed()
  return () => {
    registry.renderers.set(block, (registry.renderers.get(block) ?? []).filter((r) => r !== item))
    changed()
  }
}

/** Takes every contribution of `plugin` away (its window side reloads or stops). */
export function removePlugin(plugin: string) {
  for (const [point, list] of registry.components) registry.components.set(point, list.filter((v) => v.plugin !== plugin))
  for (const [point, list] of registry.actions) registry.actions.set(point, list.filter((a) => a.plugin !== plugin))
  for (const [block, list] of registry.renderers) registry.renderers.set(block, list.filter((r) => r.plugin !== plugin))
  changed()
}

const NONE: never[] = []

/**
 * What `slot` holds: the components contributed to it, and its actions — the plugins', then the
 * app's own for what it is given (`useSlot("tab.menu", { tab })`: Split, Zoom, Move To, Close…).
 * Given `args`, every action's `run()` is already for them; `run(other)` runs it for something else.
 */
export function useSlot(slot: string, args?: Record<string, unknown>): { components: ComponentItem[]; actions: ActionItem[] } {
  useSyncExternalStore(subscribe, () => version)
  useSyncExternalStore(subscribeProviders, providersVersion)
  const own = useProvided(slot, args)
  const contributed = registry.actions.get(slot) ?? NONE
  const key = args ? JSON.stringify(args) : ""
  const actions = useMemo(() => {
    if (!own.length && !args) return contributed
    const bound = args ? contributed.map((a) => ({ ...a, group: a.group ?? "plugins", run: (other?: unknown) => a.run(other ?? args) })) : contributed
    return own.length ? [...bound, ...own] : bound
  }, [contributed, own, key])
  return { components: registry.components.get(slot) ?? NONE, actions }
}

/** The tab option that names the plugin (`owner/name`) drawing this one tab's block. */
export const TAB_RENDERER = "@tab_renderer"
/**
 * The tab option a file tab's own methods go to (`<viewer>.<method>`): the plugin drawing it.
 * The app keeps it equal to what it draws the tab with.
 */
export const VIEWER = "@viewer"
/** The global option that names the plugin drawing each core block (`{ terminal: "me/term" }`). */
export const RENDERERS = "renderers"
/** The plugin that draws a core block when nothing else is chosen. */
const BUNDLED: Record<string, string> = { terminal: "pumi/terminal" }

/** The core's block for a file, which the renderer whose `match` fits the file best draws. */
export const FILE = "file"

/** What draws a file tab showing `path`, of `list`: the renderers that fit it, the best first;
 *  among equals the one `renderers` names, then the first. `undefined` when none fits. */
function fileRenderer(list: ComponentItem[], path: string, chosen: unknown): ComponentItem | undefined {
  let best: ComponentItem | undefined
  let most = -1
  for (const r of list) {
    const n = fit(r.match, path)
    if (n === undefined) continue
    if (n > most || (n === most && r.plugin === chosen)) [best, most] = [r, n]
  }
  return best
}

/**
 * What draws `tab`'s block: the plugin its `@tab_renderer` names, else the block's own plugin
 * (`owner.name.id` is `owner/name`'s), else for a file the one whose `match` fits it best, else
 * for a core block the one `renderers` names and then the bundled one; else any plugin that draws
 * it. `undefined` when no enabled plugin does (a file no renderer fits draws the placeholder).
 */
export function useRenderer(tab: string | undefined): ComponentItem | undefined {
  useSyncExternalStore(subscribe, () => version)
  const p = useTab(tab)
  const chosen = useOption<Record<string, string>>(RENDERERS)
  const renderer = p ? rendererFor(p, chosen) : undefined
  // A file tab's own methods (`save`, its viewer's) are the viewer's: say which plugin draws it.
  const viewer = p?.block === FILE ? renderer?.plugin : undefined
  const shown = p?.options?.[VIEWER]
  useEffect(() => {
    if (!tab || !viewer || shown === viewer) return
    call(via(tab, "option.set"), VIEWER, viewer, { scope: "tab", id: tab }).catch(() => {})
  }, [tab, viewer, shown])
  return renderer
}

/** What draws `p`'s block, `chosen` being the `renderers` option (`useRenderer`, outside React). */
export function rendererFor(p: { block: string; options?: Record<string, unknown> | null }, chosen: Record<string, string> | undefined): ComponentItem | undefined {
  const list = registry.renderers.get(p.block) ?? NONE
  if (list.length === 0) return undefined
  const by = (plugin: unknown) => (typeof plugin === "string" && plugin ? list.find((r) => r.plugin === plugin) : undefined)
  // A plugin's prefix is `owner.name.` with `-` as `_`, which a block of its own starts with.
  const own = list.find((r) => p.block.startsWith(`${r.plugin.replace("/", ".").replace(/-/g, "_")}.`))
  if (p.block === FILE) return by(p.options?.[TAB_RENDERER]) ?? by(p.options?.[VIEWER]) ?? fileRenderer(list, String(p.options?.path ?? ""), chosen?.[FILE])
  return by(p.options?.[TAB_RENDERER]) ?? own ?? by(chosen?.[p.block]) ?? by(BUNDLED[p.block]) ?? list[0]
}

/**
 * What could draw `tab`'s block, for choosing another ("Open With"): for a file the renderers
 * whose `match` fits it, the best first; else every renderer of the block. `pinned` is the plugin
 * its `@tab_renderer` names, when one does.
 */
export function useRenderers(tab: string | undefined): { list: ComponentItem[]; pinned?: string } {
  useSyncExternalStore(subscribe, () => version)
  return renderersFor(useTab(tab))
}

/** What could draw `p`'s block (`useRenderers`, outside React). */
export function renderersFor(p: { block: string; options?: Record<string, unknown> | null } | undefined): { list: ComponentItem[]; pinned?: string } {
  const all = (p && registry.renderers.get(p.block)) || NONE
  const pin = p?.options?.[TAB_RENDERER]
  const pinned = typeof pin === "string" && pin ? pin : undefined
  if (p?.block !== FILE) return { list: all, pinned }
  return { list: fitting(all, String(p.options?.path ?? "")), pinned }
}

/** The renderers of `list` whose `match` fits `path`, the best first (a stable sort keeps ties in order). */
export function fitting(list: ComponentItem[], path: string): ComponentItem[] {
  return list
    .map((r) => [r, fit(r.match, path)] as const)
    .filter((e): e is readonly [ComponentItem, number] => e[1] !== undefined)
    .sort((a, b) => b[1] - a[1])
    .map(([r]) => r)
}

/** A contribution that throws draws its error, not a broken window. */
export class Boundary extends Component<{ name: string; children?: ReactNode; fallback?: (error: Error) => ReactNode }, { error?: Error }> {
  override state: { error?: Error } = {}
  static getDerivedStateFromError(error: Error) {
    return { error }
  }
  override componentDidCatch(error: Error) {
    console.error(`pumi: ${this.props.name} threw:`, error)
  }
  override render() {
    if (!this.state.error) return this.props.children
    if (this.props.fallback) return this.props.fallback(this.state.error)
    return createElement(
      "div",
      { style: { padding: 6, borderRadius: 4, backgroundColor: "#cc666622" } },
      createElement("text", { style: { color: "#cc6666", fontSize: 11 } }, `${this.props.name}: ${this.state.error.message}`),
    )
  }
}

/** The plugin a contributed component is of (`usePlugin`). */
export const PluginContext = createContext<string | undefined>(undefined)

/** The plugin whose component this is: its id, `owner/name`. */
export function usePlugin(): string | undefined {
  return useContext(PluginContext)
}

/** Each component's wrapped self (`inCompartment`, `blamed`), by how it was wrapped. */
const wrappedBy = new Map<string, WeakMap<Function, ComponentType<any>>>()

/** `Component` called through `wrap`, once per component and key; a non-function is unchanged. */
function wrapped<P>(Component: ComponentType<P>, key: string, wrap: (draw: (props: P) => ReactNode) => (props: P) => ReactNode): ComponentType<P> {
  if (typeof Component !== "function" || (Component as any).prototype?.isReactComponent) return Component
  let byComponent = wrappedBy.get(key)
  if (!byComponent) wrappedBy.set(key, (byComponent = new WeakMap()))
  let made = byComponent.get(Component)
  if (!made) {
    const Wrapped = wrap(Component as (props: P) => ReactNode) as ComponentType<P> & { displayName?: string }
    Wrapped.displayName = (Component as any).displayName ?? Component.name
    byComponent.set(Component, (made = Wrapped))
  }
  return made
}

/**
 * `Component` in a compartment of its own (natively: what it draws is kept, sized as it would be
 * without one, and costs the window's frames only when it changes; one slow to draw draws on a
 * budget). `name` says whose it is in the performance overlay (`owner/name`, a theme's part), and
 * `blame` what the watchdog leaves out if drawing it freezes the app's JS (`native/blame.ts`).
 *
 * A compartment holds one box, so it is only put around what draws one: the component is called
 * here (its hooks are this wrapper's), and what it returns goes in a compartment when it is one
 * `<div>` or `<text>`. Anything else (a fragment, another component, nothing) is drawn as it is.
 * A component that is not a plain function (a class, `memo`, `lazy`) is given back unchanged.
 */
export function inCompartment<P>(Component: ComponentType<P>, name: string, blame = name): ComponentType<P> {
  return wrapped(Component, `compartment ${name} ${blame}`, (draw) => (props) => {
    const drawn = drawing(blame, () => draw(props))
    return isValidElement(drawn) && (drawn.type === "div" || drawn.type === "text") ? createElement("div", { compartment: name }, drawn) : drawn
  })
}

/** `Component`, drawn with `blame` noted as what is drawing (`native/blame.ts`). */
export function blamed<P>(Component: ComponentType<P>, blame: string): ComponentType<P> {
  return wrapped(Component, `blame ${blame}`, (draw) => (props) => drawing(blame, () => draw(props)))
}

/** A contributed component, drawn again only when what it is given changed (shallowly). */
const Contributed = memo(
  function Contributed({ component, props }: { component: ComponentType<any>; props: Record<string, unknown> }) {
    return createElement(component, props)
  },
  (a, b) => a.component === b.component && shallowEqual(a.props, b.props),
)

function shallowEqual(a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  const keys = Object.keys(a)
  return keys.length === Object.keys(b).length && keys.every((k) => Object.is(a[k], b[k]))
}

/**
 * Draws a contributed component with the props it is given: under its plugin's context, its own
 * error boundary, and a suspense boundary while its `ui.tsx` loads. What it draws is in a
 * compartment of its own (`inCompartment`; `ui.ts` wraps each export it loads), and it draws again
 * when what it is given changes, not each time the theme around it does.
 */
export function renderComponent(item: ComponentItem, props: Record<string, unknown> = {}): ReactNode {
  // A component is given ids as the hooks give them (`%3`): the theme passes the same.
  return createElement(
    PluginContext.Provider,
    { key: `${item.plugin}/${item.id}`, value: item.plugin },
    createElement(
      Boundary,
      { name: item.plugin },
      createElement(Suspense, { fallback: null }, createElement(Contributed, { component: inCompartment(item.Component, item.plugin), props })),
    ),
  )
}
