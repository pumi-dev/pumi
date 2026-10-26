// Elements a plugin draws with that are not the renderer's: an icon by name (the app's own set),
// and the rows of a list.
import { assets } from "../app/assets.ts"
import { useComponents, useTheme, type IconButtonProps } from "./theme.ts"

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
