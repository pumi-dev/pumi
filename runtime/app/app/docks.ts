// A theme's docks (`defineTheme({ docks })`): each one's toggle as a command (`toggle_<name>`)
// with its keys while the theme is drawn, and what the app keeps of it — whether it shows, how big
// the user dragged it, which of its panel components is in front. The window's own side bar is the
// dock `sidebar` (`default.sidebar`, `default.sidebar.width`), so a
// theme that declares it and one that doesn't share it. Nothing here draws: `sdk/dock.tsx` does.
import { humanize } from "./keymap.ts"
import * as commands from "./commands.ts"
import { getLocal, setLocal } from "./local.ts"
import { PREFS } from "./layout.ts"
import type { DockSpec } from "../sdk/theme.ts"

/** What a dock's size is until the user drags it, and how far a drag takes it. */
export const DOCK_SIZE = { size: 260, min: 160, max: 640 }

/** A dock's name: what its command is made from (`toggle_<name>`). */
const NAME = /^[a-z][a-z0-9_]*$/

/** The window's own side bar: its command is the app's, there whatever the theme. */
export const SIDEBAR = "sidebar"

/** Where the app keeps what a dock is. */
export function dockKey(name: string, what: "open" | "size" | "component"): string {
  if (name === SIDEBAR && what === "open") return PREFS.sidebar
  if (name === SIDEBAR && what === "size") return PREFS.sidebarWidth
  return `dock.${name}.${what}`
}

/** The command that shows and hides a dock. */
export const dockCommand = (name: string) => `toggle_${name}`

/** The docks of the themes drawn now, the last first (a window each; usually one theme). */
const drawn: Record<string, DockSpec>[] = []

/** The docks there are now, by name: the theme's in front. */
export function declaredDocks(): Record<string, DockSpec> {
  return drawn.at(-1) ?? {}
}

export function isOpen(name: string): boolean {
  return getLocal(dockKey(name, "open"), declaredDocks()[name]?.open ?? true)
}

export function setOpen(name: string, open: boolean) {
  setLocal(dockKey(name, "open"), open)
}

export function toggleDock(name: string) {
  setOpen(name, !isOpen(name))
}

/** A dock's title: its own, or its name's words (`secondary_sidebar` → `Secondary sidebar`). */
export function dockTitle(name: string, spec: DockSpec | undefined): string {
  return spec?.title ?? humanize(name)
}

/**
 * Makes a theme's docks real while it is drawn: each one's `toggle_<name>` command and its keys.
 * Returns what takes them away. A name that is not a command's (`a-z`, digits, `_`), or whose
 * toggle is another command's, is refused in the log.
 */
export function declareDocks(docks: Record<string, DockSpec>): () => void {
  const offs: (() => void)[] = []
  // The docks it has, of those it declares: a refused one is in no menu.
  const taken: Record<string, DockSpec> = {}
  drawn.push(taken)
  for (const [name, spec] of Object.entries(docks)) {
    if (!NAME.test(name)) {
      console.error(`pumi: the dock ${JSON.stringify(name)}: a dock's name is a-z, digits and _, from a letter`)
      continue
    }
    const command = dockCommand(name)
    if (name !== SIDEBAR) {
      const had = commands.commands().find((c) => c.name === command)
      if (had && had.group !== DOCK_GROUP) {
        console.error(`pumi: the dock ${name}: ${command} is already a command (${had.label})`)
        continue
      }
      offs.push(commands.register({ name: command, label: `Toggle ${dockTitle(name, spec).toLowerCase()}`, group: DOCK_GROUP, scope: "global", run: () => toggleDock(name) }))
    }
    if (spec.keys) offs.push(commands.bindKeys(spec.keys, command))
    taken[name] = spec
  }
  commands.rebuild()
  return () => {
    const at = drawn.lastIndexOf(taken)
    if (at >= 0) drawn.splice(at, 1)
    for (const off of [...offs].reverse()) off()
    commands.rebuild()
  }
}

/** The group a dock's command is in: the commands' lists show it so, and a dock knows its own. */
export const DOCK_GROUP = "Docks"
