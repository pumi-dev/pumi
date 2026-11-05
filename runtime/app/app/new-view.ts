// A new view in a project, with the tab it opens with: a view is what a window shows, so whatever
// makes one shows it there (`window.show(project, view)`) — ⌘T, a window of its own, a worktree.
import { call, idOn, serverOf, via } from "./engine.ts"

/** Opens a view in `project` (the caller's without one), with a tab in it, and answers the view,
 *  the project it is in and that tab. Where the tab goes is `place` (a worktree's folder, say). */
export async function openView(project?: string, place: Record<string, unknown> = {}): Promise<{ project?: string; view?: string; tab: string }> {
  const tab = idOn(serverOf(project), await call(via(project, "tab.open"), null, place, { kind: "view", ...(project ? { project } : {}) }))
  const made = await call(via(project, "tab.get"), tab)
  const view: string | undefined = typeof made?.view === "string" ? made.view : undefined
  const madeIn: string | undefined = project ?? (typeof made?.project === "string" ? made.project : undefined)
  return { project: madeIn, view, tab }
}
