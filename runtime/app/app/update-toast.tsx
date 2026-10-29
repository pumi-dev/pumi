// The update toast: a card in the window's bottom right corner while pumi's update is ready or
// plugins have new ones (`update-notices.ts`), each dismissed on its own. The app draws it over
// any theme's window, in the theme's colours; a theme does not replace it.
import { Icon } from "../sdk/elements.tsx"
import { useMetrics, useTheme } from "../sdk/theme.ts"
import { useTextSize } from "../sdk/settings/controls.tsx"
import { engine } from "./engine.ts"
import { useLocal } from "./local.ts"
import { useUpdates } from "./settings/shared.tsx"
import { openSettings } from "./settings/open.ts"
import { updateNotices, type Dismissed, type UpdateNotice } from "./update-notices.ts"

const NONE: Dismissed = {}

export function UpdateToasts() {
  const updates = useUpdates()
  const [dismissed, setDismissed] = useLocal<Dismissed>("updateToast.dismissed", NONE)
  const notices = updateNotices(updates, dismissed)
  if (!notices.length) return null
  return (
    <anchored side="bottom" align="end" anchor="bottomRight" offset={{ x: -12, y: -12 }} deferred priority={0} style={{ backgroundColor: "transparent" }}>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {notices.map((n) => (
          <Toast key={n.kind} notice={n} onDismiss={() => setDismissed((was) => ({ ...was, [n.kind]: n.key }))} />
        ))}
      </div>
    </anchored>
  )
}

function act(notice: UpdateNotice) {
  if (notice.kind === "app") engine.restartToUpdate()
  else openSettings("plugins")
}

function Toast({ notice, onDismiss }: { notice: UpdateNotice; onDismiss(): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  return (
    <div
      role="status"
      style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10, width: 320, paddingTop: 10, paddingBottom: 10, paddingLeft: 12, paddingRight: 8, borderRadius: m.radius + 4, backgroundColor: t.surface, borderWidth: 1, borderColor: t.border, boxShadow: { offsetX: 0, offsetY: 8, blurRadius: 24, spreadRadius: 0, color: "#00000060" } }}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: 2, flexGrow: 1, flexShrink: 1, minWidth: 0 }}>
        <text style={{ fontSize: ts(13), fontWeight: 600, color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis" }}>{notice.title}</text>
        <text style={{ fontSize: ts(12), color: t.muted }}>{notice.detail}</text>
      </div>
      <div
        aria-label={notice.action}
        tabIndex={0}
        onClick={() => act(notice)}
        onKeyDown={(e: any) => {
          if (e.key !== "enter" && e.key !== " ") return
          e.preventDefault?.()
          act(notice)
        }}
        style={{ display: "flex", alignItems: "center", justifyContent: "center", height: 26, flexShrink: 0, paddingLeft: 12, paddingRight: 12, borderRadius: m.radiusSmall, cursor: "pointer", backgroundColor: t.accent, keyboardFocusDim: false, hover: { opacity: 0.9 } }}
      >
        <text style={{ fontSize: ts(12), fontWeight: 500, color: t.accentText, whiteSpace: "nowrap" }}>{notice.action}</text>
      </div>
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
