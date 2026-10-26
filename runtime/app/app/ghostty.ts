// The terminals' settings as a Ghostty config (https://ghostty.org/docs/config): `config.ghostty`
// in pumi's config folder, in Ghostty's own syntax, so a Ghostty user's lines work as they are.
// The keys pumi knows become its options (`font-size` sets `guifont`'s size, `cursor-style`
// `terminal_cursor_style`); the rest stay in the file, unused. Settings → Appearance writes the
// same file, and the file is followed as it changes, so an edit in an editor shows at once.
import { existsSync, mkdirSync, readFileSync, renameSync, statSync, writeFileSync } from "node:fs"
import { homedir } from "node:os"
import { dirname, isAbsolute, join, resolve } from "node:path"
import { Engine } from "../native/addon.js"

/** The file, and who sets the options it gives (as the server records it). */
export const GHOSTTY_FILE = join(Engine.stdpath("config") ?? ".", "config.ghostty")
export const GHOSTTY_SOURCE = "config.ghostty"

const HEADER = `# pumi's terminal settings, in Ghostty's config syntax: https://ghostty.org/docs/config
# Settings → Appearance writes here too. A key pumi doesn't use yet is kept and left alone.
`

type Value = string | number | boolean | null
type Options = Record<string, unknown>
export interface Entry {
  key: string
  value: string
}

/** `key = value` lines, comments and blanks left out; a quoted value is unquoted. */
export function parse(text: string): Entry[] {
  const out: Entry[] = []
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim()
    if (!line || line.startsWith("#")) continue
    const eq = line.indexOf("=")
    if (eq < 0) continue
    const key = line.slice(0, eq).trim()
    let value = line.slice(eq + 1).trim()
    if (value.length >= 2 && value.startsWith('"') && value.endsWith('"')) value = value.slice(1, -1)
    if (key) out.push({ key, value })
  }
  return out
}

const bool = (v: string): boolean | undefined => (v === "true" ? true : v === "false" ? false : undefined)
const num = (v: string): number | undefined => {
  const n = Number.parseFloat(v)
  return Number.isFinite(n) ? n : undefined
}
const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n))
const step = (n: number, by: number) => Math.round(n / by) * by
/** `#rrggbb` (Ghostty takes it with or without the `#`). */
const hex = (v: string): string | undefined => {
  const m = /^#?([0-9a-f]{6})$/i.exec(v.trim())
  return m ? `#${m[1]!.toLowerCase()}` : undefined
}
/** A cell adjustment, `10%` or pixels (`2`), as a percent of `of` pixels. */
const percent = (v: string, of: number): number | undefined => {
  const n = num(v)
  if (n === undefined) return undefined
  return v.trim().endsWith("%") ? n : (n / of) * 100
}
const STYLES = ["thin", "light", "regular", "medium", "semibold", "bold"]
const styleOf = (v: string): string | undefined => {
  const s = v.toLowerCase().replace(/[\s_-]/g, "")
  if (s === "default" || s === "normal" || s === "book" || s === "") return "regular"
  if (s === "demibold") return "semibold"
  return STYLES.includes(s) ? s : undefined
}

/** One key ↔ one option: how the key's value reads as the option's, and the other way. */
interface Field {
  key: string
  option: string
  read(value: string, all: Entry[]): Value | undefined
  write(value: Value): string | undefined
}
const flag = (key: string, option: string): Field => ({ key, option, read: bool, write: (v) => (typeof v === "boolean" ? String(v) : undefined) })
const number = (key: string, option: string, lo: number, hi: number): Field => ({
  key,
  option,
  read: (v) => {
    const n = num(v)
    return n === undefined ? undefined : clamp(n, lo, hi)
  },
  write: (v) => (typeof v === "number" ? String(v) : undefined),
})
const color = (key: string, option: string): Field => ({ key, option, read: hex, write: (v) => (typeof v === "string" && hex(v) ? hex(v) : undefined) })
/** The size a cell adjustment in pixels is measured against: about a line of 12pt type. */
const fontSizeOf = (all: Entry[]) => num(all.findLast((e) => e.key === "font-size")?.value ?? "") ?? 12

const FIELDS: Field[] = [
  {
    key: "theme",
    option: "terminal_colorscheme",
    // `light:A,dark:B` names one for each; the terminals take the dark one.
    read: (v) => {
      const pairs = v.split(",").map((p) => p.trim())
      const dark = pairs.find((p) => p.startsWith("dark:"))
      return (dark ? dark.slice(5) : pairs[0]!.replace(/^light:/, "")).trim()
    },
    write: (v) => (typeof v === "string" && v ? v : undefined),
  },
  flag("font-thicken", "terminal_font_smoothing"),
  {
    key: "font-style",
    option: "terminal_font_style",
    read: styleOf,
    write: (v) => (typeof v === "string" ? (v === "regular" ? "default" : v === "semibold" ? "semi-bold" : v === "extralight" ? "extra-light" : v === "extrabold" ? "extra-bold" : v) : undefined),
  },
  {
    key: "adjust-cell-height",
    option: "linespace",
    read: (v, all) => {
      const p = percent(v, fontSizeOf(all) * 1.2)
      return p === undefined ? undefined : clamp(step(p, 5), -50, 100)
    },
    write: (v) => (typeof v === "number" ? `${v}%` : undefined),
  },
  number("minimum-contrast", "terminal_minimum_contrast", 1, 21),
  color("background", "terminal_background"),
  color("foreground", "terminal_foreground"),
  color("cursor-color", "terminal_cursor_color"),
  color("selection-background", "terminal_selection_background"),
  flag("palette-generate", "terminal_palette_generate"),
  flag("palette-harmonious", "terminal_palette_harmonious"),
  {
    key: "background-opacity",
    option: "ui_background_opacity",
    read: (v) => {
      const n = num(v)
      return n === undefined ? undefined : clamp(step(n * 100, 5), 20, 100)
    },
    write: (v) => (typeof v === "number" ? String(v / 100) : undefined),
  },
  {
    key: "background-blur",
    option: "ui_background_blur",
    read: (v) => {
      const b = bool(v)
      if (b !== undefined) return b ? 20 : 0
      const n = num(v)
      return n === undefined ? undefined : clamp(step(n, 2), 0, 60)
    },
    write: (v) => (typeof v === "number" ? String(v) : undefined),
  },
  {
    key: "cursor-style",
    option: "terminal_cursor_style",
    read: (v) => (["block", "bar", "underline", "block_hollow"].includes(v) ? v : undefined),
    write: (v) => (typeof v === "string" ? v : undefined),
  },
  {
    key: "cursor-style-blink",
    option: "terminal_cursor_blink",
    read: (v) => (v === "true" ? "on" : v === "false" ? "off" : undefined),
    write: (v) => (v === "on" ? "true" : v === "off" ? "false" : undefined),
  },
  {
    key: "window-padding-x",
    option: "terminal_padding_x",
    read: (v) => {
      const n = num(v.split(",")[0]!)
      return n === undefined ? undefined : clamp(Math.round(n), 0, 40)
    },
    write: (v) => (typeof v === "number" ? String(v) : undefined),
  },
  {
    key: "window-padding-y",
    option: "terminal_padding_y",
    read: (v) => {
      const n = num(v.split(",")[0]!)
      return n === undefined ? undefined : clamp(Math.round(n), 0, 40)
    },
    write: (v) => (typeof v === "number" ? String(v) : undefined),
  },
  {
    key: "window-padding-color",
    option: "terminal_padding_color",
    // `extend-always` is `extend` without Ghostty's exceptions; pumi has only the one.
    read: (v) => (v === "background" ? "background" : v === "extend" || v === "extend-always" ? "extend" : undefined),
    write: (v) => (v === "background" || v === "extend" ? v : undefined),
  },
  {
    key: "copy-on-select",
    option: "terminal_copy_on_select",
    read: (v) => (v === "false" ? false : v === "true" || v === "clipboard" ? true : undefined),
    write: (v) => (typeof v === "boolean" ? (v ? "clipboard" : "false") : undefined),
  },
  {
    key: "macos-option-as-alt",
    option: "macos_option_as_alt",
    read: (v) => (v === "false" ? false : ["true", "left", "right"].includes(v) ? true : undefined),
    write: (v) => (typeof v === "boolean" ? String(v) : undefined),
  },
]

/** The keys made of more than one option, or one option of more than one key. */
const FONT_KEYS = ["font-family", "font-size", "font-feature", "palette"]
const LIGATURES = ["calt", "liga", "dlig"]

/** Every option the file sets, and pumi's own options it owns. */
export const GHOSTTY_OPTIONS = new Set([
  ...FIELDS.map((f) => f.option),
  "guifont",
  "terminal_font_fallbacks",
  "terminal_ligatures",
  ...Array.from({ length: 16 }, (_, i) => `terminal_color_${i}`),
])
export const isGhosttyOption = (name: string) => GHOSTTY_OPTIONS.has(name)

/** What the file's lines set: each option the keys pumi knows give, and the keys it doesn't. */
export function toOptions(entries: Entry[]): { options: Record<string, Value>; unused: string[] } {
  const options: Record<string, Value> = {}
  const unused = new Set<string>()
  // A later line wins, as in Ghostty; an empty `font-family` starts the list again.
  let families: string[] = []
  let size: number | undefined
  let ligatures: boolean | undefined
  for (const e of entries) {
    if (e.key === "font-family") {
      families = e.value ? [...families, e.value] : []
      continue
    }
    if (e.key === "font-size") {
      size = num(e.value) ?? size
      continue
    }
    if (e.key === "font-feature") {
      const off = /^-(\w+)$/.exec(e.value) ?? /^(\w+)\s*=\s*0$/.exec(e.value)
      const on = /^\+?(\w+)$/.exec(e.value) ?? /^(\w+)\s*=\s*1$/.exec(e.value)
      if (off && LIGATURES.includes(off[1]!)) ligatures = false
      else if (on && LIGATURES.includes(on[1]!)) ligatures = true
      continue
    }
    if (e.key === "palette") {
      const m = /^(\d+)\s*=\s*(.+)$/.exec(e.value)
      const index = m ? Number(m[1]) : NaN
      const c = m ? hex(m[2]!) : undefined
      if (index >= 0 && index < 16 && c) options[`terminal_color_${index}`] = c
      continue
    }
    const field = FIELDS.find((f) => f.key === e.key)
    if (!field) {
      unused.add(e.key)
      continue
    }
    const value = field.read(e.value, entries)
    if (value !== undefined) options[field.option] = value
  }
  if (families.length || size !== undefined) options.guifont = `${(families[0] ?? "").replace(/ /g, "_")}${size === undefined ? "" : `:h${size}`}`
  if (families.length > 1) options.terminal_font_fallbacks = families.slice(1).join(", ")
  if (ligatures !== undefined) options.terminal_ligatures = ligatures
  return { options, unused: [...unused] }
}

/** `text` with every line of `key` taken out and `values` (one line each) put where the first was, or at the end. */
export function setKey(text: string, key: string, values: string[]): string {
  const lines = text.split("\n")
  const keyOf = (line: string) => {
    const t = line.trim()
    if (!t || t.startsWith("#") || !t.includes("=")) return undefined
    return t.slice(0, t.indexOf("=")).trim()
  }
  const at = lines.findIndex((l) => keyOf(l) === key)
  const kept = lines.filter((l) => keyOf(l) !== key)
  const fresh = values.map((v) => `${key} = ${v}`)
  // Nothing before the first of its lines is taken out, so it is at the same index in `kept`.
  if (at >= 0) {
    kept.splice(at, 0, ...fresh)
    return kept.join("\n")
  }
  if (!fresh.length) return kept.join("\n")
  const body = kept.join("\n")
  return `${body}${body === "" || body.endsWith("\n") ? "" : "\n"}${fresh.join("\n")}\n`
}

/** `text` with option `name` set to `value` (`null` takes its lines out), the other options as `current` has them. */
export function writeOption(text: string, name: string, value: Value, current: Options): string {
  const field = FIELDS.find((f) => f.option === name)
  if (field) {
    const line = value === null ? undefined : field.write(value)
    return setKey(text, field.key, line === undefined ? [] : [line])
  }
  const at = /^terminal_color_(\d+)$/.exec(name)
  if (at) {
    const index = Number(at[1])
    const others = parse(text)
      .filter((e) => e.key === "palette" && !new RegExp(`^${index}\\s*=`).test(e.value))
      .map((e) => e.value)
    const c = typeof value === "string" ? hex(value) : undefined
    return setKey(text, "palette", c ? [...others, `${index}=${c}`] : others)
  }
  if (name === "guifont" || name === "terminal_font_fallbacks") {
    const guifont = String((name === "guifont" ? value : current.guifont) ?? "")
    const fallbacks = String((name === "terminal_font_fallbacks" ? value : current.terminal_font_fallbacks) ?? "")
    const [family = "", ...rest] = guifont.split(":")
    const sized = rest.map((p) => (p.startsWith("h") ? Number.parseFloat(p.slice(1)) : NaN)).find(Number.isFinite)
    const families = [family.replace(/_/g, " ").trim(), ...fallbacks.split(",").map((f) => f.trim())].filter(Boolean)
    let out = setKey(text, "font-family", families.map((f) => (/[\s#]/.test(f) ? `"${f}"` : f)))
    if (name === "guifont") out = setKey(out, "font-size", sized === undefined ? [] : [String(sized)])
    return out
  }
  if (name === "terminal_ligatures") {
    const others = parse(text)
      .filter((e) => e.key === "font-feature" && !LIGATURES.some((l) => e.value.replace(/^[-+]/, "").startsWith(l)))
      .map((e) => e.value)
    return setKey(text, "font-feature", value === false ? [...others, ...LIGATURES.map((l) => `-${l}`)] : others)
  }
  return text
}

// ——— The file ———

function read(file = GHOSTTY_FILE): string {
  try {
    return readFileSync(file, "utf8")
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return ""
    throw new Error(`${file}: ${(err as Error).message}`)
  }
}
function write(text: string, file = GHOSTTY_FILE) {
  mkdirSync(dirname(file), { recursive: true })
  const tmp = `${file}.tmp`
  writeFileSync(tmp, text)
  renameSync(tmp, file)
}

/** Keeps option `name` at `value` in the file (`null` takes it out). */
export function keepInGhostty(name: string, value: Value, current: Options, file = GHOSTTY_FILE): void {
  const text = read(file) || HEADER
  const next = writeOption(text, name, value, current)
  if (next !== text) write(next, file)
  applied.set(name, JSON.stringify(value))
}

/** What the file gives, and the keys in it pumi doesn't use. */
export function inspect(file = GHOSTTY_FILE): { options: Record<string, Value>; unused: string[]; exists: boolean } {
  return { ...toOptions(parse(read(file))), exists: existsSync(file) }
}

/** The file, made with its header when there is none yet. */
export function ensureFile(file = GHOSTTY_FILE): string {
  if (!existsSync(file)) write(HEADER, file)
  return file
}

/** Ghostty's own config files, in the order it reads them, with any `config-file` they include. */
export function ghosttyConfigFiles(home = homedir(), env = process.env): string[] {
  const xdg = env.XDG_CONFIG_HOME || join(home, ".config")
  const dirs = [join(xdg, "ghostty")]
  if (process.platform === "darwin") dirs.push(join(home, "Library", "Application Support", "com.mitchellh.ghostty"))
  return dirs.flatMap((d) => ["config", "config.ghostty"].map((f) => join(d, f))).filter((f) => existsSync(f))
}

/** A Ghostty config's text with the files it includes (`config-file = …`) put in their place. */
export function inlineIncludes(file: string, depth = 0): string {
  const text = read(file)
  if (depth > 4) return text
  return text
    .split("\n")
    .map((line) => {
      const m = /^\s*config-file\s*=\s*"?(\??)(.*?)"?\s*$/.exec(line)
      if (!m) return line
      const path = m[2]!.replace(/^~(?=\/)/, homedir())
      const full = isAbsolute(path) ? path : resolve(dirname(file), path)
      if (!existsSync(full)) return m[1] ? `# ${line.trim()} (not found)` : line
      return `# ${line.trim()}\n${inlineIncludes(full, depth + 1)}`
    })
    .join("\n")
}

/**
 * Takes Ghostty's config as pumi's: its files, included ones inlined, written to `config.ghostty`
 * (what was there before kept beside it, `.bak`). Says what it read and how many keys pumi uses.
 */
export function importFromGhostty(file = GHOSTTY_FILE, from = ghosttyConfigFiles()): { from: string[]; used: number; unused: string[] } {
  if (!from.length) throw new Error("No Ghostty config found (~/.config/ghostty/config, or Ghostty's Application Support folder).")
  const parts = from.map((f) => `# ——— from ${f} ———\n${inlineIncludes(f).trimEnd()}\n`)
  const before = read(file)
  if (before.trim()) write(before, `${file}.bak`)
  write(`${HEADER}# Imported from Ghostty on ${new Date().toISOString().slice(0, 10)}.\n\n${parts.join("\n")}`, file)
  const { options, unused } = toOptions(parse(read(file)))
  return { from, used: Object.keys(options).length, unused }
}

// ——— Following it ———

/** What the file last set, by option (its value as JSON), so a line taken out sets it back. */
const applied = new Map<string, string>()
type Setter = (name: string, value: Value) => Promise<unknown>
let watching: ReturnType<typeof setInterval> | undefined

/**
 * Sets the options the file gives that differ from `current` (`all`: every one, as the app starts,
 * so the server records them as the file's), and resets those it gave before and no longer does.
 */
export async function apply(set: Setter, current: Options, file = GHOSTTY_FILE, all = false): Promise<void> {
  let options: Record<string, Value>
  try {
    options = toOptions(parse(read(file))).options
  } catch (err) {
    console.error(`pumi: ${(err as Error).message}`)
    return
  }
  const jobs: Promise<unknown>[] = []
  for (const [name, value] of Object.entries(options)) {
    const json = JSON.stringify(value)
    if (all || JSON.stringify(current[name] ?? null) !== json) jobs.push(set(name, value))
    applied.set(name, json)
  }
  for (const [name, json] of [...applied]) {
    if (name in options) continue
    applied.delete(name)
    if (JSON.stringify(current[name] ?? null) === json) jobs.push(set(name, null))
  }
  await Promise.allSettled(jobs)
}

/**
 * The options settings.json kept that the file owns now, moved into it (once): Settings wrote
 * them there before. A `linespace` of the old kind (a multiple of the size) becomes a percent.
 */
export function migrate(settingsFile: string, current: Options, file = GHOSTTY_FILE): string[] {
  let json: { options?: Record<string, unknown> }
  try {
    json = JSON.parse(readFileSync(settingsFile, "utf8") || "{}")
  } catch {
    return []
  }
  const kept = json.options && typeof json.options === "object" ? json.options : {}
  const moving = Object.keys(kept).filter(isGhosttyOption)
  if (!moving.length) return []
  const has = toOptions(parse(read(file))).options
  let text = read(file) || HEADER
  const values = { ...current, ...kept }
  for (const name of moving) {
    if (name in has) continue
    let value = kept[name] as Value
    if (name === "linespace" && typeof value === "number" && value >= 1 && value <= 2) value = clamp(step((value / 1.2 - 1) * 100, 5), -50, 100)
    text = writeOption(text, name, value, values)
  }
  write(text, file)
  for (const name of moving) delete kept[name]
  json.options = kept
  const tmp = `${settingsFile}.tmp`
  writeFileSync(tmp, `${JSON.stringify(json, null, 2)}\n`)
  renameSync(tmp, settingsFile)
  return moving
}

/** Applies the file now and each time it changes (checked every second), until `stop`. */
export function follow(set: Setter, current: () => Options, settingsFile?: string, file = GHOSTTY_FILE): () => void {
  if (settingsFile) {
    try {
      migrate(settingsFile, current(), file)
    } catch (err) {
      console.error("pumi: moving the terminal settings to config.ghostty:", (err as Error).message)
    }
  }
  const mtime = () => {
    try {
      return statSync(file).mtimeMs
    } catch {
      return 0
    }
  }
  let seen = mtime()
  void apply(set, current(), file, true)
  clearInterval(watching)
  watching = setInterval(() => {
    const now = mtime()
    if (now === seen) return
    seen = now
    void apply(set, current(), file)
  }, 1000)
  return () => clearInterval(watching)
}
