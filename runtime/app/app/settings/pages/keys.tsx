// Settings → Keybindings: the key options, then every command with its keys, to add one (a chord
// recorded), take one away, or give a command its default keys back.
import { useEffect, useMemo, useState, useSyncExternalStore } from "react"
import { keyGlyphs } from "../../../sdk/actions.ts"
import { Button, KeyChip, KeyRecorder, Note, OptionRow, Page, Row, Section, TextField, useSettings, useTextSize } from "../../../sdk/settings/index.ts"
import { useMetrics, useTheme } from "../../../sdk/theme.ts"
import * as commands from "../../commands.ts"
import { useGroup } from "../shared.tsx"

export interface KeyRow {
  name: string
  label: string
  group: string
  range: boolean
  keys: string[]
  defaults: string[]
  conflicts: Record<string, string[]>
}

let rowsVersion = 0
commands.subscribe(() => rowsVersion++)

/** Every command and its keys, as the keymap has them now. */
export function useKeyRows(): KeyRow[] {
  const version = useSyncExternalStore(
    (l) => commands.subscribe(l),
    () => rowsVersion,
  )
  return useMemo(() => commands.currentKeymap().rows(), [version])
}

export const FILTERS = [
  ["all", "All"],
  ["assigned", "Assigned"],
  ["custom", "Assigned by me"],
  ["unassigned", "Unassigned"],
] as const
export type Filter = (typeof FILTERS)[number][0]

/** The commands `filter` and `query` (every word in its group, label, name or keys) leave. */
export function filterRows(rows: KeyRow[], filter: Filter, query: string): KeyRow[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  return rows.filter((r) => {
    const custom = JSON.stringify(r.keys) !== JSON.stringify(r.defaults)
    const shown = filter === "all" || (filter === "assigned" && r.keys.length > 0) || (filter === "custom" && custom) || (filter === "unassigned" && !r.keys.length)
    if (!shown) return false
    const text = [r.group, r.label, r.name, ...r.keys, ...r.keys.map(keyGlyphs)].join(" ").toLowerCase()
    return words.every((w) => text.includes(w))
  })
}

/** A recorded chord as the command keeps it: a range's digit becomes its range (`cmd+1..9`). */
export function recorded(row: KeyRow, keys: string): string {
  return row.range && /\d$/.test(keys) ? `${keys.slice(0, -1)}1..9` : keys
}

/** `keymap_bindings` with `row` given `keys`: none of its own when they are its defaults. */
export function withKeys(bindings: Record<string, string[]>, row: KeyRow, keys: string[]): Record<string, string[]> {
  const next = { ...bindings }
  if (JSON.stringify(keys) === JSON.stringify(row.defaults)) delete next[row.name]
  else next[row.name] = keys
  return next
}

function Pill({ label, on, onClick }: { label: string; on: boolean; onClick(): void }) {
  const t = useTheme()
  const ts = useTextSize()
  return (
    <div role="tab" aria-selected={on} tabIndex={0} onClick={onClick} style={{ display: "flex", alignItems: "center", height: 24, paddingLeft: 10, paddingRight: 10, borderRadius: 12, backgroundColor: on ? t.selected : "transparent", cursor: "pointer", hover: on ? {} : { backgroundColor: t.hover } }}>
      <text style={{ fontSize: ts(12), color: on ? t.text : t.muted }}>{label}</text>
    </div>
  )
}

function KeyControl({ row, recording, record, set }: { row: KeyRow; recording: boolean; record(on: boolean): void; set(keys: string[]): void }) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const custom = JSON.stringify(row.keys) !== JSON.stringify(row.defaults)
  return (
    <>
      {row.keys.map((k) => (
        <KeyChip key={k} keys={k} conflicts={row.conflicts[k]} onRemove={() => set(row.keys.filter((x) => x !== k))} />
      ))}
      {!row.keys.length && !recording ? (
        <div style={{ display: "flex", alignItems: "center", height: 22, paddingLeft: 8, paddingRight: 8, borderRadius: m.radiusSmall, backgroundColor: t.editor }}>
          <text style={{ fontSize: ts(12), color: t.muted }}>Blank</text>
        </div>
      ) : null}
      {recording ? (
        <KeyRecorder
          testId={`record-${row.name}`}
          onCancel={() => record(false)}
          onRecord={(keys) => {
            record(false)
            const k = recorded(row, keys)
            if (!row.keys.includes(k)) set([...row.keys, k])
          }}
        />
      ) : null}
      {custom ? <Button ghost icon="rotate_ccw" title="Restore default" onClick={() => set(row.defaults)} /> : null}
      <Button ghost icon="plus" title="Add a key" testId={`add-key-${row.name}`} onClick={() => record(!recording)} />
    </>
  )
}

export function KeysPage() {
  const settings = useSettings()!
  const rows = useKeyRows()
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<Filter>("all")
  const [recording, setRecording] = useState<string>()
  // A link to a command (a search hit) shows it alone.
  const command = settings.highlight?.startsWith("command:") ? settings.highlight.slice("command:".length) : undefined
  useEffect(() => {
    const row = command && rows.find((r) => r.name === command)
    if (row) {
      setQuery(row.label)
      setFilter("all")
    }
  }, [command])
  const options = useGroup("Keys", ["keymap_bindings"])
  const shown = filterRows(rows, filter, query)
  const groups: { group: string; rows: KeyRow[] }[] = []
  for (const r of shown) {
    const last = groups.at(-1)
    if (last?.group === r.group) last.rows.push(r)
    else groups.push({ group: r.group, rows: [r] })
  }
  const set = (row: KeyRow, keys: string[]) => {
    const bindings = (settings.value("keymap_bindings") ?? {}) as Record<string, string[]>
    settings.set("keymap_bindings", withKeys(typeof bindings === "object" && !Array.isArray(bindings) ? bindings : {}, row, keys))
  }
  return (
    <Page title="Keybindings">
      {options.length ? (
        <Section>
          {options.map((d) => (
            <OptionRow key={d.name} name={d.name} />
          ))}
        </Section>
      ) : null}
      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <TextField search value={query} width="100%" placeholder="Filter commands or keys…" onChange={setQuery} onCommit={setQuery} testId="keys-filter" />
        <div style={{ display: "flex", flexDirection: "row", gap: 6 }}>
          {FILTERS.map(([id, label]) => (
            <Pill key={id} label={label} on={filter === id} onClick={() => setFilter(id)} />
          ))}
        </div>
      </div>
      {groups.map((g) => (
        <Section key={g.group} title={g.group}>
          {g.rows.map((r) => (
            <Row key={r.name} link={`command:${r.name}`} title={r.label} control={<KeyControl row={r} recording={recording === r.name} record={(on) => setRecording(on ? r.name : undefined)} set={(keys) => set(r, keys)} />} />
          ))}
        </Section>
      ))}
      {!shown.length ? <Note>No commands match.</Note> : null}
    </Page>
  )
}
