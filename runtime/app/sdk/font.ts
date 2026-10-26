// The text font the terminals are drawn in, for anything else that shows code (an editor, a log):
// the global options `guifont` (Neovim's `Family_Name:h12`) and `linespace` (the percent a line is
// taller than the font's own, Ghostty's `adjust-cell-height`), read as the app's terminals read
// them (`src/settings.rs`).
import { useApp } from "../app/state.ts"

/** A text font: its family, its size in points, and its line height as a multiple of the size. */
export interface Font {
  family: string
  size: number
  lineHeight: number
}

/** The monospace font each OS has: the one an unset `guifont` is drawn in. */
const MONO = process.platform === "darwin" ? "Menlo" : process.platform === "win32" ? "Consolas" : "DejaVu Sans Mono"

/** What an unset `guifont` is, and the ranges Settings keeps `guifont`'s size and `linespace` in. */
export const FONT_DEFAULT: Font = { family: MONO, size: 12, lineHeight: 1.2 }
export const FONT_SIZE_RANGE = [8, 32] as const
export const LINE_HEIGHT_ADJUST_RANGE = [-50, 100] as const
/** About how tall a monospace face's own line is, as a multiple of its size: the terminals measure
 *  the font's, which JavaScript can't. */
const NATURAL_LINE = 1.2

function clamp(value: number, [lo, hi]: readonly [number, number], fallback: number): number {
  return Number.isFinite(value) ? Math.min(hi, Math.max(lo, value)) : fallback
}

/** `guifont` and `linespace` as a `Font`: an empty or unreadable part is the default's. */
export function parseFont(guifont: unknown, linespace: unknown): Font {
  const [name = "", ...rest] = typeof guifont === "string" ? guifont.split(":") : []
  const family = name.trim().replace(/_/g, " ") || FONT_DEFAULT.family
  let size = FONT_DEFAULT.size
  for (const part of rest) {
    const h = part.startsWith("h") ? Number.parseFloat(part.slice(1)) : Number.NaN
    if (Number.isFinite(h)) size = h
  }
  const adjust = typeof linespace === "number" ? clamp(linespace, LINE_HEIGHT_ADJUST_RANGE, 0) : 0
  return {
    family,
    size: clamp(size, FONT_SIZE_RANGE, FONT_DEFAULT.size),
    lineHeight: Math.round(NATURAL_LINE * (1 + adjust / 100) * 100) / 100,
  }
}

/** `Font` as `guifont` writes it. */
export function guifont(family: string, size: number): string {
  return `${family.replace(/ /g, "_")}:h${size}`
}

/** The terminals' font, following Settings' Font family, Font size and Line height. */
export function useFont(): Font {
  const guifontValue = useApp((s) => s.options.guifont)
  const linespace = useApp((s) => s.options.linespace)
  return parseFont(guifontValue, linespace)
}
