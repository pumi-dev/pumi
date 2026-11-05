// What a tab and a view are called wherever the app shows them: a view by its name when the user
// gave it one, else by the tab last active in it (the first, without one). A view is never
// "View 1": an unnamed view's name is `null` on the wire, not its index.
import { basename } from "node:path"
import type { Snapshot, Tree } from "./engine.ts"
import { shownOf } from "./layout.ts"
import { entries, prefixOf } from "./plugins.ts"

/**
 * The name a block's plugin gives it in its manifest (`New Tab`, for the Launcher's `new`): what
 * a tab of a plugin block goes by when it has no name, title or directory of its own, rather than
 * the block's id (`pumi_dev.launcher.new`). `undefined` for a core block's, or one no plugin of
 * ours declares (a tab whose plugin is gone keeps the id).
 */
export function blockTitle(block: string): string | undefined {
  for (const e of entries.values()) {
    const prefix = prefixOf(e.id)
    if (!block.startsWith(prefix)) continue
    const mine = e.manifest.contributes?.blocks?.find((b) => b.id === block.slice(prefix.length))
    if (mine?.title) return mine.title
  }
  return undefined
}

/** A tab by what it shows: its name, its title, its directory, its block's (`New Tab`), its id. */
export function tabLabel(p: { block: string; label?: string; terminal_title?: string | null; cwd?: string | null }): string {
  const dir = p.cwd ? basename(p.cwd) : undefined
  return p.label || p.terminal_title || dir || blockTitle(p.block) || p.block
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
  return tab ? tabLabel(tab) : UNNAMED
}
