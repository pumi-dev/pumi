// The settings kit's layout: a page, its sections (a heading over a card), and the rows in them
// (a title and what it means on the left, its control on the right).
import { useEffect, useRef, type ReactNode } from "react"
import type { PublicInstance } from "../../react/types/host.ts"
import { Icon } from "../elements.tsx"
import { useMetrics, useTheme, withOpacity } from "../theme.ts"
import { useSettings } from "./context.ts"
import { present, useTextSize } from "./controls.tsx"

/** Scrolls the element into view while a link or a search names `name`; true while it does. */
export function useReveal(name: string | undefined): [boolean, (el: PublicInstance | null) => void] {
  const settings = useSettings()
  const marked = !!name && settings?.highlight === name
  const el = useRef<PublicInstance | null>(null)
  useEffect(() => {
    if (!marked) return
    // After the page has laid out: the row's place is known then.
    const timer = setTimeout(() => el.current?.scrollIntoView?.(), 0)
    return () => clearTimeout(timer)
  }, [marked])
  return [marked, (value) => (el.current = value)]
}

export interface PageProps {
  title?: string
  description?: string
  children?: ReactNode
  /** It fills the window's height, for a page whose content scrolls itself (a grid of cards). */
  fill?: boolean
}

/** A page: its title, what it is for, and its sections. */
export function Page({ title, description, children, fill }: PageProps) {
  const t = useTheme()
  const ts = useTextSize()
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20, width: "100%", ...(fill ? { flexGrow: 1, minHeight: 0 } : {}) }}>
      {title || description ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {title ? <text role="heading" aria-level={1} style={{ fontSize: ts(18), fontWeight: 600, color: t.text }}>{title}</text> : null}
          {description ? <text style={{ fontSize: ts(13), color: t.muted }}>{description}</text> : null}
        </div>
      ) : null}
      {children}
    </div>
  )
}

export interface SectionProps {
  /** What a link names it by (a built-in one's; a plugin's is its manifest id). */
  id?: string
  title?: string
  description?: string
  /** One of the app's icons, before the title. */
  icon?: string
  /** Drawn at the heading's end: a button, a menu. */
  actions?: ReactNode
  /** Rows (`Row`, `OptionRow`) and anything else; a rule goes between each. */
  children?: ReactNode
  /** Its children as they are, without the card round them. */
  bare?: boolean
}

/** A heading over a card of rows. */
export function Section({ id, title, description, icon, actions, children, bare }: SectionProps) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const [marked, reveal] = useReveal(id)
  const rows = present(children)
  return (
    <div ref={reveal} style={{ display: "flex", flexDirection: "column", gap: 10, width: "100%" }}>
      {title || actions ? (
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, minWidth: 0 }}>
            {icon ? <Icon name={icon} size={14} color={t.muted} /> : null}
            {title ? <text role="heading" aria-level={2} style={{ fontSize: ts(15), fontWeight: 600, color: marked ? t.accent : t.text }}>{title}</text> : null}
          </div>
          {actions ? <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>{actions}</div> : null}
        </div>
      ) : null}
      {description ? <text style={{ fontSize: ts(13), color: t.muted, marginTop: -4 }}>{description}</text> : null}
      {bare ? (
        rows
      ) : rows.length ? (
        <div style={{ display: "flex", flexDirection: "column", paddingLeft: 16, paddingRight: 16, borderRadius: m.radius + 6, borderWidth: 1, borderColor: marked ? t.accent : t.border, backgroundColor: t.surface }}>
          {rows.map((row, at) => (
            <div key={at} style={{ display: "flex", flexDirection: "column", ...(at < rows.length - 1 ? { borderBottomWidth: 1, borderColor: t.border } : {}) }}>
              {row}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  )
}

export interface RowProps {
  title: ReactNode
  description?: ReactNode
  /** Before the title: one of the app's icons by name, or an element (`<img>`, a status dot). */
  icon?: string | ReactNode
  /** At the row's end: a switch, a button, a field. */
  control?: ReactNode
  /** After the title, quieter: a version, an owner. */
  aside?: string
  /** The option or the thing it is, for links and search: its row is highlighted while named. */
  link?: string
  /** Under the row, full width (a strip of cards). */
  children?: ReactNode
  testId?: string
}

/** One setting: its title and what it means, and its control. */
export function Row({ title, description, icon, control, aside, link, children, testId }: RowProps) {
  const t = useTheme()
  const ts = useTextSize()
  const [marked, reveal] = useReveal(link)
  return (
    <div ref={reveal} testId={testId} style={{ display: "flex", flexDirection: "column", gap: 10, paddingTop: 12, paddingBottom: 12, ...(marked ? { marginLeft: -16, marginRight: -16, paddingLeft: 16, paddingRight: 16, backgroundColor: withOpacity(t.accent, 0.12) } : {}) }}>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 24, minHeight: 32 }}>
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 12, flexGrow: 1, flexShrink: 1, minWidth: 160 }}>
          {typeof icon === "string" ? <Icon name={icon} size={18} color={t.muted} /> : icon ?? null}
          <div style={{ display: "flex", flexDirection: "column", gap: 3, flexGrow: 1, minWidth: 0 }}>
            <div style={{ display: "flex", flexDirection: "row", alignItems: "baseline", flexWrap: "wrap", gap: 6 }}>
              {typeof title === "string" ? <text style={{ fontSize: ts(14), color: t.text }}>{title}</text> : title}
              {aside ? <text style={{ fontSize: ts(12), color: t.faint }}>{aside}</text> : null}
            </div>
            {description ? typeof description === "string" ? <text style={{ fontSize: ts(13), color: t.muted }}>{description}</text> : description : null}
          </div>
        </div>
        {control ? <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, flexShrink: 0, minWidth: 0 }}>{control}</div> : null}
      </div>
      {children}
    </div>
  )
}

/** A light: green when something works, red when it does not. */
export function StatusDot({ color }: { color: string }) {
  return <div style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: color, flexShrink: 0 }} />
}

/** A line of quiet text: a note under a section, a message. */
export function Note({ children, tone }: { children: ReactNode; tone?: "warning" | "error" }) {
  const t = useTheme()
  const ts = useTextSize()
  return <text style={{ fontSize: ts(13), color: tone === "error" ? t.error : tone === "warning" ? t.warning : t.muted }}>{children}</text>
}
