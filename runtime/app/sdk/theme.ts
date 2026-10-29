// A UI theme: its colours (dark and light), the terminal's palette, its metrics and fonts, and
// the window's parts it draws itself. The app's own parts (app/theme/default.tsx) draw anything a
// theme leaves out, so a theme can replace one part (its view bar) or the whole window (`Window`).
import { createContext, useContext, type ComponentType } from "react"
import type { StyleDesc } from "../native/host.ts"
import { useApp, useOption, type Tree } from "../app/state.ts"
import type { Action } from "./actions.ts"
import type { ComponentItem } from "./contributions.ts"

/** The colour tokens every theme has; a theme may add its own. */
export interface Tokens {
  window: string
  surface: string
  editor: string
  text: string
  muted: string
  faint: string
  border: string
  hover: string
  active: string
  selected: string
  accent: string
  accentText: string
  warning: string
  error: string
  success: string
  overlay: string
  "status.working": string
  "status.blocked": string
  "status.error": string
  "status.done": string
  "status.idle": string
  "status.unknown": string
  [token: string]: string
}

export interface TerminalPalette {
  background: string
  foreground: string
  selection: string
  cursor: string
  ansi: string[]
  opacity?: number
  /**
   * Colours of its own for any of the 256 by index (`{ 196: "#ff3b30" }`). The rest above sixteen
   * are xterm's, or made from the sixteen when `terminal_palette_generate` is on; a pinned one
   * stays either way.
   */
  palette?: Record<number, string>
}

export interface Metrics {
  fontSize: number
  radius: number
  radiusSmall: number
  /** How much a terminal fades while another pane has the focus: 0 (not at all) to 1 (gone). */
  paneFade: number
  /**
   * How much the window's backgrounds hide what is behind it: 1 is opaque, lower lets it show
   * through (with `window.blur`, blurred). Under 1 the default theme's window is `"transparent"`.
   * The parts paint their colours as they are, and the renderer lets the desktop through every
   * background by as much, however many are stacked: each keeps its own colour, and text, icons
   * and images stay opaque. macOS only for now; elsewhere the window stays opaque.
   */
  backgroundOpacity: number
  [metric: string]: number
}

/** A theme's two looks. */
export type Appearance = "dark" | "light"

/** What a theme is: `defineTheme({...})`. */
export interface ThemeSpec {
  name: string
  description?: string
  /**
   * Its looks: its colours over the default's, per appearance. A theme with one look gives only
   * that one (`colors: { light: … }`, and `terminal` the same) and is always drawn so.
   */
  colors?: { dark?: Partial<Tokens>; light?: Partial<Tokens> }
  terminal?: { dark?: Partial<TerminalPalette>; light?: Partial<TerminalPalette> }
  /**
   * Which look draws when it has both: always one (`"dark"`, `"light"`), the system's
   * (`"system"`, what leaving it out means), or what the user picks in a setting of its own:
   * `{ option: "appearance" }` names an option its manifest declares (`contributes.options`,
   * unprefixed) whose value is `system`, `dark` or `light`. The app has no such setting itself.
   */
  appearance?: Appearance | "system" | { option: string }
  metrics?: Partial<Metrics>
  /** The window's parts it draws itself, by name (see `Components`); the app's draw the rest. */
  components?: Partial<Components>
  /**
   * Font files the theme brings (`import pixel from "./fonts/pixel.ttf"`), registered before
   * it draws; name them in `fontFamily` by their family name.
   */
  fonts?: string[]
  /** How every `title` tooltip looks: a style, or one made from the tokens and metrics. */
  tooltip?: StyleDesc | ((tokens: Tokens, metrics: Metrics) => StyleDesc)
  /** The window itself. */
  window?: {
    /** What shows where the theme paints nothing: `"blurred"` is the system's vibrancy (glass). */
    background?: "opaque" | "transparent" | "blurred"
    /** How much what shows through a background that is not opaque is blurred, in px; 0 (or none) is sharp. macOS. */
    blur?: number
  }
  /**
   * How a view's tiled panes sit together (the `Splitter` part draws the edge that drags): a style,
   * or one made from the tokens, the metrics and the theme's own settings (`option("pane_style")`,
   * unprefixed, as `useThemeOption` reads them), drawn again as they change.
   */
  layout?: LayoutStyle | ((tokens: Tokens, metrics: Metrics, option: (key: string) => unknown) => LayoutStyle)
  /**
   * Its colours made from the terminals' palette (`terminal_colorscheme`'s scheme, else the
   * theme's own `terminal`), over `colors`: a theme that looks like the terminals it holds
   * whatever scheme is picked, as pumi's own does. With a scheme picked, its look (dark or light)
   * is the scheme's.
   */
  fromPalette?: (palette: TerminalPalette, appearance: Appearance) => Partial<Tokens>
  /**
   * The window's docks, by a name of the theme's choosing: a side bar, a second one on the right,
   * a panel at the foot. Each gets a command that shows and hides it (`toggle_<name>`, in the View
   * menu), its keys, a size the user drags and the app keeps, and the plugins' `panel` components
   * it holds. `<Dock name>` draws one; `useDock` and `useDockPanels` read it. `sidebar` is the
   * window's own side bar (`toggle_sidebar`, ⌘B), declared or not.
   */
  docks?: Record<string, DockSpec>
}

/** One of a theme's docks (`docks.<name>`). */
export interface DockSpec {
  /** What it is called: the View menu's item and its command's label (`Toggle <title>`). */
  title?: string
  /**
   * The chord that shows and hides it while the theme is drawn (`"cmd+alt+b"`), taken from
   * whichever command had it; `keymap_bindings` still wins. A dock named `secondary_sidebar` has
   * ⌘⌥B and one named `panel` ⌘J without saying so.
   */
  keys?: string
  /** Whether it shows until the user says otherwise; true by default. */
  open?: boolean
  /** Its size across (a side's width, a top or foot's height) in px until the user drags it; 260 by default. */
  size?: number
  /** How small and how big a drag makes it; 160 and 640 by default. */
  min?: number
  max?: number
  /**
   * The plugins' `panel` components it holds, as globs over a component's key
   * (`owner/name/component`): `["you/git/*"]`. A component goes to the first dock that names it,
   * and `"*"` takes every component no dock names. Without it a dock holds none: it draws only
   * what the theme puts in it.
   */
  panels?: string[]
}

/** How a view's tiled panes sit together. */
export interface LayoutStyle {
  /** Room between two tiled panes, and round them all, in px: `{ between, around }`, or one for both. 0 by default. */
  gap?: number | { between?: number; around?: number }
  /** The line between two panes that touch (no `gap`), in px, in `border`'s colour; 1 by default, 0 for none. */
  divider?: number
  /**
   * The frame of a pane drawn over the others (zoomed, and on its way back): a style, or one
   * made from the tokens and metrics. By default a card of `editor` with a border and a shadow.
   */
  lifted?: StyleDesc | ((tokens: Tokens, metrics: Metrics) => StyleDesc)
  /** How far in from the layout's edges a zoomed pane stops, in px (the panes under it show round it); 14 by default. */
  zoomInset?: number
}

/** A theme's `layout`, resolved: what the app's tiling draws with. */
export function useLayoutStyle(): { between: number; around: number; divider: number; lifted: StyleDesc; zoomInset: number } {
  const state = themeState()
  const options = useApp((s) => s.options)
  const spec = state.spec?.layout
  const given = (typeof spec === "function" ? spec(state.tokens, state.metrics, (key) => options[`${state.prefix ?? ""}${key}`]) : spec) ?? {}
  const gap = given.gap
  const between = typeof gap === "number" ? gap : (gap?.between ?? 0)
  const around = typeof gap === "number" ? gap : (gap?.around ?? 0)
  const lifted = typeof given.lifted === "function" ? given.lifted(state.tokens, state.metrics) : given.lifted
  return {
    between,
    around,
    divider: given.divider ?? 1,
    lifted: lifted ?? {
      borderWidth: 1,
      borderColor: state.tokens.border,
      backgroundColor: state.tokens.editor,
      boxShadow: { offsetX: 0, offsetY: 8, blurRadius: 28, spreadRadius: 0, color: "#00000080" },
    },
    zoomInset: given.zoomInset ?? 14,
  }
}

export function defineTheme(spec: ThemeSpec): ThemeSpec {
  return spec
}

export const DARK: Tokens = {
  window: "#282c34",
  surface: "#26282b",
  editor: "#1e1f22",
  text: "#dfe1e5",
  muted: "#b0b1b3",
  faint: "#888a8f",
  border: "#393b41",
  hover: "#3c3e41",
  active: "#494a4d",
  selected: "#4a4d54",
  accent: "#3474f0",
  accentText: "#ffffff",
  warning: "#f0c674",
  error: "#cc6666",
  success: "#a6e3a1",
  overlay: "#26282bf2",
  "status.working": "#89b4fa",
  "status.blocked": "#fab387",
  "status.error": "#f38ba8",
  "status.done": "#a6e3a1",
  "status.idle": "#9399b2",
  "status.unknown": "#6c7086",
}

export const LIGHT: Tokens = {
  window: "#f7f8fa",
  surface: "#f7f8fa",
  editor: "#ffffff",
  text: "#1f2023",
  muted: "#6c707e",
  faint: "#818594",
  border: "#dcdee3",
  hover: "#e4e5e9",
  active: "#d8dae0",
  selected: "#d3d5db",
  accent: "#3574f0",
  accentText: "#ffffff",
  warning: "#b27200",
  error: "#db3b4b",
  success: "#40a02b",
  overlay: "#f7f8faf2",
  "status.working": "#1e66f5",
  "status.blocked": "#d35e00",
  "status.error": "#d20f39",
  "status.done": "#2f8a1e",
  "status.idle": "#7c7f93",
  "status.unknown": "#9ca0b0",
}

export const METRICS: Metrics = { fontSize: 13, radius: 6, radiusSmall: 4, paneFade: 0.25, backgroundOpacity: 1 }

/** The theme in use and what it resolves to now. */
export interface ThemeState {
  spec: ThemeSpec
  appearance: "dark" | "light"
  tokens: Tokens
  metrics: Metrics
  /** The colour palette as it is drawn now (what the terminals use): `terminal_colorscheme`'s, the
   *  theme's, or the default's on the window's colour. */
  palette?: TerminalPalette
  components: Components
  /** The theme's plugin's option prefix (`owner.name.`), for `useThemeOption`. */
  prefix?: string
}

export const ThemeContext = createContext<ThemeState | null>(null)

function themeState(): ThemeState {
  const state = useContext(ThemeContext)
  if (!state) throw new Error("pumi: a theme's hook outside the window")
  return state
}

/** The theme's colour tokens, for the appearance in use. */
export function useTheme(): Tokens {
  return themeState().tokens
}

export function useMetrics(): Metrics {
  return themeState().metrics
}

export function useAppearance(): "dark" | "light" {
  return themeState().appearance
}

/** The colour palette as it is drawn now: the terminals' background, foreground, cursor, selection
 *  and sixteen colours, for a component that should look like the terminal beside it. */
export function usePalette(): TerminalPalette | undefined {
  return themeState().palette
}

/**
 * The palette as colour tokens (`palette.background`, `palette.foreground`, `palette.selection`,
 * `palette.cursor`, `palette.color0`…`palette.color15`): what a plugin's page reads beside the
 * theme's. The background carries the terminals' opacity as its alpha.
 */
export function paletteTokens(palette: TerminalPalette | undefined): Record<string, string> {
  if (!palette) return {}
  const tokens: Record<string, string> = {
    "palette.background": withOpacity(palette.background, palette.opacity),
    "palette.foreground": palette.foreground,
    "palette.selection": palette.selection,
    "palette.cursor": palette.cursor,
  }
  palette.ansi.forEach((color, i) => (tokens[`palette.color${i}`] = color))
  return tokens
}

/** `#rrggbb` at `opacity` (0–1) as `#rrggbbaa`; anything else, or no opacity, as it is. */
export function withOpacity(color: string, opacity: number | undefined): string {
  if (opacity === undefined || opacity >= 1 || !/^#[0-9a-f]{6}$/i.test(color)) return color
  const a = Math.round(Math.max(0, opacity) * 255)
    .toString(16)
    .padStart(2, "0")
  return `${color}${a}`
}

/**
 * One of the theme's own settings: an option its plugin declares in its manifest
 * (`contributes.options.<key>`), which Settings shows under the plugin. `key` is unprefixed.
 */
export function useThemeOption<T = unknown>(key: string, fallback?: T): T | undefined {
  const prefix = themeState().prefix
  const value = useOption<T>(prefix ? `${prefix}${key}` : key)
  return value ?? fallback
}

/** The window's parts, as the theme draws them (or the app does). */
export function useComponents(): Components {
  return themeState().components
}

/**
 * The window's parts. Each is a component a theme may give; what it doesn't give, the app's own
 * draws. A part that takes contributions draws their slot (`useSlot`) somewhere in it.
 */
export interface Components {
  /** The whole window. */
  Window: ComponentType
  /** What the window shows before the server answers: it starting, or why it could not. */
  Starting: ComponentType<{ title: string; detail: string; gaveUp: boolean; lines: string[] }>
  /** The bar of the window's session's views. */
  ViewBar: ComponentType<{ session?: string }>
  /** One view in it. */
  View: ComponentType<{ view: string; active: boolean; index: number }>
  /** A view's panes, as its layout splits them. */
  Layout: ComponentType<{ view?: string }>
  /**
   * One node of that layout: a pane, or a split of two whose edge drags (a double press on it
   * evens the view's splits). A theme with chrome of its own — an empty view's welcome, a frame
   * around the panes — draws this inside it rather than a split renderer of its own.
   */
  Split: ComponentType<{ tree: Tree; view?: string; focused?: string }>
  /**
   * One pane: its tabs (in its strip's order) and the one in front (`active`), drawn as its bar
   * and that tab's body. `focused` while it has its view's focus, `lifted` while it is drawn over
   * the others (zoomed, or on its way back).
   */
  Pane: ComponentType<{ pane: string; tabs: string[]; active?: string | null; focused: boolean; zoomed: boolean; lifted?: boolean }>
  /**
   * A pane's bar: its tabs' strip (each a `PaneTab`) while panes hold tabs (`pane_tabs`), else
   * its tab's title; then its toolbar (`tab.toolbar`, the front tab's) and its buttons. `menu`
   * opens the front tab's menu.
   */
  PaneTitle: ComponentType<{ pane: string; tabs: string[]; active?: string | null; focused: boolean; zoomed: boolean; /** The mouse is over the pane. */ over?: boolean; menu(e: { x?: number; y?: number }): void }>
  /** One tab in a pane's strip: what brings it to the front, drags it, and closes it. `focused` while its pane has the focus. */
  PaneTab: ComponentType<{ tab: string; pane: string; index: number; active: boolean; focused: boolean }>
  /** What a tab holds: its strip (`tab.strip`), its block as its renderer draws it, and its overlays. */
  TabBody: ComponentType<{ tab: string; focused: boolean }>
  /**
   * The edge between two tiled panes, drawn filling the box that drags it (a few px across, over
   * the seam): `axis` is the split's (`row`: the panes are side by side), `dragging` while it moves.
   */
  Splitter: ComponentType<{ axis: "row" | "column"; dragging: boolean }>
  /** A tab floating over the view (`kind: "float"`). */
  Float: ComponentType<{ tab: string }>
  /** What contributes to a slot (`panel`), stacked, tabbed or collapsed as the theme likes. */
  Panel: ComponentType<{ point: string }>
  /** Every session, its views and their tabs, as a tree: the default side bar's own, heading the first page of its sections. */
  Sessions: ComponentType
  /** A tab with a status (`@status`), as a list of tabs (the `Sessions` tree) shows it. */
  TabRow: ComponentType<{ tab: string }>
  /** A tab row's actions: the `tab.row.controls` contributions, given the tab. */
  TabRowControls: ComponentType<{ tab: string }>
  /** One action, as a button. */
  Action: ComponentType<{ action: Action; args?: unknown }>
  /**
   * A menu of actions where the mouse is (`useMenu` opens one): their groups with a rule between,
   * a submenu for an action with `items`, and the slot's components (each given `args` and
   * `close`). A press outside it, or a pick, calls `close`.
   */
  Menu: ComponentType<MenuProps>
  /**
   * A question over the whole window (`confirm`, `prompt`): its title, what it means, and its
   * buttons; a field too, for a prompt. Escape or a press outside it cancels.
   */
  Dialog: ComponentType<DialogProps>
  /**
   * A small icon button: the window's own (a pane's split and close) and what a plugin's
   * component draws with `IconButton` from `pumi`, so a component's buttons look like the theme's.
   */
  IconButton: ComponentType<IconButtonProps>
  /**
   * A tab's icon, wherever the window shows one (its pane's strip, the side bar, a view's tabs,
   * a tab being dragged): its own (`useTabIcon`: its `@icon`, its agent's mark, its block's), or
   * its status (`useTabStatus`) as the theme likes. `color` is what the icon is tinted with, when
   * the place it is in has a colour of its own.
   */
  TabIcon: ComponentType<TabIconProps>
}

/** What a theme's `TabIcon` part is given. */
export interface TabIconProps {
  tab: string
  size?: number
  color?: string
}

export interface MenuProps {
  /** Where it opens: the mouse, in window px. */
  at: { x: number; y: number }
  actions: Action[]
  components?: ComponentItem[]
  /** What its components are given (and `close`). */
  args?: Record<string, unknown>
  close(): void
}

export interface DialogProps {
  title: string
  detail?: string
  /** The button that goes ahead, and the one that does not (`Cancel`). */
  confirm: string
  cancel: string
  /** The going ahead ends something: drawn as a warning. */
  danger?: boolean
  /** A prompt's field. */
  input?: { value: string; placeholder?: string; onChange(value: string): void }
  onConfirm(): void
  onCancel(): void
}

/** What a button of the window is: `IconButton` in `pumi`, the `IconButton` part in a theme. */
export interface IconButtonProps {
  /** A name of the app's icons. */
  icon: string
  /** Its tooltip and accessible label. */
  title?: string
  onClick(): void
  /** Drawn as on (a toggle that is). */
  active?: boolean
  /** The icon's colour while not hovered, from `useTheme()`; the theme's own by default. */
  color?: string
}

/** An action of a slot's: data the theme draws as it likes. */
export type ActionItem = Action
