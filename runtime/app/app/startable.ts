// What a new tab can start: one list, which the `+` menu shows and Settings ▸ General ▸ New tabs
// open picks from. Each plugin that is on brings its `tab.new` actions (Terminal, Pi, Claude
// Code…), or, when it has none, each block it declares (the Launcher's list, Film, Diff), opened
// with no arguments. Plugins come in the order the server lists them, each a group of its own.
//
// The `new_tab` option names one: an action by its function (`pumi_dev.agents.new_claude`), a block
// by its id (`pumi_dev.launcher.new`). While it names nothing installed, new tabs open the
// Launcher's list, else a shell, else the first of the list: the server decides the same
// (`crates/pumi-host/src/api/tabs.rs`).
import type { ActionPick, Entry } from "./plugins.ts"

export type From = Pick<Entry, "id" | "title" | "prefix" | "manifest"> & Partial<Pick<Entry, "state" | "error">>

/** One thing a new tab can start. */
export interface Startable {
  /** What `new_tab` names it by: an action's function, or a block's id. */
  value: string
  title: string
  /** One of the app's icons by name, or another form of icon (`plugin:icon.svg`). */
  icon?: string
  /** Its plugin's id (`owner/name`): the list's group. */
  plugin: string
  /** Its plugin's name as people read it. */
  pluginTitle: string
  /** The `tab.new` action it is, by its `id` and its command (unprefixed); none for a block. */
  action?: { id: string; command: string; pick?: ActionPick }
  /** A block `new_tab` may name for it too: the one block of a plugin whose one action this is
   *  (Terminal's shell, which the Terminal plugin sets on its first run). */
  alias?: string
}

/** The blocks new tabs open, in this order, while `new_tab` names nothing installed. */
export const PREFERRED = ["pumi_dev.launcher.new", "pumi_dev.terminal.shell"]

/** A plugin that loaded: one whose manifest could not be read, or is for another API, has none. */
const loaded = (e: From) => !(e.state === "disabled" && e.error)

export function startables(entries: Iterable<From>): Startable[] {
  const out: Startable[] = []
  for (const e of entries) {
    if (!loaded(e)) continue
    const c = e.manifest.contributes
    const pluginTitle = e.title ?? e.manifest.name ?? e.id
    const blocks = c?.blocks ?? []
    const actions = c?.slots?.["tab.new"]?.actions ?? []
    if (actions.length) {
      const alias = actions.length === 1 && blocks.length === 1 && !actions[0]!.pick ? `${e.prefix}${blocks[0]!.id}` : undefined
      for (const a of actions) {
        out.push({ value: `${e.prefix}${a.command}`, title: a.title, icon: a.icon, plugin: e.id, pluginTitle, action: { id: a.id, command: a.command, pick: a.pick }, alias })
      }
      continue
    }
    for (const b of blocks) out.push({ value: `${e.prefix}${b.id}`, title: b.title ?? b.id, icon: b.icon, plugin: e.id, pluginTitle })
  }
  return out
}

/** Whether `new_tab` can name it: an action that asks for files first can't open on its own. */
export const settable = (s: Startable) => !s.action?.pick

/** The one of `list` that `value` names, by its value or its alias. */
export function named(list: Startable[], value: string): Startable | undefined {
  if (!value) return undefined
  return list.find((s) => settable(s) && (s.value === value || s.alias === value))
}

/** What new tabs open while `new_tab` is `value`: what it names, else the Launcher's list, else a shell, else the first. */
export function newTabOf(list: Startable[], value: string): Startable | undefined {
  return named(list, value) ?? PREFERRED.map((b) => named(list, b)).find(Boolean) ?? list.find(settable)
}

/**
 * The letter that picks each of `titles` while its list has the keys (the Launcher's tab, the `+`
 * menu), in the same order: the same title in the same list gets the same letter wherever it is
 * shown, whatever order the list is drawn in. Each takes its first letter if no title before it
 * has; those left take the first letter of another of their words (`Cursor Agent`'s `A`,
 * `OpenCode`'s `C`), then any other of their letters or digits, in order; past those, a digit. One
 * of `reserved` (a list's own rows: the Launcher's `File…`) is no one's. `""` once all are taken.
 */
export function letters(titles: string[], reserved: Iterable<string> = []): string[] {
  const taken = new Set([...reserved].map((k) => k.toLowerCase()))
  const out = titles.map(() => "")
  const tiers = titles.map((title) => {
    const words = title.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase().split(/[^a-z0-9]+/).filter(Boolean)
    const first = words[0]?.[0]
    const initials = words.slice(1).map((w) => w[0]!)
    const rest = [...words.join("")]
    return [first ? [first] : [], initials, rest]
  })
  for (let tier = 0; tier < 3; tier++) {
    tiers.forEach((t, i) => {
      if (out[i]) return
      const k = t[tier]!.find((c) => !taken.has(c))
      if (k === undefined) return
      out[i] = k
      taken.add(k)
    })
  }
  const digits = [..."1234567890"].filter((d) => !taken.has(d))
  return out.map((k) => (k || digits.shift() || "").toUpperCase())
}

/** How many times each was opened from a list, by its id: what `byUse` orders them by. */
export type Uses = Record<string, number>

/** `list` the most used first (`uses`, by `id`); those used as often keep the order they had. */
export function byUse<T extends { id: string }>(list: T[], uses: Uses): T[] {
  const n = (t: T) => (typeof uses[t.id] === "number" ? uses[t.id]! : 0)
  return list
    .map((t, i) => [t, i] as const)
    .sort(([a, i], [b, j]) => n(b) - n(a) || i - j)
    .map(([t]) => t)
}
