// The commands: the app's own, the plugins' (their manifests), and what the window's components
// register while mounted (a theme's `toggle_sidebar`). One registry feeds the keymap and the menus,
// and anything that lists them; `run` does one.
import { useEffect } from "react"
import { Engine } from "../native/addon.js"
import { call, engine, idOn, serverOf, store, via } from "./engine.ts"
import { chordOf, humanize, Keymap, type Command } from "./keymap.ts"
import * as plugins from "./plugins.ts"
import * as serverKeys from "./serverkeys.ts"
import * as layout from "./layout.ts"
import * as worktreeViews from "./worktree-views.ts"
import { setChordFor } from "../sdk/actions.ts"
import { openSettings } from "./settings/open.ts"
import type { AppWindow } from "./windows.tsx"
import { setCapturedKeys } from "../native/addon.js"
import { FONT_DEFAULT, FONT_SIZE_RANGE, guifont, parseFont } from "../sdk/font.ts"
import { explainNoNewTab } from "./new-tab.ts"
import { openView } from "./new-view.ts"

/** Where a command runs: the window it was asked in, and what of it is in front. */
export interface Context {
  window: AppWindow
  session?: string
  view?: string
  tab?: string
  /** What a contribution's action passes (a tab row's tab, a file). */
  args?: unknown
}

export interface CommandDef extends Command {
  run(context: Context, index?: number): unknown
}

const registry = new Map<string, CommandDef>()
const listeners = new Set<() => void>()
let keymap = new Keymap({ commands: [], defaults: [] })

/** macOS: what the Mac's own menus write, and the keys macOS has always given an app. */
const DEFAULTS: [string, string][] = [
  ["new_view", "cmd+t"],
  ["new_workspace", "cmd+n"],
  ["new_window", "cmd+shift+n"],
  ["close_window", "cmd+alt+w"],
  ["minimize_window", "cmd+m"],
  ["close_tab", "cmd+w"],
  ["close_view", "cmd+shift+w"],
  ["reopen_closed", "cmd+shift+t"],
  ["split_vertical", "cmd+d"],
  ["split_horizontal", "cmd+shift+d"],
  ["zoom", "cmd+enter"],
  ["zoom", "shift+escape"],
  ["focus_pane_left", "cmd+alt+left"],
  ["focus_pane_right", "cmd+alt+right"],
  ["focus_pane_up", "cmd+alt+up"],
  ["focus_pane_down", "cmd+alt+down"],
  ["resize_pane_left", "cmd+alt+shift+left"],
  ["resize_pane_right", "cmd+alt+shift+right"],
  ["resize_pane_up", "cmd+alt+shift+up"],
  ["resize_pane_down", "cmd+alt+shift+down"],
  ["previous_view", "cmd+["],
  ["next_view", "cmd+]"],
  ["previous_workspace", "cmd+shift+["],
  ["next_workspace", "cmd+shift+]"],
  ["switch_view", "cmd+1..9"],
  ["clear_tab", "cmd+k"],
  ["settings", "cmd+,"],
  ["help", "cmd+/"],
  ["fullscreen", "cmd+ctrl+f"],
  ["font_bigger", "cmd+equal"],
  ["font_bigger", "cmd+plus"],
  ["font_smaller", "cmd+minus"],
  ["font_reset", "cmd+0"],
  ["quit", "cmd+q"],
  ["reload_config", "cmd+shift+r"],
  ["toggle_sidebar", "cmd+b"],
  ["equalize_panes", "cmd+alt+equal"],
  ["toggle_secondary_sidebar", "cmd+alt+b"],
  ["toggle_panel", "cmd+j"],
]

/**
 * Every OS but macOS, as Windows Terminal has its keys: a bare `ctrl+letter` stays the terminal's
 * (it is Ctrl+C), so the app's own take `ctrl+shift`, `ctrl+alt`, `alt+shift` or `alt`.
 */
const DEFAULTS_OTHER: [string, string][] = [
  ["new_view", "ctrl+shift+t"],
  ["new_workspace", "ctrl+shift+n"],
  ["new_window", "ctrl+shift+alt+n"],
  ["close_window", "alt+f4"],
  ["close_tab", "ctrl+shift+w"],
  ["close_view", "ctrl+shift+alt+w"],
  ["reopen_closed", "ctrl+shift+alt+t"],
  ["split_vertical", "alt+shift+plus"],
  ["split_vertical", "alt+shift+equal"],
  ["split_horizontal", "alt+shift+minus"],
  ["zoom", "ctrl+shift+enter"],
  ["focus_pane_left", "alt+left"],
  ["focus_pane_right", "alt+right"],
  ["focus_pane_up", "alt+up"],
  ["focus_pane_down", "alt+down"],
  ["resize_pane_left", "alt+shift+left"],
  ["resize_pane_right", "alt+shift+right"],
  ["resize_pane_up", "alt+shift+up"],
  ["resize_pane_down", "alt+shift+down"],
  ["previous_view", "ctrl+shift+tab"],
  ["next_view", "ctrl+tab"],
  ["previous_workspace", "ctrl+shift+pageup"],
  ["next_workspace", "ctrl+shift+pagedown"],
  ["switch_view", "ctrl+alt+1..9"],
  ["clear_tab", "ctrl+shift+k"],
  ["settings", "ctrl+,"],
  ["help", "ctrl+shift+/"],
  ["fullscreen", "f11"],
  ["font_bigger", "ctrl+equal"],
  ["font_bigger", "ctrl+plus"],
  ["font_smaller", "ctrl+minus"],
  ["font_reset", "ctrl+0"],
  ["quit", "ctrl+shift+q"],
  ["reload_config", "ctrl+shift+r"],
  ["toggle_sidebar", "ctrl+shift+b"],
  ["equalize_panes", "ctrl+shift+alt+equal"],
  ["toggle_secondary_sidebar", "ctrl+alt+b"],
  ["toggle_panel", "ctrl+shift+j"],
]

// A new tab nothing can open (`new_tab` names no block) says so in the window; the rest go to the log.
const swallow = (err: unknown) => {
  if (!explainNoNewTab(err)) console.error(String(err))
}

/** The views of the session `c` shows, by their number, as the view bar has them. */
function viewsOf(c: Context): { view_id: string; number: number }[] {
  const snapshot = store.get().snapshot
  if (!snapshot || !c.session) return []
  return snapshot.views.filter((v) => v.workspace_id === c.session).sort((a, b) => a.number - b.number)
}

/** Shows the view `by` views on from the window's own, wrapping (a negative goes back). */
function stepView(c: Context, by: number) {
  const views = viewsOf(c)
  if (!views.length) return
  const at = Math.max(views.findIndex((v) => v.view_id === c.view), 0)
  c.window.showView(views[(((at + by) % views.length) + views.length) % views.length]!.view_id)
}

/** Where the tab in front of `context` is, or the view's. */
function tab(c: Context): string | undefined {
  return c.tab
}

const split = (direction: "right" | "down") => (c: Context) =>
  call(via(c.tab ?? c.session, "tab.open"), null, {}, { kind: "tiled", split: direction, ...(c.tab ? { beside: c.tab } : {}) })

const focus = (direction: "left" | "right" | "up" | "down") => (c: Context) =>
  call(via(c.tab ?? c.session, "pane.focus"), null, { direction })

/** The pane holding the tab in front of `context`, as the store's tree has it. */
function paneOf(c: Context): string | undefined {
  return c.view ? layout.paneOfTab(store.get().trees[c.view], c.tab)?.pane : undefined
}

const resize = (direction: "left" | "right" | "up" | "down") => (c: Context) =>
  c.tab ? call(via(c.tab, "tab.update"), c.tab, { size: { direction, amount: 3 } }) : undefined

/** Another session of the window's, by `offset`, wrapping. */
function sessionAt(c: Context, offset: number): string | undefined {
  const sessions = store.get().snapshot?.workspaces ?? []
  const at = sessions.findIndex((w) => w.workspace_id === c.session)
  if (!sessions.length) return undefined
  return sessions[(at + offset + sessions.length) % sessions.length]!.workspace_id
}

/**
 * A session made with a terminal in it, shown in `window`: `tab.open` answers the tab, and its
 * `session` is the one just made. `server` is where it is made, when that is not this one.
 */
export async function openSession(window: { show(session: string): void }, place: { name?: string; cwd?: string } = {}, server?: string): Promise<string | undefined> {
  const tab = idOn(server, await call(methodOn(server, "tab.open"), null, {}, { kind: "session", ...place }))
  const made = await call(methodOn(server, "tab.get"), tab)
  const session: string | undefined = typeof made?.session === "string" ? made.session : undefined
  if (session) window.show(session)
  return session
}

/** `method` on `server`, or on the one this app talks to (`via`, for an id that says which). */
function methodOn(server: string | undefined, method: string): string {
  return server ? `${server}/${method}` : method
}

function fontSize(change: (size: number) => number) {
  const { options } = store.get()
  const { family, size } = parseFont(options.guifont, options.linespace)
  const next = Math.min(FONT_SIZE_RANGE[1], Math.max(FONT_SIZE_RANGE[0], change(size)))
  return call("option.set", "guifont", guifont(family, next), {})
}

const APP: [string, string, (c: Context, index?: number) => unknown][] = [
  ["new_window", "New window", (c) => openView(c.session).then(({ session, view }) => (session ? c.window.manager.open(session, view) : c.window.manager.open()))],
  ["close_window", "Close window", (c) => c.window.close()],
  ["minimize_window", "Minimize window", (c) => c.window.renderer.minimizeWindow()],
  ["zoom_window", "Zoom window", (c) => c.window.renderer.zoomWindow()],
  ["fullscreen", "Toggle full screen", (c) => c.window.renderer.toggleFullscreen()],
  ["new_workspace", "New session", (c) => openSession(c.window, {}, serverOf(c.session))],
  ["close_workspace", "Close session", (c) => (c.session ? call(via(c.session, "session.close"), c.session) : undefined)],
  ["previous_workspace", "Previous session", (c) => { const s = sessionAt(c, -1); if (s) c.window.show(s) }],
  ["next_workspace", "Next session", (c) => { const s = sessionAt(c, 1); if (s) c.window.show(s) }],
  ["switch_workspace", "Switch to session 1–9", (c, i) => { const s = store.get().snapshot?.workspaces[(i ?? 1) - 1]; if (s) c.window.show(s.workspace_id) }],
  ["new_view", "New view", (c) => openView(c.session).then(({ session, view }) => session && view && c.window.show(session, view))],
  ["new_worktree", "New worktree", (c) => (c.session && worktreeViews.canHave(c.session) ? worktreeViews.createAsking(c.window, c.session) : undefined)],
  ["close_view", "Close view", (c) => (c.view ? call(via(c.view, "view.close"), c.view) : undefined)],
  ["reopen_closed", "Reopen closed tab", async () => {
    const [last] = (await call("tab.list", { closed: true })) ?? []
    // Into its pane while that is there, else beside the tab its pane was beside.
    if (last) return call("tab.open", last.state?.block ?? null, { state: last.state }, { kind: "tiled", ...(last.pane ? { pane: last.pane } : {}), ...(last.view ? { view: last.view } : {}), ...(last.beside ? { beside: last.beside } : {}) })
  }],
  ["previous_view", "Previous view", (c) => stepView(c, -1)],
  ["next_view", "Next view", (c) => stepView(c, 1)],
  ["switch_view", "Switch to view 1–9", (c, i) => {
    const go = numbered.get(c.window.id)
    if (go) return go(i ?? 1)
    const view = viewsOf(c).find((v) => v.number === (i ?? 1))
    if (view) c.window.showView(view.view_id)
  }],
  ["split_vertical", "Split right", split("right")],
  ["split_horizontal", "Split down", split("down")],
  ["new_tab", "New tab in pane", (c) => call(via(c.tab ?? c.session, "tab.open"), null, {}, { kind: "tiled", pane: paneOf(c) ?? "current" })],
  ["next_tab", "Next tab in pane", (c) => call(via(c.tab ?? c.session, "tab.focus"), c.tab ?? null, { offset: 1 })],
  ["previous_tab", "Previous tab in pane", (c) => call(via(c.tab ?? c.session, "tab.focus"), c.tab ?? null, { offset: -1 })],
  ["close_tab", "Close tab", (c) => (tab(c) !== undefined ? call(via(c.tab, "tab.close"), tab(c)) : undefined)],
  ["close_pane", "Close pane and its tabs", (c) => call(via(c.tab ?? c.session, "pane.close"), paneOf(c) ?? null)],
  ["zoom", "Zoom pane", (c) => call(via(c.tab ?? c.session, "pane.update"), paneOf(c) ?? null, { zoom: "toggle" })],
  ["clear_tab", "Clear terminal", (c) => (tab(c) !== undefined ? call(via(c.tab, "tab.call"), tab(c), "clear_history", {}) : undefined)],
  ["cycle_pane_next", "Next pane", (c) => call(via(c.tab ?? c.session, "pane.focus"), paneOf(c) ?? null, { offset: 1 })],
  ["cycle_pane_previous", "Previous pane", (c) => call(via(c.tab ?? c.session, "pane.focus"), paneOf(c) ?? null, { offset: -1 })],
  ["focus_pane_left", "Focus pane left", focus("left")],
  ["focus_pane_right", "Focus pane right", focus("right")],
  ["focus_pane_up", "Focus pane up", focus("up")],
  ["focus_pane_down", "Focus pane down", focus("down")],
  ["resize_pane_left", "Resize pane left", resize("left")],
  ["resize_pane_right", "Resize pane right", resize("right")],
  ["resize_pane_up", "Resize pane up", resize("up")],
  ["resize_pane_down", "Resize pane down", resize("down")],
  ["font_bigger", "Bigger text", () => fontSize((s) => s + 1)],
  ["font_smaller", "Smaller text", () => fontSize((s) => s - 1)],
  ["font_reset", "Default text size", () => fontSize(() => FONT_DEFAULT.size)],
  ["reload_config", "Reload keymap", () => rebuild()],
  ["settings", "Open settings", (c) => openSettings((c.args as { page?: string } | undefined)?.page, (c.args as { link?: string } | undefined)?.link)],
  ["diagnostics", "Open diagnostics", () => engine.openDiagnostics()],
  ["restart_to_update", "Restart to update", () => engine.restartToUpdate()],
  ["toggle_sidebar", "Toggle sidebar", () => layout.toggle(layout.PREFS.sidebar)],
  ["toggle_pane_titles", "Toggle pane title bars", () => layout.toggleSetting("pane_titles")],
  ["toggle_pane_tabs", "Toggle tabs in panes", () => call("option.set", "pane_tabs", store.get().options.pane_tabs === false, {})],
  ["equalize_panes", "Equalize panes", (c) => (c.view ? layout.equalize(c.view) : undefined)],
  ["toggle_auto_equalize", "Toggle equalizing panes on open and close", () => layout.toggleSetting("pane_equalize")],
  ["quit", "Quit", (c) => c.window.manager.quit()],
]

for (const [name, label, run] of APP) {
  registry.set(name, { name, label, group: "App", scope: "global", range: name.startsWith("switch_"), run })
}

/** By window id: what its ⌘1…⌘9 go to while a part of it numbers its own rows (the side bar's). */
const numbered = new Map<number, (n: number) => void>()

/**
 * Gives a window's `switch_view` (⌘1…⌘9) to `go`, which is passed the number, until what it
 * returns is called: the side bar numbers its rows across every session. Without it, the keys go
 * to the shown session's views by their number.
 */
export function numberViews(window: number, go: (n: number) => void): () => void {
  numbered.set(window, go)
  return () => {
    if (numbered.get(window) === go) numbered.delete(window)
  }
}

/** Adds a command while its owner lives; returns what removes it. */
export function register(def: CommandDef): () => void {
  registry.set(def.name, def)
  rebuild()
  return () => {
    if (registry.get(def.name) === def) registry.delete(def.name)
    rebuild()
  }
}

/** What the mounted parts say the keys are: the themes' bindings. */
const themeKeys = new Map<object, { keys: string; command: string }>()

/**
 * What the window's theme says a chord runs, while the calling part is mounted: the theme owns its
 * shortcuts, so `cmd+w` is closing a tab in one theme and a window in another. The chord leaves
 * whichever command had it; `keymap_bindings` wins over it.
 *
 * ```tsx
 * useBinding("cmd+w", "close_tab")
 * ```
 */
export function useBinding(keys: string, command: string) {
  useEffect(() => bindKeys(keys, command), [keys, command])
}

/** `useBinding` outside a component: the chord runs `command` until what it returns is called. */
export function bindKeys(keys: string, command: string): () => void {
  const owner = {}
  themeKeys.set(owner, { keys, command })
  rebuild()
  return () => {
    themeKeys.delete(owner)
    rebuild()
  }
}

/** Registers `name` while the component is mounted: `run` is called with the context. */
export function useCommand(name: string, label: string, run: (c: Context, index?: number) => unknown, group = "Window") {
  useEffect(() => register({ name, label, group, scope: "global", run }), [name, label, group, run])
}

/** The plugins' commands, from their manifests: running one calls its function. */
function pluginCommands(): CommandDef[] {
  const out: CommandDef[] = []
  for (const entry of plugins.entries.values()) {
    for (const command of entry.manifest.contributes?.commands ?? []) {
      const name = `${entry.prefix}${command.id}`
      out.push({
        name,
        label: command.title || humanize(command.id),
        group: entry.title ?? entry.manifest.name,
        scope: "global",
        run: (c) => engine.call(name, c.args === undefined ? [] : [c.args], c.tab ? { tab: c.tab } : undefined),
      })
    }
  }
  return out
}

/** Every command there is now. */
export function commands(): CommandDef[] {
  const all = new Map(registry)
  for (const c of pluginCommands()) if (!all.has(c.name)) all.set(c.name, c)
  return [...all.values()]
}

export function currentKeymap(): Keymap {
  return keymap
}

// What menus write beside an action of a command: its keys as the keymap has them now.
setChordFor((command, index) => keymap.chordFor(command, index))

export function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** Builds the keymap again from the commands and the options, and tells the window what to take. */
export function rebuild() {
  const options = store.get().options
  keymap = new Keymap({
    commands: commands(),
    defaults: process.platform === "darwin" ? DEFAULTS : DEFAULTS_OTHER,
    theme: [...themeKeys.values()],
    user: (options.keymap_bindings ?? {}) as Record<string, string[]>,
  })
  capture()
  for (const listener of listeners) listener()
}

let pendingKeys: string[] = []
/** How many modals hold the keyboard (`useHoldKeys`). */
let held = 0
let pendingTimer: ReturnType<typeof setTimeout> | undefined
const SEQUENCE_TIMEOUT = 1000

/** Tells the window which keys to take before the focused element: the app's and the server's,
 * or none while a modal holds the keyboard. */
function capture() {
  if (held) return setCapturedKeys([], false)
  setCapturedKeys([...keymap.firsts, ...serverKeys.captured()], pendingKeys.length > 0 || serverKeys.pending())
}

/** While the component is mounted every key goes to the focused element: the app's bindings (and
 * the server's) take none, as a modal's own keys (`cmd+enter`, `escape`) must reach it. */
export function useHoldKeys() {
  useEffect(() => {
    held++
    pendingKeys = []
    capture()
    return () => {
      held--
      capture()
    }
  }, [])
}

/** Runs `name` in `context`. */
export function run(name: string, context: Context, index?: number): void {
  const command = commands().find((c) => c.name === name)
  if (!command) return console.error(`pumi: no command ${name}`)
  try {
    Promise.resolve(command.run(context, index)).catch(swallow)
  } catch (err) {
    swallow(err)
  }
}

/** A key the window took (`windowKeyDown`): a binding of the app's, else the server's. */
export function key(event: { key?: string | null; modifiers?: any }, context: Context, scopes: string[] = ["global"]) {
  if (held) return
  const chord = chordOf(event)
  if (["shift", "control", "alt", "platform", "function", "ctrl", "cmd"].includes(chord.split("+").at(-1)!)) return
  const resolved = keymap.resolve(scopes, pendingKeys, chord)
  clearTimeout(pendingTimer)
  if ("run" in resolved) {
    pendingKeys = []
    capture()
    return run(resolved.run.name, context, resolved.run.index)
  }
  if ("pending" in resolved) {
    pendingKeys.push(chord)
    capture()
    pendingTimer = setTimeout(() => {
      const fallback = resolved.fallback
      pendingKeys = []
      capture()
      if (fallback) run(fallback.name, context, fallback.index)
    }, SEQUENCE_TIMEOUT)
    return
  }
  pendingKeys = []
  if (serverKeys.takes(chord)) serverKeys.press(chord, context.tab).then(capture, swallow)
  capture()
}

let lastOptions: unknown
store.subscribe(() => {
  const { options } = store.get() as any
  if (options !== lastOptions) {
    lastOptions = options
    serverKeys.follow(options)
    rebuild()
  }
})
plugins.subscribe(rebuild)
