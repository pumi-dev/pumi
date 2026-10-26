// What a tab and a view are called wherever the app shows them: a view by its name when the user
// gave it one, else by the tab last active in it (the first, without one). A view is never
// "View 1": an unnamed view's name is `null` on the wire, not its index.
import { basename } from "node:path"
import type { Snapshot, Tree } from "./engine.ts"
import { shownOf } from "./layout.ts"

type Tab = Snapshot["tabs"][number]

/** The bundled `pumi/launcher`'s block: a new pane's first tab, until what goes in it is picked. */
export const NEW_TAB = "pumi.launcher.new"

/** A tab that is not an agent, by what it shows: its name, its title, its directory, its type. */
export function tabLabel(p: { block: string; label?: string; terminal_title?: string | null; cwd?: string | null }): string {
  if (p.block === NEW_TAB) return p.label || "New Tab"
  const dir = p.cwd ? basename(p.cwd) : undefined
  return p.label || p.terminal_title || dir || p.block
}

/** A tab as its title bar names it: an agent by its title first, else by `tabLabel`. */
export function tabTitle(p: Tab): string {
  return (p.agent && (p.terminal_title || p.label || p.agent)) || tabLabel(p)
}

/** What a view with no tabs, and no name, is called. */
const UNNAMED = "View"

/** A view's name: the user's, else its last active tab's title, else its first tab's. */
export function viewName(snapshot: Snapshot | undefined, trees: Record<string, Tree>, view: string): string {
  const it = snapshot?.views.find((v) => v.view_id === view)
  if (it?.label) return it.label
  const held = snapshot?.tabs.filter((t) => t.view_id === view) ?? []
  const active = snapshot?.layouts.find((l) => l.view_id === view)?.focused_tab_id
  const tree = trees[view]
  const first = tree ? shownOf(tree)[0] : undefined
  const tab = held.find((t) => t.tab_id === active) ?? held.find((t) => t.tab_id === first) ?? held[0]
  return tab ? tabTitle(tab) : UNNAMED
}
