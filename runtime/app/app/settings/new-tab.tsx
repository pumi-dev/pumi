// Settings → General → New tabs open: the `new_tab` option as a menu of what a new tab can start,
// the `+` menu's list (`startable.ts`): Terminal, Pi, Claude Code, the Launcher's list, Film…
// While the option names nothing installed, the menu shows what new tabs open then (the Launcher's
// list, else a shell, else the first); with nothing installed there is nothing to pick, and the
// row offers plugins.
import { useSyncExternalStore } from "react"
import { Button, OptionRow, SelectField, type SelectOption } from "../../sdk/settings/index.ts"
import * as plugins from "../plugins.ts"
import { newTabOf, settable, startables, type From } from "../startable.ts"

/** Its title, and its plugin's name when they differ (`Claude Code · Agents`). */
const labelOf = (title: string, plugin: string) => (title === plugin ? title : `${title} · ${plugin}`)

/**
 * The menu's choices and the one chosen, for `new_tab` being `current`. A block `new_tab` names
 * that a plugin declares but the list leaves out (an agent's bare terminal) opens, so it is shown
 * too; anything else not installed is what new tabs fall back on.
 */
export function newTabChoices(entries: Iterable<From>, current: string): { options: SelectOption[]; value: string } {
  const all = [...entries]
  const list = startables(all).filter(settable)
  const options: SelectOption[] = list.map((s) => ({ value: s.value, label: labelOf(s.title, s.pluginTitle) }))
  for (const e of all) {
    const block = (e.manifest.contributes?.blocks ?? []).find((b) => `${e.prefix}${b.id}` === current)
    if (block && !list.some((s) => s.value === current || s.alias === current)) {
      options.push({ value: current, label: labelOf(block.title ?? block.id, e.title ?? e.manifest.name ?? e.id) })
      return { options, value: current }
    }
  }
  return { options, value: newTabOf(list, current)?.value ?? "" }
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
        const { options, value: chosen } = newTabChoices(plugins.entries.values(), typeof value === "string" ? value : "")
        if (!options.length) {
          return (
            <Button primary onClick={() => open("store")}>
              Get plugins
            </Button>
          )
        }
        return <SelectField options={options} value={chosen} onChange={(picked) => set(picked)} width={240} testId="select-new_tab" />
      }}
    />
  )
}
