// Where a window's view settles as the server's state moves: its own, kept while the server has
// it; the one it showed in its session last; the session's in front; or its first. Pure, for
// `windows.tsx`, and for what the windows do without a renderer (its test).

/** What a window shows now, and what it was told to show before the server listed it. */
export interface Placement {
  /** The session it shows (`$1`). */
  session?: string
  /** Its view of it (`@2`). */
  view?: string
  /** Whether the server had `session` when it was last looked at: one it stopped listing closed. */
  listed: boolean
  /** A view it was just told to show, which the snapshot has yet to list. */
  pending?: string
}

/** The server's state a window settles against: the part of the snapshot it reads. */
export interface PlacementState {
  workspaces: readonly { workspace_id: string; active_view_id?: string | null }[]
  views: readonly { view_id: string; workspace_id: string }[]
  focused_workspace_id?: string | null
}

/**
 * Where the window is after `state` arrives: it stays on its session and its view while the
 * server has them, and its view while the snapshot has yet to list it (`pending`: the server
 * made it a moment ago, and the window shows what it had until it arrives). Its session closed:
 * the one in front, or the first. Its view closed, or it shows none: the view it showed in that
 * session last, else the session's in front, else its first. Its session the snapshot has yet to
 * list: left alone until it does.
 */
export function settle(was: Placement, remembered: ReadonlyMap<string, string>, state: PlacementState): Placement {
  const sessions = state.workspaces
  const listed = was.session !== undefined && sessions.some((s) => s.workspace_id === was.session)
  let session = was.session
  let view = was.view
  let pending = was.pending
  let known = was.listed
  if (session && !listed && known) {
    // The server closed the session it showed: the one in front, or the first.
    session = state.focused_workspace_id ?? sessions[0]?.workspace_id
    view = undefined
    pending = undefined
    known = session !== undefined
  } else if (!session) {
    const first = state.focused_workspace_id ?? sessions[0]?.workspace_id
    if (first) {
      session = first
      known = true
    }
  } else if (listed) known = true
  if (!session) return { session, view, listed: known, pending }
  const views = state.views.filter((v) => v.workspace_id === session)
  // The view it was told to show is there now: it takes it, as it kept what it showed until then.
  if (pending !== undefined && views.some((v) => v.view_id === pending)) return { session, view: pending, listed: known, pending: undefined }
  if (view && views.some((v) => v.view_id === view)) return { session, view, listed: known, pending }
  // It is still waiting for a view the server made: it shows what it has until it arrives.
  if (pending !== undefined) return { session, view, listed: known, pending }
  const active = sessions.find((s) => s.workspace_id === session)?.active_view_id ?? undefined
  const last = remembered.get(session)
  view = views.find((v) => v.view_id === last)?.view_id ?? views.find((v) => v.view_id === active)?.view_id ?? views[0]?.view_id
  // A view it is still waiting for stays pending: it shows what the session has until it arrives.
  return { session, view, listed: known, pending }
}
