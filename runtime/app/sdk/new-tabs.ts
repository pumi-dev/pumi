// What a new tab can be, as one list: the `+` menu's and the Launcher's. It is `startable.ts`'s
// (each plugin's `tab.new` actions, or its blocks when it has none), drawn the most used first,
// each row with the letter that picks it while the list has the keys (`letters`). A row opened
// from either list counts as a use of it (`USES`, kept in window.json), so both keep one order.
import { useMemo, useSyncExternalStore } from "react"
import { call, via } from "../app/engine.ts"
import { getLocal, setLocal, useLocal } from "../app/local.ts"
import * as plugins from "../app/plugins.ts"
import { fromServer } from "../app/serverkeys.ts"
import { useApp } from "../app/state.ts"
import { byUse, letters, startables, type Uses } from "../app/startable.ts"
import { keyGlyphs } from "./actions.ts"
import { useSlot } from "./contributions.ts"

/** Where a new tab goes: into a pane, as one of its tabs, or in a tab's place (the Launcher's). */
export type NewTabPlace = { kind: "tiled"; pane: string } | { kind: "tiled"; replace: string }

/** One thing a new tab can be: a row of the `+` menu and of the Launcher's list. */
export interface NewTab {
  /** `owner/name/<action id>` for an action, `owner/name/<block>` for a block. */
  id: string
  title: string
  /** One of the app's icons by name: a block's own file (`plugin:icon.svg`) is a box. */
  icon: string
  /** Its plugin's id (`owner/name`). */
  plugin: string
  /** What the `new_tab` option names it by: an action's function, or a block's id. */
  value: string
  /** The key that picks it while its list has the keys (`C`), the same in both lists; `""` for none. */
  letter: string
  /** The keys that open one from anywhere, when they are bound: the app's command, or the server's. */
  keys?: string
  /** Opens it, through tab or pane `at` (it names the server), at `place`; counts as a use of it. */
  open(at: string, place: NewTabPlace): Promise<unknown>
}

/** window.json's key: how many times each row was opened from either list, by its id. */
export const USES = "new_tab.uses"

/** The letters no row gets: the Launcher's own `File…` is `F`, so a row's letter is the same in both lists. */
export const RESERVED_LETTERS = ["f"]

/** Counts one use of the row `id`. */
export function noteUse(id: string) {
  const uses = getLocal<Uses>(USES, {})
  setLocal(USES, { ...uses, [id]: (typeof uses[id] === "number" ? uses[id] : 0) + 1 })
}

/** The keys the server's tables (`keys`) bind to `fn`, as the OS writes them (`⇧ ⌘ E`). */
function serverShortcut(tables: unknown, fn: string): string | undefined {
  if (!tables || typeof tables !== "object") return undefined
  for (const name of ["root", "global"]) {
    const table = (tables as Record<string, unknown>)[name]
    if (!table || typeof table !== "object") continue
    for (const [key, binding] of Object.entries(table as Record<string, { call?: unknown } | null>)) {
      if (binding?.call === fn) return keyGlyphs(key.split(" ").map((k) => fromServer(k) ?? k).join(" "))
    }
  }
  return undefined
}

let pluginsVersion = 0
plugins.subscribe(() => pluginsVersion++)

/** An icon of the app's by name, for a row: a block's own file (`plugin:icon.svg`) is a box. */
const menuIcon = (icon: string | undefined) => (icon && /^[a-z0-9_]{1,64}$/.test(icon) ? icon : "box")

/**
 * What a new tab can be, the most used first: the list Settings ▸ General ▸ New tabs open picks
 * from too (`startable.ts`). An action is run with `{tab, place}`: the pane or tab it opens beside
 * (it names the server), and where; one with a `pick` asks for its files first. A block opens there
 * with no arguments. Letters are given in the list's own order, before it is sorted, so a row keeps
 * its letter as the uses change.
 */
export function useNewTabs(): NewTab[] {
  const { actions } = useSlot("tab.new")
  const tables = useApp((s) => s.options.keys)
  const version = useSyncExternalStore(plugins.subscribe, () => pluginsVersion)
  const [uses] = useLocal<Uses>(USES, {})
  const list = useMemo(() => {
    const slotted = new Map(actions.map((a) => [`${a.plugin}/${a.id}`, a]))
    const all = startables(plugins.entries.values())
    const keys = letters(all.map((s) => s.title), RESERVED_LETTERS)
    return all.map((s, i): NewTab => {
      const id = s.action ? `${s.plugin}/${s.action.id}` : `${s.plugin}/${s.value}`
      const slot = slotted.get(id)
      const command = s.action ? s.value : undefined
      return {
        id,
        title: s.title,
        icon: menuIcon(s.icon ?? (s.action ? "plus" : undefined)),
        plugin: s.plugin,
        value: s.value,
        letter: keys[i]!,
        keys: slot?.keys ?? (command ? serverShortcut(tables, command) : undefined),
        open: async (on, place) => {
          noteUse(id)
          if (slot) return slot.run({ tab: on, place })
          if (s.action) return plugins.command(s.plugin, s.action.command, { tab: on, place })
          return call(via(on, "tab.open"), s.value, {}, place)
        },
      }
    })
  }, [actions, tables, version])
  return useMemo(() => byUse(list, uses), [list, uses])
}

/** The row of `list` a key picks (`c`, any case), by its letter. */
export function byLetter<T extends { letter: string }>(list: T[], key: string): T | undefined {
  const k = key.toLowerCase()
  return k ? list.find((row) => row.letter !== "" && row.letter.toLowerCase() === k) : undefined
}
