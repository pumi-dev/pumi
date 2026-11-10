// A window's tree: the theme in use, resolved, drawing its `Window` (or the app's). A theme that
// throws falls back to the app's own window, saying why.
import { readFileSync } from "node:fs"
import { createContext, useEffect, useMemo, useState, type ComponentType } from "react"
import { onSlowLoop, slowLoopWarning } from "../react/components/index.ts"
import { useGpuix } from "../react/hooks/use-gpuix.ts"
import { blamed, Boundary, inCompartment } from "../sdk/contributions.ts"
import { useApp, useOption, useWindow } from "./state.ts"
import {
  isDark,
  METRICS,
  ThemeContext,
  TOKENS,
  type TerminalPalette,
  type ThemeSpec,
  type ThemeState,
} from "../sdk/theme.ts"
import { engine } from "./engine.ts"
import { themeOff } from "./mode.ts"
import { note, prefixOf } from "./plugins.ts"
import { AppLayers } from "./layers.tsx"
import { UpdateToasts } from "./update-toast.tsx"
import { DEFAULT } from "./theme/default.tsx"
import { DEFAULT_SCHEME, schemeLook, terminalScheme } from "./theme/terminal/index.ts"
import { useThemeSpec, useThemes } from "./themes.ts"
import { declareDocks } from "./docks.ts"

/** The terminals' palette the default theme gives with no scheme picked: GitHub Dark Default's. */
const TERMINAL: TerminalPalette = terminalScheme(DEFAULT_SCHEME)!

/**
 * pumi's own theme: `terminal_colorscheme`'s scheme made into tokens (`schemeLook`), else the
 * default theme's own (GitHub Dark Default's, `TOKENS`). Its settings (`ui_font_size`,
 * `ui_inactive_pane_fade`, `ui_pane_gap`, `ui_radius`, `ui_accent`, `ui_background_opacity`,
 * `ui_background_blur`) are its metrics, layout and window, and its accent over the look
 * (`usePumi`).
 */
const PUMI: ThemeSpec = { name: "pumi" }

/** A setting's number when it is one, else nothing (the default's then). */
const numberOf = (value: unknown): number | undefined => (typeof value === "number" && Number.isFinite(value) ? value : undefined)

/** pumi's own theme with its settings: its spec, and the accent its look takes (a `#rrggbb`). */
function usePumi(): { spec: ThemeSpec; accent?: string } {
  const fontSize = numberOf(useOption("ui_font_size"))
  const fade = numberOf(useOption("ui_inactive_pane_fade"))
  const gap = numberOf(useOption("ui_pane_gap"))
  const radius = numberOf(useOption("ui_radius"))
  // Unset is the options' defaults (a little see-through, blurred), not opaque.
  const opacity = numberOf(useOption("ui_background_opacity")) ?? 95
  const blur = numberOf(useOption("ui_background_blur")) ?? 40
  const given = useOption<string>("ui_accent")
  const accent = typeof given === "string" && /^#[0-9a-f]{6}$/i.test(given.trim()) ? given.trim() : undefined
  const spec = useMemo<ThemeSpec>(() => {
    // 100% (or unset) is opaque: no transparency and no blur, as if the settings were not there.
    const glass = opacity < 100 ? Math.min(Math.max(opacity, 0), 100) / 100 : undefined
    if ([fontSize, fade, gap, radius, glass].every((v) => v === undefined)) return PUMI
    const metrics: Partial<ThemeState["metrics"]> = {}
    if (glass !== undefined) metrics.backgroundOpacity = glass
    if (fontSize !== undefined) metrics.fontSize = Math.min(Math.max(fontSize, 8), 24)
    if (fade !== undefined) metrics.paneFade = Math.min(Math.max(fade, 0), 100) / 100
    if (radius !== undefined) {
      metrics.radius = Math.max(radius, 0)
      metrics.radiusSmall = Math.round((Math.max(radius, 0) * 2) / 3)
    }
    return {
      ...PUMI,
      metrics,
      ...(gap ? { layout: { gap: Math.max(gap, 0) } } : {}),
      ...(glass !== undefined ? { window: { background: "transparent" as const, blur: Math.max(blur ?? 0, 0) } } : {}),
    }
  }, [fontSize, fade, gap, radius, opacity, blur])
  return { spec, accent }
}

/**
 * The parts a theme gives that are drawn in a compartment of their own (`inCompartment`): what
 * they draw costs the window's frames only when it changes, and a slow one only its share. Not
 * the parts that hold tabs (`Window`, `Layout`, `Split`, `Pane`, `Float`): a terminal's output
 * draws its tab again, and a part around it on a budget would hold the output back.
 */
const COMPARTMENTED = ["Starting", "ViewBar", "View", "Panel", "Action"] as const

/**
 * A theme's own parts: the ones that hold no tab each in a compartment, and every one drawn with
 * the theme noted as what is drawing (if it freezes the app's JS, the app starts again without it).
 */
function ownParts(spec: ThemeSpec): Partial<ThemeState["components"]> {
  const given: Record<string, unknown> = {}
  const blame = `theme:${spec.name}`
  for (const [name, part] of Object.entries(spec.components ?? {})) {
    if (!part) continue
    given[name] = (COMPARTMENTED as readonly string[]).includes(name)
      ? inCompartment(part as ComponentType<any>, `${spec.name}.${name}`, blame)
      : blamed(part as ComponentType<any>, blame)
  }
  return given as Partial<ThemeState["components"]>
}

function resolve(spec: ThemeSpec, prefix?: string, colors?: Partial<ThemeState["tokens"]>, pumi = false): ThemeState {
  const own = colors ?? spec.colors ?? {}
  return {
    spec,
    tokens: { ...TOKENS, ...own } as ThemeState["tokens"],
    metrics: { ...METRICS, ...(spec.metrics ?? {}) } as ThemeState["metrics"],
    components: { ...DEFAULT, ...ownParts(spec) },
    prefix: pumi ? undefined : prefix,
  }
}

/** Font files already given to the text system: once is enough, for every window. */
const registered = new Set<string>()

/** Registers a theme's fonts (once each) before it draws. */
function useFonts(fonts: string[] | undefined) {
  const { renderer } = useGpuix()
  useMemo(() => {
    for (const path of fonts ?? []) {
      if (registered.has(path) || !renderer?.registerFont) continue
      registered.add(path)
      try {
        renderer.registerFont(readFileSync(path))
      } catch (err) {
        console.error(`pumi: the theme's font ${path}:`, (err as Error)?.message ?? err)
      }
    }
  }, [fonts, renderer])
}

const NO_LINES: string[] = []

/**
 * The theme a window draws now, resolved: `ui_theme`'s (none in safe mode, nor one that `failed`),
 * else pumi's own with its settings, for the look the theme says, with the terminals' palette.
 * Settings draws with it too (`settings/window.tsx`).
 */
export function useResolvedTheme(safe: boolean, failed?: string): { state: ThemeState & { palette: TerminalPalette }; own: TerminalPalette; shown?: ThemeSpec; drawn: ThemeSpec; plugin?: string } {
  const name = useOption<string>("ui_theme")
  // A theme that froze the app's JS is left out until the app starts again (`app/mode.ts`).
  const { spec: loaded, error: themeError } = useThemeSpec(safe || themeOff || name === "default" ? undefined : name)
  const spec = safe || themeOff ? undefined : loaded
  useEffect(() => {
    if (themeError) console.error(`pumi: the theme ${name}: ${themeError}`)
  }, [themeError])
  const plugin = useThemes().find((t) => t.id === name)?.plugin
  const shown = failed ? undefined : spec
  const own = usePumi()
  const pumi = !shown
  const drawn = shown ?? own.spec
  const prefix = plugin ? prefixOf(plugin) : undefined
  // `terminal_colorscheme`, when it names a scheme, colours the terminals instead of the theme,
  // and pumi's own theme takes its colours from it too. Unset is its default (GitHub Dark
  // Default); empty means the theme's own colours, and without a scheme the terminals sit on the
  // window's colour unless the theme gives them their own.
  const picked = useOption<string>("terminal_colorscheme")
  const schemeName = picked ?? DEFAULT_SCHEME
  const scheme = useMemo(() => terminalScheme(schemeName), [schemeName])
  const matched = useMemo(() => (pumi && scheme ? schemeLook(scheme) : undefined), [pumi, scheme])
  // A theme that makes its colours from the palette (`fromPalette`) looks as the scheme does.
  const follows = !pumi && !!drawn.fromPalette
  const accent = pumi ? own.accent : undefined
  const state = useMemo(() => {
    const colors = accent ? { ...(matched ?? {}), accent } : matched
    const made = resolve(drawn, prefix, colors, pumi)
    const own = drawn.terminal ?? {}
    // pumi's own theme without a scheme is exactly GitHub Dark Default, the scheme it draws itself
    // from; another theme's terminals sit on its window's colour, unless it gives their own.
    const given: TerminalPalette = scheme
      ? { ...scheme, ...(own.opacity === undefined ? {} : { opacity: own.opacity }) }
      : pumi
        ? { ...TERMINAL, ...own }
        : { ...TERMINAL, background: made.tokens.window, ...own }
    const resolved = follows ? { ...made, tokens: { ...made.tokens, ...drawn.fromPalette!(given) } as ThemeState["tokens"] } : made
    // See-through (`backgroundOpacity`), the window still paints its colours as they are: the
    // renderer lets the desktop through every background by as much (`window.setGlass`).
    return { ...resolved, palette: given }
  }, [drawn, prefix, scheme, matched, accent, pumi, follows])
  // The palette with no scheme picked: the theme's own, on the default theme's colours, for
  // Settings to show beside the schemes (the same as the default scheme's, for pumi's own).
  const ownPalette = useMemo<TerminalPalette>(() => {
    if (!scheme) return state.palette
    if (pumi) return { ...TERMINAL, ...drawn.terminal }
    const tokens = { ...TOKENS, ...drawn.colors }
    return { ...TERMINAL, background: tokens.window, ...drawn.terminal }
  }, [state.palette, scheme, pumi, drawn])
  return { state, own: ownPalette, shown, drawn, plugin }
}

/** The palette the theme gives with no scheme picked (`useResolvedTheme`'s `own`), in Settings. */
export const OwnPaletteContext = createContext<TerminalPalette | undefined>(undefined)

const HEX = /^#[0-9a-f]{6}$/i
/** The terminals' colours set one by one (`terminal_background`, `terminal_color_1`, a Ghostty
 *  config's `background` and `palette`), over the scheme's. */
function useTerminalColors(): { slots: Partial<TerminalPalette>; pinned: Record<number, string> } {
  const options = useApp((s) => s.options)
  const key = JSON.stringify(
    ["terminal_background", "terminal_foreground", "terminal_cursor_color", "terminal_selection_background", ...Array.from({ length: 16 }, (_, i) => `terminal_color_${i}`)].map((name) => {
      const value = options[name]
      return typeof value === "string" && HEX.test(value.trim()) ? value.trim() : null
    }),
  )
  return useMemo(() => {
    const [background, foreground, cursor, selection, ...ansi] = JSON.parse(key) as (string | null)[]
    const slots: Partial<TerminalPalette> = {}
    if (background) slots.background = background
    if (foreground) slots.foreground = foreground
    if (cursor) slots.cursor = cursor
    if (selection) slots.selection = selection
    const pinned: Record<number, string> = {}
    ansi.forEach((color, i) => color && (pinned[i] = color))
    return { slots, pinned }
  }, [key])
}

export function Root({ safe }: { safe: boolean }) {
  const [failed, setFailed] = useState<string>()
  const { state, shown, drawn, plugin } = useResolvedTheme(safe, failed)
  useFonts(shown?.fonts)
  // An endless animation the renderer slowed for what its frames cost goes in the log of who drew
  // it: a plugin's component (its compartment is its id), or the theme's part (`<theme>.<Part>`).
  useEffect(
    () =>
      onSlowLoop((compartment) => {
        const line = slowLoopWarning(compartment)
        console.warn(line)
        const owner = compartment?.includes("/") ? compartment : compartment?.startsWith(`${drawn.name}.`) ? plugin : undefined
        if (owner) note(owner, line)
      }),
    [drawn, plugin],
  )
  // Its docks' toggles and their keys, while it is drawn.
  const docks = shown?.docks
  useEffect(() => (docks ? declareDocks(docks) : undefined), [docks])
  const { renderer } = useGpuix()
  const window = useWindow()
  useEffect(() => {
    const given = shown?.tooltip
    const style = typeof given === "function" ? given(state.tokens, state.metrics) : given
    try {
      renderer?.setTooltipStyle?.(style ? JSON.stringify(style) : null)
    } catch (err) {
      console.error(`pumi: the theme's tooltip style:`, (err as Error)?.message ?? err)
    }
  }, [state, renderer])
  const background = drawn.window?.background ?? "opaque"
  const blur = background === "opaque" ? 0 : (drawn.window?.blur ?? 0)
  const glass = background === "opaque" ? 1 : Math.min(Math.max(state.metrics.backgroundOpacity ?? 1, 0), 1)
  useEffect(() => {
    try {
      window.setBackground(background)
      window.setBlur(blur)
      window.setGlass(glass)
    } catch (err) {
      console.error("pumi: the window's background:", (err as Error)?.message ?? err)
    }
  }, [background, blur, glass, window])
  // The native chrome (a webview tab's) follows the look the theme resolved to, not macOS's: a
  // dark window means a dark title bar and page chrome.
  const dark = isDark(state.tokens.window)
  useEffect(() => {
    try {
      engine.setAppearance(dark ? "dark" : "light")
    } catch (err) {
      console.error("pumi: the appearance:", (err as Error)?.message ?? err)
    }
  }, [dark])
  const generate = useOption<boolean>("terminal_palette_generate") === true
  const harmonious = useOption<boolean>("terminal_palette_harmonious") === true
  const own = useTerminalColors()
  useEffect(() => {
    if (!state.palette) return
    const palette = { ...state.palette, ...own.slots, palette: { ...state.palette.palette, ...own.pinned } }
    engine.setTerminalPalette({ ...palette, generate, harmonious })
  }, [state.palette, generate, harmonious, own])
  const { Window, Starting } = state.components
  const ready = useApp((s) => s.snapshot !== undefined)
  const setup = useApp((s) => s.setup)
  const error = useApp((s) => s.error)
  // The theme's window, and over it the app's own layers, whatever the theme draws. The wrapper
  // is the root's size, as the theme's window was.
  return (
    <ThemeContext.Provider value={state}>
      <div style={{ width: "100%", height: "100%", position: "relative" }}>
        <Boundary
          name={state.spec.name}
          fallback={(error) => {
            queueMicrotask(() => setFailed(`${state.spec.name}: ${error.message}`))
            return null
          }}
        >
          {ready ? (
            <>
              <Window />
              <UpdateToasts />
              <AppLayers />
            </>
          ) : (
            <Starting
              title={setup?.title ?? (error ? "The server did not answer" : "")}
              detail={setup?.detail ?? error ?? ""}
              gaveUp={setup?.gaveUp ?? error !== undefined}
              lines={setup?.lines ?? NO_LINES}
            />
          )}
        </Boundary>
      </div>
    </ThemeContext.Provider>
  )
}
