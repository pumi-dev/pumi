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
