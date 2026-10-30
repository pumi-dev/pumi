// What the update toast says, from the updater's state (`Engine.updateState`): pumi's own update
// once it is downloaded, and the plugins that have a new version. Each has a `key` naming what it
// tells of (the version, the plugins and theirs), so a dismissed one stays hidden until there is
// something newer to tell.

/** The part of `UpdateState` (settings/shared.tsx) the notices read. */
export interface UpdatesSeen {
  app: { status: string; version?: string | null; notes?: string | null }
  plugins: { found: { name: string; from?: string; to?: string }[] | null }
}

export interface UpdateNotice {
  kind: "app" | "plugins"
  /** What it tells of: dismissing it hides it until this changes. */
  key: string
  title: string
  detail: string
  action: string
  /** A page to read about it (pumi's release notes), with the button's label. */
  link?: { label: string; url: string }
}

/** The keys last dismissed, by kind. */
export type Dismissed = Partial<Record<UpdateNotice["kind"], string>>

export function updateNotices(state: UpdatesSeen, dismissed: Dismissed): UpdateNotice[] {
  const notices: UpdateNotice[] = []
  const { app, plugins } = state
  if (app.status === "ready" && app.version) {
    const link = app.notes ? { link: { label: "What's new", url: app.notes } } : {}
    notices.push({ kind: "app", key: app.version, title: `pumi ${app.version} is ready`, detail: "Restart to update.", action: "Restart", ...link })
  }
  const found = plugins.found ?? []
  if (found.length) {
    const key = found
      .map((p) => `${p.name}@${p.to ?? ""}`)
      .sort()
      .join(",")
    const title = found.length === 1 ? `${found[0].name} has an update` : `${found.length} plugins have updates`
    notices.push({ kind: "plugins", key, title, detail: "Update them in Settings.", action: "Review" })
  }
  return notices.filter((n) => dismissed[n.kind] !== n.key)
}
