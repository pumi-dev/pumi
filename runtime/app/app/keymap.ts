// The keymap: which chord runs which command, in which scope. A port of the Rust app's keymap.
//
// A command is bound by its name to key sequences: `cmd+t`, `ctrl+x ctrl+f`, or a range
// (`cmd+1..9`, its action gets the index). Bindings come from the defaults, the window's theme,
// the plugins' manifests, and the user (`keymap_bindings`), which replace a command's keys
// outright. A scope (`global`, or what the focused part of the window says it is) picks among
// bindings of the same keys; the most specific scope wins.

export type Scope = string

export interface Command {
  name: string
  label: string
  group: string
  /** `global`, or the scope it runs in. */
  scope: Scope
  /** A range command (`switch_tab`): its action carries the index. */
  range?: boolean
}

export interface Action {
  name: string
  index?: number
}

export type Resolved = { run: Action } | { pending: true; fallback?: Action } | { unbound: true }

const MODIFIER_RANK: Record<string, number> = { ctrl: 0, alt: 1, shift: 2, cmd: 3 }

const SHIFTED: [string, string][] = [
  ["[", "{"], ["]", "}"], [";", ":"], ["'", '"'], [",", "<"], [".", ">"], ["slash", "?"], ["\\", "|"],
  ["minus", "_"], ["equal", "plus"], ["`", "~"], ["1", "!"], ["2", "@"], ["3", "#"], ["4", "$"],
  ["5", "%"], ["6", "^"], ["7", "&"], ["8", "*"], ["9", "("], ["0", ")"],
]

export function normalizeKey(raw: string): string {
  switch (raw.trim().toLowerCase()) {
    case "cmd": case "command": case "super": case "meta": return "cmd"
    case "ctrl": case "control": return "ctrl"
    case "option": case "opt": return "alt"
    case "-": return "minus"
    case "=": return "equal"
    case "+": return "plus"
    case "/": return "slash"
    default: return raw.trim().toLowerCase()
  }
}

export function normalizeChord(raw: string): string | undefined {
  raw = raw.trim()
  if (!raw) return undefined
  const plusKey = raw.endsWith("++") || raw === "+"
  const body = raw.endsWith("++") ? raw.slice(0, -2) : raw
  const parts = body.split("+").filter(Boolean).map(normalizeKey)
  if (plusKey) parts.push("plus")
  let key = parts.pop()
  if (key === undefined) return undefined
  const shifted = SHIFTED.find(([base]) => base === key && base !== "`")?.[1]
  const at = parts.indexOf("shift")
  if (shifted && at >= 0) {
    parts.splice(at, 1)
    key = shifted
  }
  const mods = [...new Set(parts)].sort((a, b) => (MODIFIER_RANK[a] ?? 4) - (MODIFIER_RANK[b] ?? 4))
  return mods.length ? `${mods.join("+")}+${key}` : key
}

export function isRange(keys: string): boolean {
  return (keys.split(/[ +]/).at(-1) ?? "").includes("..")
}

export function normalizeKeys(raw: string): string | undefined {
  const strokes = raw.split(/\s+/).filter(Boolean)
  const last = strokes.pop()
  if (last === undefined) return undefined
  const out: string[] = []
  for (const stroke of strokes) {
    const chord = normalizeChord(stroke)
    if (!chord) return undefined
    out.push(chord)
  }
  if (isRange(last)) {
    const at = last.lastIndexOf("+")
    if (at < 0) out.push(last)
    else {
      const mods = normalizeChord(`${last.slice(0, at)}+x`)
      if (!mods) return undefined
      out.push(`${mods.slice(0, -1)}${last.slice(at + 1)}`)
    }
  } else {
    const chord = normalizeChord(last)
    if (!chord) return undefined
    out.push(chord)
  }
  return out.join(" ")
}

/** A binding's sequences, a range's one per index. */
export function expand(keys: string): [string, number | undefined][] {
  if (!isRange(keys)) return [[keys, undefined]]
  const space = keys.lastIndexOf(" ")
  const head = space >= 0 ? keys.slice(0, space) : undefined
  const last = space >= 0 ? keys.slice(space + 1) : keys
  const plus = last.lastIndexOf("+")
  const mods = plus >= 0 ? last.slice(0, plus) : undefined
  const range = plus >= 0 ? last.slice(plus + 1) : last
  const [from, to] = range.split("..").map((n) => Number.parseInt(n.trim(), 10))
  if (!Number.isFinite(from) || !Number.isFinite(to)) return []
  const out: [string, number][] = []
  for (let index = from!; index <= to!; index++) {
    const chord = normalizeChord(mods ? `${mods}+${index}` : String(index))
    if (chord) out.push([head ? `${head} ${chord}` : chord, index])
  }
  return out
}

function subtract(list: string[], keys: string): string[] {
  const removed = new Set(expand(keys).map(([sequence]) => sequence))
  const out: string[] = []
  for (const binding of list) {
    if (!isRange(binding)) {
      if (!removed.has(binding)) out.push(binding)
      continue
    }
    const kept = expand(binding).filter(([s]) => !removed.has(s)).map(([, i]) => i!)
    const at = binding.lastIndexOf("..")
    const stem = binding.slice(0, binding.slice(0, at).search(/\d+$/))
    const runs: [number, number][] = []
    for (const index of kept) {
      const last = runs.at(-1)
      if (last && last[1] + 1 === index) last[1] = index
      else runs.push([index, index])
    }
    out.push(...runs.map(([a, b]) => `${stem}${a}..${b}`))
  }
  return out
}

export interface KeymapInput {
  commands: Command[]
  /** `command=keys` pairs: the app's own, then the plugins'. */
  defaults: [string, string][]
  /**
   * What the window's theme says the keys are (`keymap_bindings` wins): a chord takes the
   * command it is given and leaves whichever command had it.
   */
  theme?: { keys: string; command: string }[]
  /** `keymap_bindings`: a command's keys, replacing what it had. */
  user?: Record<string, string[]>
}

export class Keymap {
  readonly commands: Command[]
  readonly defaults = new Map<string, string[]>()
  readonly bindings = new Map<string, string[]>()
  readonly custom = new Set<string>()
  private exact = new Map<string, Action>()
  private prefixes = new Set<string>()
  /** Every chord that starts a binding: what the window takes before the focused element. */
  readonly firsts = new Set<string>()

  constructor({ commands, defaults, theme = [], user = {} }: KeymapInput) {
    this.commands = commands
    const known = new Map(commands.map((c) => [c.name, c]))
    const table = new Map<string, string[]>(commands.map((c) => [c.name, []]))
    const bind = (command: string, raw: string) => {
      const list = table.get(command)
      const keys = normalizeKeys(raw)
      if (list && keys && !list.includes(keys)) list.push(keys)
    }
    for (const [command, keys] of defaults) bind(command, keys)
    for (const [name, list] of table) this.defaults.set(name, [...list])
    // The theme's: its chord goes to its command, and the command that had it loses it.
    for (const entry of theme) {
      const keys = normalizeKeys(entry.keys)
      if (!keys || !known.has(entry.command)) continue
      const scope = known.get(entry.command)!.scope
      for (const c of commands) {
        if (c.name === entry.command || c.scope === scope || c.scope === "global") table.set(c.name, subtract(table.get(c.name)!, keys))
      }
      bind(entry.command, keys)
    }
    for (const [command, chords] of Object.entries(user)) {
      if (!table.has(command)) continue
      table.set(command, [...new Set(chords.map(normalizeKeys).filter((k): k is string => !!k))])
      this.custom.add(command)
    }
    for (const [name, list] of table) this.bindings.set(name, list)
    // Custom bindings index last, and single chords after ranges, so they win a collision.
    const entries: [number, number, Command, string][] = []
    for (const command of commands) {
      for (const keys of table.get(command.name) ?? []) {
        entries.push([this.custom.has(command.name) ? 1 : 0, isRange(keys) ? 0 : 1, command, keys])
      }
    }
    entries.sort((a, b) => a[0] - b[0] || a[1] - b[1])
    for (const [, , command, keys] of entries) {
      for (const [sequence, index] of expand(keys)) {
        const strokes = sequence.split(" ")
        this.firsts.add(strokes[0]!)
        for (let end = 1; end < strokes.length; end++) this.prefixes.add(`${command.scope}\u0000${strokes.slice(0, end).join(" ")}`)
        this.exact.set(`${command.scope}\u0000${sequence}`, { name: command.name, index })
      }
    }
  }

  /** The commands that also answer `keys`, in `command`'s scope or the global one. */
  conflicts(command: string, keys: string): Command[] {
    const own = this.commands.find((c) => c.name === command)
    return this.commands.filter(
      (c) => c.name !== command && (c.scope === own?.scope || c.scope === "global" || own?.scope === "global") && this.keys(c.name).includes(keys),
    )
  }

  /** Every command's keys, for Settings → Keybindings. */
  rows() {
    return this.commands.map((c) => ({
      name: c.name,
      label: c.label,
      group: c.group,
      range: c.range === true,
      keys: this.keys(c.name),
      defaults: this.defaults.get(c.name) ?? [],
      conflicts: Object.fromEntries(this.keys(c.name).map((k) => [k, this.conflicts(c.name, k).map((o) => o.label)])),
    }))
  }

  keys(command: string): string[] {
    return this.bindings.get(command) ?? []
  }

  /** The chord to show for `command`: a single keystroke when it has one. Of a range command,
   *  the one its range gives `index` (`switch_tab`'s `cmd+1..9`, 3: `cmd+3`). */
  chordFor(command: string, index?: number): string | undefined {
    const fixed =
      index === undefined
        ? this.keys(command).filter((k) => !isRange(k))
        : this.keys(command).flatMap(expand).filter(([, at]) => at === index).map(([keys]) => keys)
    return fixed.find((k) => !k.includes(" ")) ?? fixed[0]
  }

  /** What `chord`, after the `pending` ones, does in `scopes` (most specific first). */
  resolve(scopes: Scope[], pending: string[], chord: string): Resolved {
    const sequence = [...pending, chord].join(" ")
    for (let at = 0; at < scopes.length; at++) {
      const key = `${scopes[at]}\u0000${sequence}`
      if (this.prefixes.has(key)) {
        for (const scope of scopes.slice(at)) {
          const fallback = this.exact.get(`${scope}\u0000${sequence}`)
          if (fallback) return { pending: true, fallback }
        }
        return { pending: true }
      }
      const action = this.exact.get(key)
      if (action) return { run: action }
    }
    return { unbound: true }
  }
}

/** A key event's chord, as bindings write it. */
export function chordOf(event: { key?: string | null; modifiers?: { ctrl: boolean; alt: boolean; shift: boolean; cmd: boolean } | null }): string {
  const m = event.modifiers ?? { ctrl: false, alt: false, shift: false, cmd: false }
  let key = normalizeKey(event.key ?? "")
  let shift = m.shift
  if (key === "~" && !shift) [shift, key] = [true, "`"]
  const parts = [m.ctrl && "ctrl", m.alt && "alt", shift && "shift", m.cmd && "cmd"].filter(Boolean)
  return parts.length ? `${parts.join("+")}+${key}` : key
}

/** `name` as a label: `toggle_sidebar` → `Toggle sidebar`. */
export function humanize(name: string): string {
  const words = name.split(/[_\- ]+|(?<=[a-z])(?=[A-Z])/).filter(Boolean)
  return words
    .map((word, index) => {
      if (word.length > 1 && word === word.toUpperCase()) return word
      return index === 0 ? word[0]!.toUpperCase() + word.slice(1).toLowerCase() : word.toLowerCase()
    })
    .join(" ")
}
