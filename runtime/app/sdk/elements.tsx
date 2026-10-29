// Elements a plugin draws with that are not the renderer's: an icon by name (the app's own set),
// and the rows of a list.
import { assets } from "../app/assets.ts"
import type { IconSource } from "./icons.ts"
import { useComponents, useTheme, type IconButtonProps, type TabIconProps } from "./theme.ts"

export interface IconProps {
  /** A name of the app's icons: `folder`, `git_branch`, `check`, `close`, `search`, `agent`, … */
  name: string
  size?: number
  /** A colour, from `useTheme()`. */
  color?: string
  style?: Record<string, unknown>
}

/** An icon of the app's set, tinted by `color`. */
export function Icon({ name, size = 14, color, style }: IconProps) {
  const src = assets.icon(name)
  if (!src) return null
  return <svg src={src} style={{ width: size, height: size, flexShrink: 0, ...(color ? { color } : {}), ...style }} />
}

/** An icon `iconSource` resolved (`useTabIcon`'s): tinted by `color` if it takes the theme's colour, else in its own. */
export function IconImage({ icon, size = 14, color }: { icon: IconSource; size?: number; color?: string }) {
  if (!icon.src) return null
  if (icon.tint) return <svg src={icon.src} style={{ width: size, height: size, flexShrink: 0, ...(color ? { color } : {}) }} />
  return <img src={icon.src} objectFit="contain" style={{ width: size, height: size, flexShrink: 0 }} />
}

/** A tab's icon, as the theme's `TabIcon` part draws it: its own, its agent's or its block's, or its status. */
export function TabIcon(props: TabIconProps) {
  const C = useComponents()
  return <C.TabIcon {...props} />
}

/**
 * A button with one of the app's icons, drawn by the theme's `IconButton` part: in a tab's title
 * bar (`tab.toolbar`) it is one of the tab's own buttons, under whichever theme draws them.
 */
export function IconButton(props: IconButtonProps) {
  const C = useComponents()
  return <C.IconButton {...props} />
}

/** How an agent's status is drawn: its colour and an icon of the app's set. */
export function useAgentStatus(status: string | undefined): { color: string; icon: string } {
  const t = useTheme()
  return { color: t[`agent.${status || "unknown"}`] ?? t["agent.unknown"], icon: `status_${status && status !== "unknown" ? status : "unknown"}` }
}

/**
 * What the window's elements can do that an older app's cannot, for a plugin that runs in both: it
 * reads them off the module (`import * as pumi from "pumi"`, `pumi.features?.inlineText`), since a
 * name an older app lacks fails the import.
 * - `inlineText`: `<text inline>`, a paragraph of styled runs.
 * - `textInlines`: a text field's `inlines` (nodes its children draw) and `caret`, and the selection
 *   on its `change` and `select`.
 */
export const features = { inlineText: true, textInlines: true } as const
