// Settings → General → New tabs open: the `new_tab` option as a menu of the blocks the plugins that
// are on declare (Launcher's list, Terminal's shell, Browser's page, …). pumi has no block of its
// own for a new tab, so with nothing picked a new tab doesn't open; with nothing installed there is
// nothing to pick, and the row says to install a plugin.
import { useSyncExternalStore } from "react"
import { Button, OptionRow, SelectField, type SelectOption } from "../../sdk/settings/index.ts"
import * as plugins from "../plugins.ts"

/** What the menu calls `new_tab` naming no block. */
export const NOTHING = "Nothing (install a plugin)"

type Entry = Pick<plugins.Entry, "id" | "title" | "prefix" | "active" | "manifest">

/**
 * The menu's choices: nothing, then every block of a plugin that is on, called by its title and
 * its plugin's name when they differ (`New Tab · Launcher`). A value naming a block no plugin that
 * is on declares stays, said to be missing, so the menu shows what is set.
 */
export function newTabChoices(entries: Iterable<Entry>, current: string): SelectOption[] {
  const out: SelectOption[] = [{ value: "", label: NOTHING }]
  for (const e of entries) {
    if (!e.active) continue
    const plugin = e.title ?? e.manifest.name ?? e.id
    for (const b of e.manifest.contributes?.blocks ?? []) {
      const title = b.title ?? b.id
      out.push({ value: `${e.prefix}${b.id}`, label: title === plugin ? title : `${title} · ${plugin}` })
    }
  }
  if (current && !out.some((o) => o.value === current)) out.push({ value: current, label: `${current} (not installed)` })
  return out
}

let version = 0
plugins.subscribe(() => version++)

export function NewTabRow({ open }: { open(page: string, link?: string): void }) {
  useSyncExternalStore(
    (l) => plugins.subscribe(l),
    () => version,
  )
  return (
    <OptionRow
      name="new_tab"
      control={(value, set) => {
        const current = typeof value === "string" ? value : ""
        const choices = newTabChoices(plugins.entries.values(), current)
        return (
          <>
            {choices.length === 1 ? (
              <Button primary onClick={() => open("store")}>
                Get plugins
              </Button>
            ) : null}
            <SelectField options={choices} value={current} onChange={(picked) => set(picked)} width={240} testId="select-new_tab" />
          </>
        )
      }}
    />
  )
}
