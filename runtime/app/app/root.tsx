// A window's tree: the theme in use, resolved for the appearance, drawing its `Window` (or the
// app's). A theme that throws falls back to the app's own window, saying why.
import { readFileSync } from "node:fs"
import { useEffect, useMemo, useState, type ComponentType } from "react"
import { Engine } from "../native/addon.js"
import { onSlowLoop, slowLoopWarning } from "../react/components/index.ts"
import { useGpuix } from "../react/hooks/use-gpuix.ts"
import { blamed, Boundary, inCompartment } from "../sdk/contributions.ts"
import { useApp, useOption, useWindow } from "./state.ts"
import {
  DARK,
  LIGHT,
  METRICS,
  ThemeContext,
  type Appearance,
  type TerminalPalette,
  type ThemeSpec,
  type ThemeState,
} from "../sdk/theme.ts"
import { engine } from "./engine.ts"
import { themeOff } from "./mode.ts"
import { note, prefixOf } from "./plugins.ts"
import { AppLayers } from "./layers.tsx"
import { DEFAULT } from "./theme/default.tsx"
import { appearanceOf, schemeLook, terminalScheme } from "./theme/terminal/index.ts"
import { useThemeSpec, useThemes } from "./themes.ts"
import { declareDocks } from "./docks.ts"

const TERMINAL: { dark: TerminalPalette; light: TerminalPalette } = {
  dark: {
    background: "#1e1f22",
    foreground: "#d4d6da",
    selection: "#3a4a63",
    cursor: "#d8d8d8",
    ansi: ["#000000", "#fa5355", "#56b45a", "#c2c300", "#4581eb", "#fa54ff", "#33c2c1", "#adadad", "#555555", "#fb7172", "#67ff4f", "#ffff00", "#6d9df1", "#fb82ff", "#60d3d1", "#eeeeee"],
  },
  light: {
    background: "#ffffff",
    foreground: "#383a42",
    selection: "#bfceff",
    cursor: "#4f525e",
    ansi: ["#383a42", "#e45649", "#50a14f", "#c18401", "#0184bc", "#a626a4", "#0997b3", "#bcbcbc", "#4f525e", "#e06c75", "#6aa65e", "#d19a2a", "#3b8ee8", "#b359b1", "#2aa8c2", "#ffffff"],
  },
}

/** The system's appearance, as macOS says it. */
const SYSTEM = Engine.systemAppearance()

/**
 * pumi's own theme: one look, `terminal_colorscheme`'s scheme made into tokens (`schemeLook`), or
 * the default's dark one without a scheme. Dark or light is what its background is. Its settings
 * (`ui_font_size`, `ui_inactive_pane_fade`, `ui_pane_gap`, `ui_radius`, `ui_accent`,
 * `ui_background_opacity`, `ui_background_blur`) are its metrics, layout and window, and its
 * accent over the look (`usePumi`).
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
 * The look a theme draws: its only one, or the one its `appearance` says (its own setting's
 * value, the system's, or a fixed one). A theme that gives neither look has both, the default's.
 */
function useLook(spec: ThemeSpec, prefix: string | undefined): Appearance {
  const given = (["dark", "light"] as const).filter((a) => spec.colors?.[a] || spec.terminal?.[a])
  const want = spec.appearance
  const key = typeof want === "object" ? `${prefix ?? ""}${want.option}` : ""
  const chosen = useOption<string>(key)
  if (given.length === 1) return given[0]!
  const pick = typeof want === "object" ? chosen : want
  return pick === "dark" || pick === "light" ? pick : SYSTEM
}

/**
 * The parts a theme gives that are drawn in a compartment of their own (`inCompartment`): what
 * they draw costs the window's frames only when it changes, and a slow one only its share. Not
 * the parts that hold tabs (`Window`, `Layout`, `Split`, `Pane`, `Float`): a terminal's output
 * draws its tab again, and a part around it on a budget would hold the output back.
 */
const COMPARTMENTED = ["Starting", "ViewBar", "View", "Panel", "Agent", "AgentControls", "Action"] as const

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

function resolve(spec: ThemeSpec, appearance: Appearance, prefix?: string, colors?: Partial<ThemeState["tokens"]>, pumi = false): ThemeState {
  const base = appearance === "dark" ? DARK : LIGHT
  const own = colors ?? spec.colors?.[appearance] ?? {}
  return {
    spec,
    appearance,
    tokens: { ...base, ...own } as ThemeState["tokens"],
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
export function useResolvedTheme(safe: boolean, failed?: string): { state: ThemeState & { palette: TerminalPalette }; shown?: ThemeSpec; drawn: ThemeSpec; plugin?: string } {
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
  const look = useLook(drawn, pumi ? undefined : prefix)
  // `terminal_colorscheme`, when it names a scheme, colours the terminals instead of the theme,
  // and pumi's own theme takes its look from it too. Without one, the terminals sit on the
  // window's colour unless the theme gives them their own.
  const schemeName = useOption<string>("terminal_colorscheme")
  const scheme = useMemo(() => terminalScheme(schemeName), [schemeName])
  const matched = useMemo(() => (pumi && scheme ? schemeLook(scheme) : undefined), [pumi, scheme])
  // A theme that makes its colours from the palette (`fromPalette`) looks as the scheme does: dark
  // or light as its background is.
  const follows = !pumi && !!drawn.fromPalette
  const appearance = pumi ? (matched?.appearance ?? appearanceOf(DARK.editor)) : follows && scheme ? appearanceOf(scheme.background) : look
  const accent = pumi ? own.accent : undefined
  const state = useMemo(() => {
    const colors = accent ? { ...(matched?.tokens ?? {}), accent } : matched?.tokens
    const made = resolve(drawn, appearance, prefix, colors, pumi)
    const own = drawn.terminal?.[appearance] ?? {}
    const given: TerminalPalette = scheme
      ? { ...scheme, ...(own.opacity === undefined ? {} : { opacity: own.opacity }) }
      : { ...TERMINAL[appearance], background: made.tokens.window, ...own }
    const resolved = follows ? { ...made, tokens: { ...made.tokens, ...drawn.fromPalette!(given, appearance) } as ThemeState["tokens"] } : made
    // See-through (`backgroundOpacity`), the window still paints its colours as they are: the
    // renderer lets the desktop through every background by as much (`window.setGlass`).
    return { ...resolved, palette: given }
  }, [drawn, appearance, prefix, scheme, matched, accent, pumi, follows])
  return { state, shown, drawn, plugin }
}

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
  // The native chrome (a webview tab's) follows the look the theme resolved to, not macOS's.
  useEffect(() => {
    try {
      engine.setAppearance(state.appearance)
    } catch (err) {
      console.error("pumi: the appearance:", (err as Error)?.message ?? err)
    }
  }, [state.appearance])
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
