// Actions: what can be done to a thing (a tab, a view, a session), as data a theme draws however
// it likes — a menu, a toolbar, a Start menu, a palette. The app's own (split, zoom, close, move,
// rename…) and the plugins' (`contributes.slots.<slot>.actions`) are the same kind of thing and
// come out of the same slot together: `useSlot("tab.menu", { tab })` gives every item of a tab's
// menu, ready to run.
import { useContext, useRef } from "react"
import { useApp, WindowContext, type State, type WindowInfo } from "../app/state.ts"

/** One thing to do: a menu item, a toolbar button. */
export interface Action {
  id: string
  title: string
  /** One of the app's icons. */
  icon?: string
  /** Whose it is: a plugin's id (`owner/name`), or `pumi` for the app's own. */
  plugin: string
  /** The command it is, when it is one: its keys are `keys`. */
  command?: string
  /** The keys that run it, as its OS writes them (`⇧ ⌘ D`, `Ctrl+Shift+D`). */
  keys?: string
  /** Items of one group sit together; a menu draws a rule between groups (`grouped`). */
  group?: string
  /** A toggle that is on, or the choice made among its group. */
  checked?: boolean
  /** Shown, but not to be run now. */
  disabled?: boolean
  /** It ends something (a close): drawn as a warning, and it asks first when `confirm` says so. */
  danger?: boolean
  /** What it opens instead of running: a submenu. */
  items?: Action[]
  /** Runs it with what its slot was given, or with `args` instead. */
  run(args?: unknown): void
}

/** Another name for `Action`. */
export type ActionItem = Action

/** What a provider makes its actions from: the app's state now, and the window they are for. */
export interface ActionContext {
  state: State
  window?: WindowInfo
}

/**
 * Makes the actions a slot has for what it is given (`{ tab }`), from the app's state as it is
 * now. Each one's `run` already knows what it is for.
 */
export type ActionProvider = (args: Record<string, unknown>, context: ActionContext) => Action[]

const providers = new Map<string, ActionProvider[]>()
const listeners = new Set<() => void>()
let version = 0

/**
 * Adds actions of your own to `point` (`tab.menu`, `view.menu`, a slot of a theme's own): they
 * come out of `useSlot(point)` beside the app's and the plugins'. Returns what takes them away.
 */
export function addActions(point: string, provider: ActionProvider): () => void {
  providers.set(point, [...(providers.get(point) ?? []), provider])
  version++
  for (const l of listeners) l()
  return () => {
    providers.set(point, (providers.get(point) ?? []).filter((p) => p !== provider))
    version++
    for (const l of listeners) l()
  }
}

export function subscribeProviders(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export const providersVersion = () => version

/** Has every slot's actions made again: for a provider whose actions changed outside the store. */
export function refreshActions() {
  version++
  for (const l of listeners) l()
}

const NONE: Action[] = []

/** What makes an action draw differently: two lists with the same signature draw the same. */
function signature(list: Action[]): string {
  return list.map((a) => `${a.plugin}/${a.id}|${a.title}|${a.icon ?? ""}|${a.keys ?? ""}|${a.group ?? ""}|${a.checked ? 1 : 0}${a.disabled ? 1 : 0}${a.danger ? 1 : 0}|${a.items ? `[${signature(a.items)}]` : ""}`).join("\n")
}

/**
 * The app's (and themes') actions for `point`, given `args`: the same array while nothing about
 * them changed, so what draws them is not drawn again for nothing.
 */
export function useProvided(point: string, args: Record<string, unknown> | undefined): Action[] {
  const last = useRef<{ sig: string; list: Action[] }>(undefined)
  const window = useContext(WindowContext) ?? undefined
  const key = `${window?.id ?? ""}|${window?.session ?? ""}|${args ? JSON.stringify(args) : ""}`
  return useApp((state) => {
    const list = (providers.get(point) ?? NONE).flatMap((provide) => provide(args ?? {}, { state, window }))
    if (!list.length) return NONE
    const sig = `${key}\n${signature(list)}`
    if (last.current?.sig === sig) return last.current.list
    // The same items with fresh `run`s: kept, as what they run reads the store when they run.
    last.current = { sig, list: list.map(withKeys) }
    return last.current.list
  })
}

/** An action with its command's keys, when it has a command and no keys of its own. */
function withKeys(a: Action): Action {
  const keys = a.keys ?? (a.command ? shortcut(a.command) : undefined)
  const items = a.items?.map(withKeys)
  return keys === a.keys && items === undefined ? a : { ...a, ...(keys ? { keys } : {}), ...(items ? { items } : {}) }
}

/** Consecutive actions of one group together, for a menu to draw a rule between. */
export function grouped(actions: readonly Action[]): Action[][] {
  const out: Action[][] = []
  for (const a of actions) {
    const last = out[out.length - 1]
    if (last && (last[0]!.group ?? "") === (a.group ?? "")) last.push(a)
    else out.push([a])
  }
  return out
}

const MACOS = process.platform === "darwin"
/** A chord's modifiers, in the order it is written: macOS's glyphs (`⇧ ⌘ D`), every other OS's
 *  names (`Win+Shift+S`). */
const MODIFIERS: [string, string][] = MACOS
  ? [["ctrl", "⌃"], ["alt", "⌥"], ["shift", "⇧"], ["cmd", "⌘"]]
  : [["cmd", "Win"], ["ctrl", "Ctrl"], ["alt", "Alt"], ["shift", "Shift"]]
/** The keys macOS draws as a glyph (`↵`), and how the other OSes name them (`Enter`). */
const KEY_NAMES: Record<string, [string, string]> = {
  enter: ["↵", "Enter"], escape: ["⎋", "Esc"], backspace: ["⌫", "Backspace"], delete: ["⌦", "Delete"],
  tab: ["⇥", "Tab"], space: ["Space", "Space"], left: ["←", "←"], right: ["→", "→"], up: ["↑", "↑"], down: ["↓", "↓"],
  pageup: ["⇞", "PageUp"], pagedown: ["⇟", "PageDown"], equal: ["=", "="], minus: ["-", "-"], plus: ["+", "+"],
}

/** The keymap's chords for a command (`cmd+shift+d`): the app's commands give it (`setChordFor`). */
let chordFor: (command: string, index?: number) => string | undefined = () => undefined

export function setChordFor(lookup: (command: string, index?: number) => string | undefined) {
  chordFor = lookup
}

/** The keys `command` answers to as its OS writes them (macOS's `⇧ ⌘ D`, else `Ctrl+Shift+D`),
 *  or nothing when it has none. Of a range command, the keys of `index` (`switch_tab`, 3: `⌘ 3`). */
export function shortcut(command: string, index?: number): string | undefined {
  const keys = chordFor(command, index)
  return keys ? keyGlyphs(keys) : undefined
}

/**
 * The keys of `command` (of a range's `index`) as `shortcut` writes them, when what is held down
 * (`useHeldModifiers`: `cmd`, `shift+cmd`) is just what they need before their key: what a theme
 * shows beside what they do while ⌘ is held. Nothing else, and nothing for a sequence.
 */
export function heldShortcut(held: string, command: string, index?: number): string | undefined {
  if (!held) return undefined
  const keys = chordFor(command, index)
  const at = keys?.lastIndexOf("+") ?? -1
  if (!keys || at < 0 || keys.includes(" ")) return undefined
  return keys.slice(0, at) === held ? keyGlyphs(keys) : undefined
}

/** Keys as the keymap writes them (`cmd+shift+d`, `ctrl+x ctrl+f`) as the OS writes them: a Mac
 *  menu's `⇧ ⌘ D`, else `Ctrl+Shift+D`. */
export function keyGlyphs(keys: string): string {
  return keys
    .split(" ")
    .filter(Boolean)
    .map((chord) => {
      const parts = chord.split("+")
      const key = parts.pop() ?? ""
      const mods = MODIFIERS.filter(([name]) => parts.includes(name)).map(([, label]) => label)
      const named = KEY_NAMES[key]?.[MACOS ? 0 : 1]
      const glyph = named ?? (key.length === 1 ? key.toUpperCase() : key ? key[0]!.toUpperCase() + key.slice(1) : "")
      return [...mods, glyph].join(MACOS ? " " : "+")
    })
    .join(MACOS ? "   " : " ")
}
