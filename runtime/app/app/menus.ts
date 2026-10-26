// The macOS menu bar. The application menu (About, Settings…, Keybindings…, Services, Hide, Quit)
// is the addon's; everything else is here, built from the command registry so a theme's or a
// plugin's command reaches the bar without a Rust change. A click comes back as a `menu` event
// (`on("menu", …)`, see `windows.tsx`) and runs in the window in front.
//
// `Edit` is missing on purpose: AppKit takes a menu's key equivalent before the window sees it, so
// ⌘C and ⌘V would never reach the text elements that handle them. `Pane` and `View` show what is
// there — a command that is not registered right now (a theme without a sidebar) is left out.
import { engine, on } from "./engine.ts"
import { commands, currentKeymap, subscribe } from "./commands.ts"
import type { WindowManager } from "./windows.tsx"
import { declaredDocks, dockCommand, dockTitle, SIDEBAR } from "./docks.ts"

/** Where the View menu lists the theme's docks. */
const DOCKS = "<docks>"

/** The theme's docks, as the View menu's items (the side bar is there already). */
function dockEntries(): Entry[] {
  return Object.entries(declaredDocks())
    .filter(([name]) => name !== SIDEBAR)
    .map(([name, spec]) => ({ command: dockCommand(name), title: dockTitle(name, spec) }))
}

/** A menu item as the bar is built here, before its command is looked up. */
interface Entry {
  /** The command to run; the title and the chord are the keymap's unless given. */
  command?: string
  title?: string
  separator?: boolean
  name?: string
  items?: Entry[]
}

/** …and as the addon reads it (`src/menus.rs`). */
interface Item {
  command?: string
  title?: string
  chord?: string
  separator?: boolean
  name?: string
  items?: Item[]
}

interface Menu {
  name: string
  items: Entry[]
}

const BAR: Menu[] = [
  {
    name: "File",
    items: [
      { command: "new_view" },
      { command: "new_workspace" },
      { command: "new_window" },
      { separator: true },
      { command: "close_tab" },
      { command: "close_view" },
      { command: "close_workspace" },
      { command: "close_window" },
      { separator: true },
      { command: "reopen_closed" },
    ],
  },
  {
    name: "View",
    items: [
      { command: "toggle_sidebar", title: "Primary Side Bar" },
      // The theme's docks (`docks`), each by its title: filled in by `bar()`.
      { name: DOCKS },
      { separator: true },
      { command: "font_bigger" },
      { command: "font_smaller" },
      { command: "font_reset" },
      { separator: true },
      { command: "fullscreen" },
    ],
  },
  {
    name: "Pane",
    items: [
      { command: "split_vertical" },
      { command: "split_horizontal" },
      { command: "zoom", title: "Zoom Pane" },
      { separator: true },
      { command: "cycle_pane_next" },
      { command: "cycle_pane_previous" },
      { separator: true },
      {
        name: "Focus Pane",
        items: [
          { command: "focus_pane_left", title: "Left" },
          { command: "focus_pane_right", title: "Right" },
          { command: "focus_pane_up", title: "Up" },
          { command: "focus_pane_down", title: "Down" },
        ],
      },
      {
        name: "Resize Pane",
        items: [
          { command: "resize_pane_left", title: "Left" },
          { command: "resize_pane_right", title: "Right" },
          { command: "resize_pane_up", title: "Up" },
          { command: "resize_pane_down", title: "Down" },
        ],
      },
      { separator: true },
      { command: "clear_tab" },
    ],
  },
  {
    name: "Window",
    items: [
      { command: "minimize_window", title: "Minimize" },
      { command: "zoom_window", title: "Zoom" },
      { separator: true },
      { command: "next_view" },
      { command: "previous_view" },
      { separator: true },
      { command: "next_workspace" },
      { command: "previous_workspace" },
      { separator: true },
      { command: "diagnostics", title: "Diagnostics" },
    ],
  },
  {
    name: "Help",
    items: [{ command: "help" }, { command: "reload_config" }],
  },
]

/** Title case, the way menus write a command's label (`New tab` → `New Tab`). */
export function titleCase(label: string): string {
  const minor = new Set(["a", "an", "and", "as", "at", "by", "for", "in", "of", "on", "or", "the", "to"])
  return label
    .split(" ")
    .map((word, at) => (at === 0 || !minor.has(word) ? word.charAt(0).toUpperCase() + word.slice(1) : word))
    .join(" ")
}

/** An entry as a menu item, or nothing when its command is not registered right now. */
function item(entry: Entry, known: Set<string>, keymap: ReturnType<typeof currentKeymap>): Item | undefined {
  if (!entry.command || !known.has(entry.command)) return undefined
  const chord = keymap.chordFor(entry.command)
  const label = keymap.commands.find((c) => c.name === entry.command)?.label
  return { command: entry.command, title: entry.title ?? titleCase(label ?? entry.command), ...(chord ? { chord } : {}) }
}

function items(entries: Entry[], known: Set<string>, keymap: ReturnType<typeof currentKeymap>): Item[] {
  const out: Item[] = []
  for (const entry of entries) {
    if (entry.name === DOCKS && !entry.items) {
      out.push(...items(dockEntries(), known, keymap))
      continue
    }
    if (entry.separator) {
      out.push({ separator: true })
      continue
    }
    if (entry.items) {
      const submenu = items(entry.items, known, keymap)
      // A submenu of nothing but separators would open empty: leave it out whole.
      if (submenu.some((i) => !i.separator)) out.push({ name: entry.name, items: submenu })
      continue
    }
    const resolved = item(entry, known, keymap)
    if (resolved) out.push(resolved)
  }
  return out
}

/** The whole bar, for the app in the state it is in now. */
export function bar(): { name: string; items: Item[] }[] {
  const known = new Set(commands().map((c) => c.name))
  const keymap = currentKeymap()
  return BAR.map((menu) => ({ name: menu.name, items: items(menu.items, known, keymap) })).filter((menu) =>
    menu.items.some((i) => !i.separator),
  )
}

/** Tells the addon what the bar is now. Cheap enough to do whenever the commands change. */
export function install(): void {
  engine.setMenus(JSON.stringify(bar()))
}

let installed = false

/** Installs the bar, and keeps it true as commands come and go. The bar is macOS's: every other
 *  OS keeps its own window (an app's menu there is a Windows or GTK one), and the commands are
 *  in the window and on their keys. */
export function watch(manager: WindowManager): void {
  if (process.platform !== "darwin") return
  if (!installed) {
    installed = true
    subscribe(install)
    on("menu", (data: { command?: string }) => {
      if (data?.command) manager.menu(data.command)
    })
  }
  install()
}
