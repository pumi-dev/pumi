// What macOS asks the app to open: files and folders dropped on its Dock icon, or opened with it
// (Finder's "Open With", `open -a pumi PATH`). Each goes to the session it belongs to, shown in the
// window in front: a folder to the session whose directory it is; a file to the session with the
// deepest directory holding it (the shown one, of equals), beside that session's current tab.
// Without one, a new session: at the folder, or at the file's repository (else its folder).
import { existsSync, statSync } from "node:fs"
import { basename } from "node:path"
import { pathOf, repoOf, route } from "./open-route.ts"
import { call, engine, on, serverOf, store } from "./engine.ts"
import type { WindowManager } from "./windows.tsx"

/** A session's current tab, as the server has it now (one made a moment ago included). */
async function currentTab(session: string): Promise<string | undefined> {
  const info = await call("session.get", session)
  if (info?.activeView == null) return undefined
  const view = await call("view.get", info.activeView)
  return typeof view?.activeTab === "string" ? view.activeTab : undefined
}

/** Opens what macOS asked to, in turn: a session made for one may be where the next goes. */
async function openAll(manager: WindowManager, urls: string[]) {
  const paths = urls.map(pathOf).filter((p): p is string => !!p && existsSync(p))
  if (!paths.length) return
  if (!manager.windows.length) manager.open()
  const window = manager.windows.find((w) => w.active) ?? manager.windows[0]!
  /** The sessions made here, by directory: the snapshot may not list them yet. */
  const made = new Map<string, string>()
  for (const path of paths) {
    try {
      const folder = statSync(path).isDirectory()
      const local = (store.get().snapshot?.workspaces ?? []).filter((w) => !serverOf(w.workspace_id))
      const sessions = [...local.map((w) => ({ id: w.workspace_id, cwd: w.cwd })), ...[...made].map(([cwd, id]) => ({ id, cwd }))]
      const where = route(path, folder, sessions, window.session, repoOf)
      let session: string
      if ("session" in where) session = where.session
      else {
        // A session of its own, with a terminal in it: `tab.open` answers the tab, and its
        // `session` is the one just made.
        const tab = await call("tab.open", "terminal", {}, { kind: "session", cwd: where.create, name: basename(where.create) || where.create })
        const opened = await call("tab.get", tab)
        session = opened.session
        made.set(where.create, session)
      }
      if (!folder) {
        const beside = await currentTab(session)
        await call("tab.open", "file", { path }, { kind: "tiled", split: "right", ...(beside === undefined ? {} : { beside }) })
      }
      window.show(session)
    } catch (err) {
      console.error(`pumi: opening ${path}:`, (err as Error)?.message ?? err)
    }
  }
  window.renderer.activateWindow()
}

/**
 * Opens what macOS asked to, now and each time it asks again (an `app` event with `opened`).
 * Once the server's state is here: what came before waits in the engine (`Engine.takeOpened`).
 */
export function start(manager: WindowManager) {
  let queue = Promise.resolve()
  const drain = () => {
    queue = queue.then(() => openAll(manager, engine.takeOpened())).catch((err) => console.error("pumi: opening:", err))
  }
  on("app", (data: { opened?: boolean }) => {
    if (data?.opened) drain()
  })
  drain()
}
