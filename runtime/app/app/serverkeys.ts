// The server's key tables, as the window sees them: the global `keys` option (a table, then a key,
// then `{call, args}`), which keys they bind so the window takes them before the terminal does, and
// what a key pressed at a tab becomes. A bound key is the call the table gives it; a key bound
// nowhere is the tab's, sent there in the server's notation (`tab.call`'s `input {keys}`).
import { call, engine, store, via } from "./engine.ts"

const NAMES: [string, string][] = [
  ["enter", "Enter"], ["escape", "Escape"], ["tab", "Tab"], ["backspace", "BSpace"], ["space", "Space"],
  ["delete", "DC"], ["insert", "IC"], ["home", "Home"], ["end", "End"], ["pageup", "PPage"], ["pagedown", "NPage"],
  ["up", "Up"], ["down", "Down"], ["left", "Left"], ["right", "Right"], ["minus", "-"], ["equal", "="],
  ["plus", "+"], ["slash", "/"],
]

/** A key in the server's notation (`M-Left`) as a chord (`alt+left`). */
export function fromServer(key: string): string | undefined {
  let rest = key.replace(/^<|>$/g, "")
  const mods = new Set<string>()
  while (rest.length > 2 && rest[1] === "-") {
    const m = ({ C: "ctrl", M: "alt", A: "alt", S: "shift", D: "cmd" } as Record<string, string>)[rest[0]!.toUpperCase()]
    if (!m) break
    mods.add(m)
    rest = rest.slice(2)
  }
  let name = NAMES.find(([, t]) => t.toLowerCase() === rest.toLowerCase())?.[0] ?? rest
  if (name.length === 1 && /[A-Z]/.test(name)) {
    mods.add("shift")
    name = name.toLowerCase()
  } else name = name.toLowerCase()
  const order = ["ctrl", "alt", "shift", "cmd"].filter((m) => mods.has(m))
  return order.length ? `${order.join("+")}+${name}` : name
}

/** What a key's table binds it to: a function, and its arguments. */
export interface Binding {
  call?: string
  args?: unknown
}

/** A table's keys, by their chords (`ctrl+b c` is a two-key sequence). */
type Tables = Record<string, Record<string, Binding>>

/** The tables the window reads: the server's own and a terminal's. */
const TABLES = ["root", "global", "terminal"]

let tables: Tables = {}
let bound = new Set<string>()
let prefixes = new Set<string>()
/** The table the keys are in: `root`, or one a prefix put the window in (`prefix`). */
let table = "root"
/** The chords of a sequence begun and not finished (`ctrl+b c`). */
let sequence: string[] = []

/** Follows the server's tables (`keys`) and its `prefix` options. */
export function follow(options: Record<string, unknown>) {
  const keys = options.keys
  const next: Tables = {}
  if (keys && typeof keys === "object" && !Array.isArray(keys)) {
    for (const [name, entries] of Object.entries(keys as Record<string, unknown>)) {
      if (!entries || typeof entries !== "object" || Array.isArray(entries)) continue
      const byChord: Record<string, Binding> = {}
      for (const [key, binding] of Object.entries(entries as Record<string, unknown>)) {
        const chord = key.split(" ").map(fromServer).join(" ")
        byChord[chord] = (binding ?? {}) as Binding
      }
      next[name] = byChord
    }
  }
  tables = next
  const firsts = new Set<string>()
  for (const name of TABLES) {
    for (const chord of Object.keys(tables[name] ?? {})) {
      const first = chord.split(" ")[0]
      if (first) firsts.add(first)
    }
  }
  bound = firsts
  prefixes = new Set(
    [options.prefix, options.prefix2].filter((p): p is string => typeof p === "string" && p !== "None").map(fromServer).filter((c): c is string => !!c),
  )
}

/** The chords the window takes for the server. */
export function captured(): string[] {
  return [...new Set([...bound, ...prefixes])]
}

/** Whether the server has keys of a sequence and waits for more. */
export function pending(): boolean {
  return sequence.length > 0 || table !== "root"
}

/** Whether the server's tables bind `chord` (or wait for more). */
export function takes(chord: string): boolean {
  return pending() || bound.has(chord) || prefixes.has(chord)
}

/** The binding for `joined` (`ctrl+b` or `ctrl+b c`) in the tables the window reads. */
function lookup(joined: string): Binding | undefined {
  for (const name of TABLES) {
    const found = tables[name]?.[joined]
    if (found) return found
  }
  return undefined
}

/** Whether `joined` begins a longer binding: the server waits for the keys after it. */
function begins(joined: string): boolean {
  const start = `${joined} `
  return TABLES.some((name) => Object.keys(tables[name] ?? {}).some((chord) => chord.startsWith(start)))
}

/** Runs `binding` at `tab`: the function it names, as a caller with that tab. */
function run(binding: Binding, tab: string | undefined): Promise<unknown> | undefined {
  const name = binding.call
  if (!name) return undefined
  const args = binding.args === undefined ? [] : (Array.isArray(binding.args) ? binding.args : [binding.args])
  return Promise.resolve(engine.call(via(tab, name), args, tab ? { tab } : undefined)).catch((err) =>
    console.error(`pumi: ${name}:`, (err as Error)?.message ?? err),
  )
}

/** Hands `chord`, pressed at `tab`, to the server's key tables. */
export async function press(chord: string, tab: string | undefined): Promise<void> {
  const where = table
  if (where !== "root") {
    // A key in a table the prefix put the window in: what it binds, and out of the table.
    table = "root"
    sequence = []
    const found = tables[where]?.[chord]
    if (found) await run(found, tab)
    return
  }
  const seq = [...sequence, chord]
  const joined = seq.join(" ")
  const found = lookup(joined)
  if (found) {
    sequence = []
    await run(found, tab)
    return
  }
  if (begins(joined)) {
    sequence = seq
    return
  }
  if (seq.length === 1 && prefixes.has(chord)) {
    table = "prefix"
    return
  }
  sequence = []
  // Bound nowhere: the key is the tab's.
  const block = store.get().snapshot?.tabs.find((t) => t.tab_id === tab)?.block
  if (tab && (!block || block === "terminal")) await call(via(tab, "tab.call"), tab, "input", { keys: seq })
}
