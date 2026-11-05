// The update toast: a card in the window's bottom right corner while pumi's update is ready or
// plugins have new ones (`update-notices.ts`), and for a moment when a check a person asked for is
// answered either way ("pumi 0.48.0 is up to date"), each dismissed on its own. pumi's has a
// "What's new" button beside Restart, opening its release page in the system's browser. The app
// draws it over any theme's window, in the theme's colours; a theme does not replace it.
import { useEffect, useState } from "react"
import { Icon } from "../sdk/elements.tsx"
import { useMetrics, useTheme } from "../sdk/theme.ts"
import { useTextSize } from "../sdk/settings/controls.tsx"
import { engine } from "./engine.ts"
import { useLocal } from "./local.ts"
import { openSettings } from "./settings/open.ts"
import { openPath, useUpdates } from "./settings/shared.tsx"
import { answerNotice, updateNotices, type Dismissed, type UpdateNotice } from "./update-notices.ts"

const NONE: Dismissed = {}
/** How long a check's answer stays: enough to read it (a found update's toast stays until applied). */
const ANSWERED_MS = 8000

export function UpdateToasts() {
  const updates = useUpdates()
  const [dismissed, setDismissed] = useLocal<Dismissed>("updateToast.dismissed", NONE)
  const [answer, setAnswer] = useState<UpdateNotice | null>(null)
  // A check a person asked for (the menu's Check for Updates, Settings' button): what came of it is
  // said even when nothing did, then taken away — the state it describes outlives it in Settings.
  useEffect(() => {
    const said = answerNotice(updates)
    if (!said) return
    setAnswer(said)
    const timer = setTimeout(() => setAnswer(null), ANSWERED_MS)
    return () => clearTimeout(timer)
  }, [updates.seq])
  const notices = updateNotices(updates, dismissed)
  if (!notices.length && !answer) return null
  return (
    <anchored side="bottom" align="end" anchor="bottomRight" offset={{ x: -12, y: -12 }} deferred priority={0} style={{ backgroundColor: "transparent" }}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 8 }}>
        {notices.map((n) => (
          <Toast key={n.kind} notice={n} onDismiss={() => setDismissed((was) => ({ ...was, [n.kind]: n.key }))} />
        ))}
        {answer ? <Toast key="check" notice={answer} onDismiss={() => setAnswer(null)} /> : null}
      </div>
    </anchored>
  )
}

function act(notice: UpdateNotice) {
  if (notice.kind === "app") engine.restartToUpdate()
  else if (notice.kind === "plugins") openSettings("plugins")
}

function Toast({ notice, onDismiss }: { notice: UpdateNotice; onDismiss(): void }) {
  const t = useTheme()
  const m = useMetrics()
  const { link } = notice
  return (
    <div
      role="status"
      style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, width: link ? 380 : 320, paddingTop: 10, paddingBottom: 10, paddingLeft: 12, paddingRight: 8, borderRadius: m.radius + 4, backgroundColor: t.surface, borderWidth: 1, borderColor: t.border, boxShadow: { offsetX: 0, offsetY: 8, blurRadius: 24, spreadRadius: 0, color: "#00000060" } }}
    >
      <Words notice={notice} />
      {link ? <ToastButton label={link.label} onPress={() => openPath(link.url)} /> : null}
      {notice.action ? <ToastButton label={notice.action} primary onPress={() => act(notice)} /> : null}
      <div
        aria-label="Dismiss"
        title="Dismiss"
        tabIndex={0}
        onClick={onDismiss}
        onKeyDown={(e: any) => {
          if (e.key !== "enter" && e.key !== " ") return
          e.preventDefault?.()
          onDismiss()
        }}
        style={{ display: "flex", alignItems: "center", justifyContent: "center", width: 22, height: 22, flexShrink: 0, borderRadius: m.radiusSmall, cursor: "pointer", keyboardFocusDim: false, hover: { backgroundColor: t.hover } }}
      >
        <Icon name="close" size={12} color={t.muted} />
      </div>
    </div>
  )
}

function Words({ notice }: { notice: UpdateNotice }) {
  const ts = useTextSize()
  const t = useTheme()
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2, flexGrow: 1, flexShrink: 1, minWidth: 0, marginRight: 2 }}>
      <text style={{ fontSize: ts(13), fontWeight: 600, color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis" }}>{notice.title}</text>
      <text style={{ fontSize: ts(12), color: t.muted }}>{notice.detail}</text>
    </div>
  )
}

/** One of a toast's buttons: the action, in the accent colour (`primary`), or a quieter one beside it. */
function ToastButton({ label, primary, onPress }: { label: string; primary?: boolean; onPress(): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  return (
    <div
      aria-label={label}
      tabIndex={0}
      onClick={onPress}
      onKeyDown={(e: any) => {
        if (e.key !== "enter" && e.key !== " ") return
        e.preventDefault?.()
        onPress()
      }}
      style={{ display: "flex", alignItems: "center", justifyContent: "center", height: 26, flexShrink: 0, paddingLeft: 12, paddingRight: 12, borderRadius: m.radiusSmall, cursor: "pointer", keyboardFocusDim: false, ...(primary ? { backgroundColor: t.accent, hover: { opacity: 0.9 } } : { backgroundColor: t.active, hover: { backgroundColor: t.hover } }) }}
    >
      <text style={{ fontSize: ts(12), fontWeight: 500, color: primary ? t.accentText : t.text, whiteSpace: "nowrap" }}>{label}</text>
    </div>
  )
}
