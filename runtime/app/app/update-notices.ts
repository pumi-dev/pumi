// What the update toast says, from the updater's state (`Engine.updateState`): pumi's own update
// once it is downloaded, the plugins that have a new version, and the answer to a check a person
// asked for — which says so either way, "pumi 0.48.0 is up to date" as much as an update found.
// Each has a `key` naming what it tells of (the version, the plugins and theirs), so a dismissed
// one stays hidden until there is something newer to tell.

/** The part of `UpdateState` (settings/shared.tsx) the notices read. */
export interface UpdatesSeen {
  app: {
    status: string
    version?: string | null
    notes?: string | null
    /** The version running, and the channel it came from: said when nothing newer exists. */
    current?: string
    channel?: string
    /** Why it could not look, where it could not. */
    error?: string | null
  }
  plugins: { found: { name: string; from?: string; to?: string }[] | null }
  /** This state answers a check a person asked for (the menu item, Settings' button). */
  manual?: boolean
}

export interface UpdateNotice {
  kind: "app" | "plugins" | "check"
  /** What it tells of: dismissing it hides it until this changes. */
  key: string
  title: string
  detail: string
  /** The button that does what the notice is about; none for a notice that only says. */
  action?: string
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

/**
 * The updater's answer to a check a person asked for (`manual`), whether it found something or not:
 * "pumi 0.48.0 is up to date" where nothing newer exists, and the reason where it could not look
 * (a development build) or the check failed. Nothing while a check runs or an update downloads:
 * `updateNotices` says when one is ready, and a found update keeps that toast until it is applied.
 */
export function answerNotice(state: UpdatesSeen): UpdateNotice | undefined {
  const { app } = state
  if (!state.manual) return undefined
  switch (app.status) {
    case "upToDate":
      return {
        kind: "check",
        key: "upToDate",
        title: app.current ? `pumi ${app.current} is up to date` : "pumi is up to date",
        detail: app.channel ? `That is the newest version on the ${app.channel} channel.` : "That is the newest version.",
      }
    case "failed":
      return { kind: "check", key: "failed", title: "Could not check for updates", detail: app.error ?? "The check did not finish." }
    case "found":
      return {
        kind: "check",
        key: "found",
        title: `pumi ${app.version} is out`,
        detail: app.error ? `This copy does not install it: ${app.error}.` : "This copy does not install updates.",
      }
    case "unavailable":
      return { kind: "check", key: "unavailable", title: "This copy does not update itself", detail: app.error ?? "A development build, or an install it cannot write beside." }
    default:
      return undefined
  }
}
