// The terminal colour schemes `terminal_colorscheme` picks from: every scheme in
// iTerm2-Color-Schemes (https://github.com/mbadolato/iTerm2-Color-Schemes, MIT, see ./LICENSE),
// taken from its `windowsterminal/` files at 2477e787 (2026-10-22). Settings lists them
// (`schemeNames`, Settings → Appearance). Each is [background, foreground, cursor, selection, ...16 ansi].
import type { Appearance, TerminalPalette, Tokens } from "../../../sdk/theme.ts"
import schemes from "./themes.json"

const SCHEMES = schemes as Record<string, string[]>

let names: string[] | undefined
/** Every scheme's name, in the order a person reads them. */
export function schemeNames(): string[] {
  return (names ??= Object.keys(SCHEMES).filter((n) => SCHEMES[n]!.length === 20).sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase())))
}

/** The scheme called `name`, as the terminals' palette; none when there is no such scheme. */
export function terminalScheme(name: string | undefined): TerminalPalette | undefined {
  const row = name ? SCHEMES[name] : undefined
  if (!row || row.length !== 20) return undefined
  const [background, foreground, cursor, selection, ...ansi] = row
  return { background: background!, foreground: foreground!, cursor: cursor!, selection: selection!, ansi }
}

/** A `#rrggbb` colour as its three channels, 0–255. */
function channels(hex: string): [number, number, number] {
  const n = Number.parseInt(hex.slice(1, 7), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}

/** `from` taken `by` (0–1) of the way to `to`. */
function mix(from: string, to: string, by: number): string {
  const [a, b] = [channels(from), channels(to)]
  return `#${a.map((c, i) => Math.round(c + (b[i]! - c) * by).toString(16).padStart(2, "0")).join("")}`
}

/** Relative luminance, 0 (black) to 1 (white). */
function luminance(hex: string): number {
  const [r, g, b] = channels(hex).map((c) => {
    const s = c / 255
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  })
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!
}

/** Dark or light, as a background is: dark below 0.4 luminance. */
export function appearanceOf(background: string): Appearance {
  return luminance(background) < 0.4 ? "dark" : "light"
}

/**
 * pumi's own look made from a scheme: dark or light as its background is, the window's
 * surfaces its background stepped toward its foreground, its text the foreground, and the
 * accent and states its ansi blue, yellow, red and green.
 */
export function schemeLook(scheme: TerminalPalette): { appearance: Appearance; tokens: Partial<Tokens> } {
  const { background: bg, foreground: fg, ansi } = scheme
  const toward = (by: number) => mix(bg, fg, by)
  const accent = ansi[4]!
  const surface = toward(0.04)
  const faint = mix(fg, bg, 0.5)
  return {
    appearance: appearanceOf(bg),
    tokens: {
      window: toward(0.05),
      surface,
      editor: bg,
      text: fg,
      muted: mix(fg, bg, 0.3),
      faint,
      border: toward(0.12),
      hover: toward(0.1),
      active: toward(0.16),
      selected: toward(0.2),
      accent,
      accentText: luminance(accent) > 0.4 ? "#000000" : "#ffffff",
      warning: ansi[3]!,
      error: ansi[1]!,
      success: ansi[2]!,
      overlay: `${surface}f2`,
      "agent.working": accent,
      "agent.blocked": ansi[3]!,
      "agent.error": ansi[1]!,
      "agent.done": ansi[2]!,
      "agent.idle": mix(fg, bg, 0.4),
      "agent.unknown": faint,
    },
  }
}
