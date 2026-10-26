// Views in a session's worktrees: a view is in one when its `worktree` option names the folder (the
// core starts its terminals there); without it, it is in its session's folder. Opening a worktree
// opens a view in it; a new worktree is made under `worktree_dir` and opened. Git runs on this
// machine, so only this machine's sessions have them.
import { useEffect, useSyncExternalStore } from "react"
import { call, serverOf, store } from "./engine.ts"
import * as worktrees from "./worktrees.ts"
import { confirm, prompt } from "../sdk/dialogs.ts"

const strip = (path: string) => path.replace(/(.)\/+$/, "$1")

/** Whether two folders are the same one, a trailing slash aside. */
export const samePath = (a: string | undefined, b: string | undefined) => !!a && !!b && strip(a) === strip(b)

const sessionOf = (session: string) => store.get().snapshot?.workspaces.find((w) => w.workspace_id === session)

/** The folder a view works in: its worktree, else its session's. */
export function folderOf(view: string): string | undefined {
  const snapshot = store.get().snapshot
  const v = snapshot?.views.find((x) => x.view_id === view)
  if (!v) return undefined
  return v.worktree ?? sessionOf(v.workspace_id)?.cwd
}

/** Whether a session can have worktrees here: it is this machine's and has a folder. */
export function canHave(session: string): boolean {
  return !serverOf(session) && !!sessionOf(session)?.cwd
}

/** The worktrees of a session's repository; none for a session that cannot have them. */
export function listFor(session: string): Promise<worktrees.Worktree[]> {
  const cwd = sessionOf(session)?.cwd
  return canHave(session) && cwd ? worktrees.list(cwd) : Promise.resolve([])
}

/** The local branches of a session's repository that no worktree has checked out. */
export async function freeBranchesFor(session: string): Promise<string[]> {
  const cwd = sessionOf(session)?.cwd
  if (!canHave(session) || !cwd) return []
  const [all, trees] = await Promise.all([worktrees.branches(cwd), worktrees.list(cwd)])
  const taken = new Set(trees.map((w) => w.branch).filter(Boolean))
  return all.filter((b) => !taken.has(b))
}

/**
 * A new view of `session` in `path`, one of its worktrees, brought to `window`. A view in the
 * session's own folder needs no `worktree` option: it is there already.
 */
export async function open(window: { session?: string; show(session: string): void }, session: string, path: string): Promise<void> {
  const home = sessionOf(session)?.cwd
  const tab: string = await call("tab.open", "terminal", { cwd: path }, { kind: "view", session })
  if (window.session !== session) window.show(session)
  if (samePath(home, path)) return
  const { view } = await call("tab.get", tab)
  if (typeof view === "string") await call("option.set", "worktree", path, { scope: "view", id: view })
}

/** A fresh name for a new worktree of `session`'s repository: none of its branches' or folders'. */
export async function freshName(session: string): Promise<string> {
  const cwd = sessionOf(session)?.cwd
  const [trees, branches] = cwd ? await Promise.all([worktrees.list(cwd), worktrees.branches(cwd)]) : [[], []]
  return worktrees.newName([...branches, ...trees.flatMap((w) => [worktrees.nameOf(w), worktrees.shortName(w.path)])])
}

/**
 * A new worktree of `session`'s repository on the branch `name` (made from the main checkout's
 * HEAD when there is none), opened in a view of its own. Named fresh when no name is given.
 */
export async function create(window: { session?: string; show(session: string): void }, session: string, name?: string): Promise<string> {
  const cwd = sessionOf(session)?.cwd
  if (!cwd || serverOf(session)) throw new Error("worktrees are made for this machine's sessions with a folder")
  const dir = store.get().options.worktree_dir
  const path = await worktrees.create(cwd, name?.trim() || (await freshName(session)), typeof dir === "string" ? dir : undefined)
  await open(window, session, path)
  forget(session)
  return path
}

/**
 * Asks for the new worktree's branch (a fresh name to start with), then makes it as `create`
 * does. What goes wrong is said in a dialog.
 */
export async function createAsking(window: { session?: string; show(session: string): void }, session: string): Promise<void> {
  try {
    const dir = store.get().options.worktree_dir
    const name = await prompt({
      title: "New Worktree",
      detail: `A new branch, checked out in ${typeof dir === "string" && dir ? dir : worktrees.DEFAULT_DIR}.`,
      value: await freshName(session),
      placeholder: "Branch name",
      confirm: "Create",
    })
    if (name?.trim()) await create(window, session, name)
  } catch (err) {
    report(err)
  }
}

/** Says what went wrong where the user sees it. */
export function report(err: unknown) {
  const message = err instanceof Error ? err.message : String(err)
  console.error(`pumi: worktree: ${message}`)
  confirm({ title: "Worktree", detail: message, confirm: "OK", cancel: "Close" }).catch(() => {})
}

// A session's worktrees as last read, by session, for what draws them: read again when asked for
// after `STALE_MS`, and when one is made here.
const known = new Map<string, { at: number; list: worktrees.Worktree[] }>()
const listeners = new Set<() => void>()
const STALE_MS = 5000
const NO_WORKTREES: worktrees.Worktree[] = []

function refresh(session: string) {
  const was = known.get(session)
  if (was && Date.now() - was.at < STALE_MS) return
  known.set(session, { at: Date.now(), list: was?.list ?? NO_WORKTREES })
  listFor(session).then(
    (list) => {
      const was = known.get(session)?.list
      const same = JSON.stringify(was) === JSON.stringify(list)
      known.set(session, { at: Date.now(), list: same && was ? was : list })
      if (!same) for (const l of listeners) l()
    },
    (err) => console.error(`pumi: worktrees of ${session}: ${err?.message ?? err}`),
  )
}

/** `session`'s worktrees as last read: none before they are (`useWorktrees` reads them). */
export function cached(session: string): worktrees.Worktree[] {
  return known.get(session)?.list ?? NO_WORKTREES
}

/** Reads `session`'s worktrees again the next time they are asked for. */
export function forget(session: string) {
  known.delete(session)
}

/**
 * `session`'s worktrees, the main checkout first: none until they are read, and none for a
 * session not in a repository. While `live`, they are read (again, once stale).
 */
export function useWorktrees(session: string, live = true): worktrees.Worktree[] {
  useEffect(() => {
    if (live && canHave(session)) refresh(session)
  }, [session, live])
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => void listeners.delete(l)
    },
    () => known.get(session)?.list ?? NO_WORKTREES,
  )
}
