// What macOS asks the app to open: files and folders dropped on its Dock icon, or opened with it
// (Finder's "Open With", `open -a pumi PATH`). Each goes to the project it belongs to, shown in the
// window in front: a folder to the project whose directory it is; a file to the project with the
// deepest directory holding it (the shown one, of equals), beside that project's current tab.
// Without one, a new project: at the folder, or at the file's repository (else its folder).
// A plugin's install link (`pumi://plugins/install/…`, `install-link.ts`) asks, then installs it.
import { existsSync, statSync } from "node:fs"
import { confirm } from "../sdk/dialogs.ts"
import { installedOf, listingOf, parseInstallLink, questionOf, type InstallLink, type Listing } from "./install-link.ts"
import { pathOf, repoOf, route } from "./open-route.ts"
import { call, engine, on, serverOf, store } from "./engine.ts"
import { openProject, reveal } from "./open-folder.ts"
import { openSettings } from "./settings/open.ts"
import type { WindowManager } from "./windows.tsx"
import { explainNoNewTab } from "./new-tab.ts"

/** A project's current tab, as the server has it now (one made a moment ago included). */
async function currentTab(project: string): Promise<string | undefined> {
  const info = await call("project.get", project)
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

/** Opens what macOS asked to, in turn: a project made for one may be where the next goes. */
async function openAll(manager: WindowManager, urls: string[]) {
  for (const url of urls) {
    const link = parseInstallLink(url)
    if (link) await install(manager, link).catch((err) => console.error(`pumi: installing ${link.source}:`, err))
  }
  const paths = urls.map(pathOf).filter((p): p is string => !!p && existsSync(p))
  if (!paths.length) return
  const window = front(manager)
  /** The projects made here, by directory: the snapshot may not list them yet. */
  const made = new Map<string, string>()
  for (const path of paths) {
    try {
      const folder = statSync(path).isDirectory()
      const local = (store.get().snapshot?.projects ?? []).filter((w) => !serverOf(w.project_id))
      const projects = [
        ...local.map((w) => ({ id: w.project_id, path: w.path })),
        ...[...made].map(([path, id]) => ({ id, path })),
      ]
      const where = route(path, folder, projects, window.project, repoOf)
      let project: string
      if ("project" in where) project = where.project
      else {
        const opened = await openProject(where.create)
        if (!opened) continue
        project = opened
        made.set(where.create, project)
      }
      if (!folder) {
        const beside = await currentTab(project)
        await call("tab.open", "file", { path }, { kind: "tiled", split: "right", ...(beside === undefined ? {} : { beside }) })
      }
      reveal(manager, window, project)
    } catch (err) {
      if (!explainNoNewTab(err)) console.error(`pumi: opening ${path}:`, (err as Error)?.message ?? err)
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
