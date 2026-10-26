// The settings a theme has of its own that the app's kit reads for it: how its panes and side bar
// behave, whether its layout moves, whether its controls tap the trackpad. Each is the drawing
// theme's: pumi's own theme's is a core option in Settings' "Default theme" groups
// (`ui_pane_titles`, …); a plugin's theme declares the key in its manifest
// (`contributes.options.pane_titles`), and Settings shows it on its page. A theme that declares
// none of them gets each one's default.
import { useCallback, useContext } from "react"
import { ThemeContext } from "../sdk/theme.ts"
import { store } from "./engine.ts"
import { safe, themeOff } from "./mode.ts"
import { setOption } from "./settings/store.ts"
import { useOption } from "./state.ts"
import { getThemes } from "./themes.ts"

/** The most tab icons a view shows (`view_icons`); past them, a count. */
export const VIEW_ICONS_MAX = 3

/** Each setting by the key a plugin's theme declares it under: pumi's own theme's option for it, and its default. */
export const THEME_SETTINGS = {
  pane_titles: { option: "ui_pane_titles", default: true },
  pane_outline: { option: "ui_pane_outline", default: false },
  pane_equalize: { option: "ui_pane_equalize", default: true },
  focus_follows_mouse: { option: "ui_focus_follows_mouse", default: false },
  sidebar_swipe: { option: "ui_sidebar_swipe", default: true },
  view_icons: { option: "ui_view_icons", default: VIEW_ICONS_MAX },
  // A number for what settings.json kept before it was a theme's: 1 on, 0 off.
  animate: { option: "animate", default: true as boolean | number },
  haptics: { option: "haptics", default: true },
} as const

export type ThemeSetting = keyof typeof THEME_SETTINGS
type ValueOf<K extends ThemeSetting> = (typeof THEME_SETTINGS)[K]["default"] extends boolean ? boolean : number

/** A plugin's option prefix, from its id (`owner/name` → `owner.name.`), as `plugins.ts`'s `prefixOf`. */
const prefixOf = (id: string) => `${id.replace("/", ".").replaceAll("-", "_")}.`

/** The option `key` is for the theme with option prefix `prefix` (none: pumi's own). */
export function settingName(key: ThemeSetting, prefix: string | undefined): string {
  return prefix ? `${prefix}${key}` : THEME_SETTINGS[key].option
}

/** The option `key` is for the theme `ui_theme` names, outside a window's tree (a command, a haptic). */
export function currentSettingName(key: ThemeSetting): string {
  const name = store.get().options.ui_theme
  const theme = safe || themeOff || !name || name === "default" ? undefined : getThemes().find((t) => t.id === name)
  return settingName(key, theme ? prefixOf(theme.plugin) : undefined)
}

/** `key`'s value for the theme `ui_theme` names, outside a window's tree. */
export function currentSetting<K extends ThemeSetting>(key: K): ValueOf<K> {
  const value = store.get().options[currentSettingName(key)]
  return (value ?? THEME_SETTINGS[key].default) as ValueOf<K>
}

/** Sets `key` for the theme drawn now, as Settings would (settings.json keeps it). */
export function setSetting(key: ThemeSetting, value: boolean | number): Promise<void> {
  return setOption(currentSettingName(key), value)
}

/** Turns a switch of the theme drawn now over (`toggle_pane_titles`, `toggle_auto_equalize`). */
export function toggleSetting(key: ThemeSetting): void {
  setSetting(key, !truthy(currentSetting(key))).catch((err) => console.error(`pumi: ${currentSettingName(key)}:`, String(err?.message ?? err)))
}

const truthy = (v: unknown) => v !== false && v !== 0

/** `key` for the theme drawing this window, and what sets it. */
export function useThemeSetting<K extends ThemeSetting>(key: K): [ValueOf<K>, (next: ValueOf<K> | ((old: ValueOf<K>) => ValueOf<K>)) => void] {
  const prefix = useContext(ThemeContext)?.prefix
  const name = settingName(key, prefix)
  const value = (useOption(name) ?? THEME_SETTINGS[key].default) as ValueOf<K>
  const set = useCallback(
    (next: ValueOf<K> | ((old: ValueOf<K>) => ValueOf<K>)) => {
      const old = (store.get().options[name] ?? THEME_SETTINGS[key].default) as ValueOf<K>
      const v = typeof next === "function" ? (next as (old: ValueOf<K>) => ValueOf<K>)(old) : next
      setOption(name, v).catch((err) => console.error(`pumi: ${name}:`, String(err?.message ?? err)))
    },
    [name, key],
  )
  return [value, set]
}

/** Whether the theme drawing this window moves its layout (`animate`). */
export function useAnimate(): boolean {
  return truthy(useThemeSetting("animate")[0])
}

/** Whether a control's haptic plays, for the theme `ui_theme` names (`haptics`). */
export function hapticsOn(): boolean {
  return truthy(currentSetting("haptics"))
}
