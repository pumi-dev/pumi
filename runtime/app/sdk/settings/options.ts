// What Settings knows of an option: its declaration (`registry.list("option")`), what it is
// called and grouped under, and how its value is typed into a field and shown again.

/** An option's value: what JSON holds (an option is kept in settings.json). */
export type OptionValue = string | number | boolean | null | OptionValue[] | { [key: string]: OptionValue }

/** An option's declaration, as the server lists it. */
export interface Decl {
  name: string
  type: "boolean" | "number" | "string" | "table" | "any" | string
  default?: OptionValue
  desc?: string
  choices?: OptionValue[] | null
  labels?: string[] | null
  readonly?: boolean
  plugin?: string | null
  label?: string | null
  group?: string | null
  min?: number | null
  max?: number | null
  step?: number | null
  unit?: string | null
  placeholder?: string | null
  /** `toggle`, `slider`, `json`, `color` (a `#rrggbb`), or a control Settings draws itself (`ui_theme`, `font`, …). */
  control?: string | null
  scope?: string | null
  blocks?: string[] | null
}

/** Its title: the declared label, else its name made readable (`files.hide_hidden` is `Hide hidden`). */
export function titleOf(d: Decl): string {
  if (d.label) return d.label
  const base = (d.name.split(".").pop() ?? d.name).replaceAll("_", " ")
  return base ? base[0]!.toUpperCase() + base.slice(1) : d.name
}

/** Its heading: the declared group, else its plugin, else `Server` (the server's own). */
export function groupOf(d: Decl): string {
  return d.group || d.plugin || "Server"
}

/** Its choices with what each is called. */
export function choicesOf(d: Decl): { value: string; label: string }[] {
  return (d.choices ?? []).map((c, i) => {
    const value = typeof c === "string" ? c : JSON.stringify(c)
    return { value, label: d.labels?.[i] ?? value }
  })
}

/** Whether Settings shows it: not read-only, and not one tab's own (a browser's `url`). */
export function settable(d: Decl): boolean {
  return !d.readonly && !(d.scope === "tab" && (d.blocks?.length ?? 0) > 0)
}

/** A value as a field shows it. */
export function textOf(value: OptionValue | undefined): string {
  if (typeof value === "string") return value
  if (value === null || value === undefined) return ""
  return JSON.stringify(value)
}

/** Text typed into a field, as the option's value; an error says why it is not one. */
export function parse(d: Decl, text: string): { value: OptionValue } | { error: string } {
  switch (d.type) {
    case "number": {
      const n = Number(text.trim())
      return text.trim() !== "" && Number.isFinite(n) ? { value: n } : { error: `${JSON.stringify(text)} is not a number` }
    }
    case "boolean":
      return { value: ["true", "on", "yes", "1"].includes(text.trim()) }
    case "table":
    case "any":
      if (!text.trim()) return { value: d.default ?? null }
      try {
        return { value: JSON.parse(text) }
      } catch (err) {
        return { error: `not JSON: ${(err as Error).message}` }
      }
    default:
      return { value: text }
  }
}

/** A number with as many decimals as its step has, and its unit (`0.30`, `20 px`, `100%`). */
export function formatNumber(value: number, step = 1, unit = ""): string {
  let decimals = 0
  let scaled = step
  while (Math.abs(scaled - Math.round(scaled)) > 1e-9 && decimals < 4) {
    scaled *= 10
    decimals++
  }
  const space = unit === "" || unit === "%" ? "" : " "
  return `${value.toFixed(decimals)}${space}${unit}`
}

/** `raw` on the grid of `step` from `min`, kept within the range. */
export function snap(raw: number, min: number, max: number, step = 1): number {
  const s = step > 0 ? step : 1
  const n = Math.round((raw - min) / s) * s + min
  return Math.round(Math.min(max, Math.max(min, n)) * 1e9) / 1e9
}
