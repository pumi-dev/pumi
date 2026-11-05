// Views in a project's worktrees: a view is in one when its `worktree` option names the folder (the
// core starts its terminals there); without it, it is in its project's folder. Opening a worktree
// opens a view in it; a new worktree is made under `worktree_dir` and opened. Git runs on this
// machine, so only this machine's projects have them.
import { useEffect, useSyncExternalStore } from "react"
import { call, serverOf, store } from "./engine.ts"
import { openView } from "./new-view.ts"
import * as worktrees from "./worktrees.ts"
import { confirm, prompt } from "../sdk/dialogs.ts"

const strip = (path: string) => path.replace(/(.)\/+$/, "$1")

/** Whether two folders are the same one, a trailing slash aside. */
export const samePath = (a: string | undefined, b: string | undefined) => !!a && !!b && strip(a) === strip(b)

const projectOf = (project: string) => store.get().snapshot?.projects.find((w) => w.project_id === project)

/** The folder a view works in: its worktree, else its project's. */
export function folderOf(view: string): string | undefined {
  const snapshot = store.get().snapshot
  const v = snapshot?.views.find((x) => x.view_id === view)
  if (!v) return undefined
  return v.worktree ?? projectOf(v.project_id)?.path
}

/** Whether a project can have worktrees here: it is this machine's and has a folder. */
export function canHave(project: string): boolean {
  return !serverOf(project) && !!projectOf(project)?.path
}

/** The worktrees of a project's repository; none for a project that cannot have them. */
export function listFor(project: string): Promise<worktrees.Worktree[]> {
  const cwd = projectOf(project)?.path
  return canHave(project) && cwd ? worktrees.list(cwd) : Promise.resolve([])
}

/** The local branches of a project's repository that no worktree has checked out. */
export async function freeBranchesFor(project: string): Promise<string[]> {
  const cwd = projectOf(project)?.path
  if (!canHave(project) || !cwd) return []
  const [all, trees] = await Promise.all([worktrees.branches(cwd), worktrees.list(cwd)])
  const taken = new Set(trees.map((w) => w.branch).filter(Boolean))
  return all.filter((b) => !taken.has(b))
}

/**
 * A new view of `project` in `path`, one of its worktrees, brought to `window`. A view in the
 * project's own folder needs no `worktree` option: it is there already.
 */
export async function open(window: { show(project: string, view?: string): void }, project: string, path: string): Promise<void> {
  const home = projectOf(project)?.path
  const { view } = await openView(project, { cwd: path })
  window.show(project, view)
  if (samePath(home, path) || !view) return
  await call("option.set", "worktree", path, { scope: "view", id: view })
}

/** A fresh name for a new worktree of `project`'s repository: none of its branches' or folders'. */
export async function freshName(project: string): Promise<string> {
  const cwd = projectOf(project)?.path
  const [trees, branches] = cwd ? await Promise.all([worktrees.list(cwd), worktrees.branches(cwd)]) : [[], []]
  return worktrees.newName([...branches, ...trees.flatMap((w) => [worktrees.nameOf(w), worktrees.shortName(w.path)])])
}

/**
 * A new worktree of `project`'s repository on the branch `name` (made from the main checkout's
 * HEAD when there is none), opened in a view of its own. Named fresh when no name is given.
 */
export async function create(window: { show(project: string, view?: string): void }, project: string, name?: string): Promise<string> {
  const cwd = projectOf(project)?.path
  if (!cwd || serverOf(project)) throw new Error("worktrees are made for this machine's projects with a folder")
  const dir = store.get().options.worktree_dir
  const path = await worktrees.create(cwd, name?.trim() || (await freshName(project)), typeof dir === "string" ? dir : undefined)
  await open(window, project, path)
  forget(project)
  return path
}

/**
 * Asks for the new worktree's branch (a fresh name to start with), then makes it as `create`
 * does. What goes wrong is said in a dialog.
 */
export async function createAsking(window: { show(project: string, view?: string): void }, project: string): Promise<void> {
  try {
    const dir = store.get().options.worktree_dir
    const name = await prompt({
      title: "New Worktree",
      detail: `A new branch, checked out in ${typeof dir === "string" && dir ? dir : worktrees.DEFAULT_DIR}.`,
      value: await freshName(project),
      placeholder: "Branch name",
      confirm: "Create",
    })
    if (name?.trim()) await create(window, project, name)
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

// A project's worktrees as last read, by project, for what draws them: read again when asked for
// after `STALE_MS`, and when one is made here.
const known = new Map<string, { at: number; list: worktrees.Worktree[] }>()
const listeners = new Set<() => void>()
const STALE_MS = 5000
const NO_WORKTREES: worktrees.Worktree[] = []

function refresh(project: string) {
  const was = known.get(project)
  if (was && Date.now() - was.at < STALE_MS) return
  known.set(project, { at: Date.now(), list: was?.list ?? NO_WORKTREES })
  listFor(project).then(
    (list) => {
      const was = known.get(project)?.list
      const same = JSON.stringify(was) === JSON.stringify(list)
      known.set(project, { at: Date.now(), list: same && was ? was : list })
      if (!same) for (const l of listeners) l()
    },
    (err) => console.error(`pumi: worktrees of ${project}: ${err?.message ?? err}`),
  )
}

/** `project`'s worktrees as last read: none before they are (`useWorktrees` reads them). */
export function cached(project: string): worktrees.Worktree[] {
  return known.get(project)?.list ?? NO_WORKTREES
}

/** Reads `project`'s worktrees again the next time they are asked for. */
export function forget(project: string) {
  known.delete(project)
}

/**
 * `project`'s worktrees, the main checkout first: none until they are read, and none for a
 * project not in a repository. While `live`, they are read (again, once stale).
 */
export function useWorktrees(project: string, live = true): worktrees.Worktree[] {
  useEffect(() => {
    if (live && canHave(project)) refresh(project)
  }, [project, live])
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => void listeners.delete(l)
    },
    () => known.get(project)?.list ?? NO_WORKTREES,
  )
}
