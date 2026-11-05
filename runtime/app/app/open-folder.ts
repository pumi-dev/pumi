import { call, idOn, serverOf, store } from "./engine.ts"
import { projectAt } from "./open-route.ts"
import type { AppWindow, WindowManager } from "./windows.tsx"

export async function openProject(path: string, server?: string): Promise<string | undefined> {
  const method = (name: string) => (server ? `${server}/${name}` : name)
  const tab = idOn(server, await call(method("tab.open"), null, {}, { kind: "project", path }))
  const made = await call(method("tab.get"), tab)
  return typeof made?.project === "string" ? made.project : undefined
}

export function reveal(manager: WindowManager, window: AppWindow, project: string) {
  const showing = manager.windows.find((w) => w.project === project)
  if (!showing) return window.show(project)
  showing.renderer.activateWindow()
  showing.show(project)
}

export async function openFolder(manager: WindowManager, window: AppWindow) {
  const picked = await call("fs.pick", { kind: "directory", title: "Open Local Folder" })
  const dir = Array.isArray(picked) ? picked[0] : picked
  if (typeof dir !== "string" || !dir) return
  const local = (store.get().snapshot?.projects ?? []).filter((p) => !serverOf(p.project_id))
  const found = projectAt(local.map((p) => ({ id: p.project_id, path: p.path })), dir)
  const project = found ?? (await openProject(dir))
  if (project) reveal(manager, window, project)
}
