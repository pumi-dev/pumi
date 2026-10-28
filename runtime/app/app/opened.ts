// What macOS asks the app to open: files and folders dropped on its Dock icon, or opened with it
// (Finder's "Open With", `open -a pumi PATH`). Each goes to the session it belongs to, shown in the
// window in front: a folder to the session whose directory it is; a file to the session with the
// deepest directory holding it (the shown one, of equals), beside that session's current tab.
// Without one, a new session: at the folder, or at the file's repository (else its folder).
// A plugin's install link (`pumi://plugins/install/…`, `install-link.ts`) asks, then installs it.
import { existsSync, statSync } from "node:fs"
import { basename } from "node:path"
import { confirm } from "../sdk/dialogs.ts"
import { installedOf, listingOf, parseInstallLink, questionOf, type InstallLink, type Listing } from "./install-link.ts"
import { pathOf, repoOf, route } from "./open-route.ts"
import { call, engine, on, serverOf, store } from "./engine.ts"
import { openSettings } from "./settings/open.ts"
import type { WindowManager } from "./windows.tsx"

/** A session's current tab, as the server has it now (one made a moment ago included). */
async function currentTab(session: string): Promise<string | undefined> {
  const info = await call("session.get", session)
  if (info?.activeView == null) return undefined
  const view = await call("view.get", info.activeView)
  return typeof view?.activeTab === "string" ? view.activeTab : undefined
}

/** The window in front, one opened when there is none, brought to the front. */
function front(manager: WindowManager) {
  if (!manager.windows.length) manager.open()
  const window = manager.windows.find((w) => w.active) ?? manager.windows[0]!
  window.renderer.activateWindow()
  return window
}

/** Asks to install what `link` names, then installs it and says how that went. */
async function install(manager: WindowManager, link: InstallLink) {
  front(manager)
  const [listings, installed] = await Promise.all([
    call("registry.list", "plugin", { available: true }).catch(() => []) as Promise<Listing[]>,
    call("registry.list", "plugin").catch(() => []) as Promise<any[]>,
  ])
  const listing = listingOf(link, listings ?? [])
  const name = listing?.name ?? link.name
  if (installedOf(link, installed ?? [], listing)) {
    if (await confirm({ title: `${name} is installed already`, detail: "Its settings are in Settings → Plugins.", confirm: "Open Settings", cancel: "Done" })) openSettings("plugins")
    return
  }
  if (!(await confirm(questionOf(link, listing)))) return
  for (;;) {
    try {
      const id: string = await call("plugin.add", link.source, {})
      if (await confirm({ title: `Installed ${id}`, detail: "Its settings are in Settings → Plugins.", confirm: "Open Settings", cancel: "Done" })) openSettings("plugins")
      return
    } catch (err) {
      const why = String((err as Error)?.message ?? err)
      console.error(`pumi: installing ${link.source}:`, why)
      if (!(await confirm({ title: `Could not install ${name}`, detail: why, confirm: "Try again", cancel: "Close" }))) return
    }
  }
}

/** Opens what macOS asked to, in turn: a session made for one may be where the next goes. */
async function openAll(manager: WindowManager, urls: string[]) {
  for (const url of urls) {
    const link = parseInstallLink(url)
    if (link) await install(manager, link).catch((err) => console.error(`pumi: installing ${link.source}:`, err))
  }
  const paths = urls.map(pathOf).filter((p): p is string => !!p && existsSync(p))
  if (!paths.length) return
  const window = front(manager)
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
