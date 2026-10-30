// A new view in a session, with the tab it opens with: a view is what a window shows, so whatever
// makes one shows it there (`window.show(session, view)`) — ⌘T, a window of its own, a worktree.
import { call, idOn, serverOf, via } from "./engine.ts"

/** Opens a view in `session` (the caller's without one), with a tab in it, and answers the view,
 *  the session it is in and that tab. Where the tab goes is `place` (a worktree's folder, say). */
export async function openView(session?: string, place: Record<string, unknown> = {}): Promise<{ session?: string; view?: string; tab: string }> {
  const tab = idOn(serverOf(session), await call(via(session, "tab.open"), null, place, { kind: "view", ...(session ? { session } : {}) }))
  const made = await call(via(session, "tab.get"), tab)
  const view: string | undefined = typeof made?.view === "string" ? made.view : undefined
  const madeIn: string | undefined = session ?? (typeof made?.session === "string" ? made.session : undefined)
  return { session: madeIn, view, tab }
}
