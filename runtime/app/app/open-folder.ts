import { existsSync } from "node:fs"
import { join } from "node:path"
import { confirm, prompt } from "../sdk/dialogs.ts"
import { call, idOn, serverOf, store } from "./engine.ts"
import { projectAt, repoName } from "./open-route.ts"
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

/**
 * Asks for a repository's address and the folder to clone it into, clones it there and shows it:
 * the project there after. Zed's order, the address then the destination, and its picker's button.
 */
export async function cloneRepository(manager: WindowManager, window: AppWindow) {
  const address = (
    await prompt({
      title: "Clone Repository",
      detail: "Clone a repository from GitHub or other sources.",
      placeholder: "Enter repository URL…",
      confirm: "Clone",
    })
  )?.trim()
  if (!address) return
  const name = repoName(address)
  if (!name) {
    await confirm({ title: "That is not a repository's address", detail: `Nothing in ${address} says what to call its folder.`, confirm: "OK", cancel: "Close" })
    return
  }
  const picked = await call("fs.pick", { kind: "directory", title: "Select as Repository Destination" })
  const dir = Array.isArray(picked) ? picked[0] : picked
  if (typeof dir !== "string" || !dir) return
  const target = join(dir, name)
  if (existsSync(target)) {
    const local = (store.get().snapshot?.projects ?? []).filter((p) => !serverOf(p.project_id))
    const found = projectAt(local.map((p) => ({ id: p.project_id, path: p.path })), target)
    if (found) return reveal(manager, window, found)
    await confirm({ title: `${name} is there already`, detail: `${target} is a folder of its own: choose another destination.`, confirm: "OK", cancel: "Close" })
    return
  }
  const failed = await clone(address, target)
  if (failed) {
    await confirm({ title: `Could not clone ${name}`, detail: failed, confirm: "OK", cancel: "Close" })
    return
  }
  try {
    const project = await openProject(target)
    if (project) reveal(manager, window, project)
  } catch (err) {
    await confirm({ title: `Cloned ${name}`, detail: `${target} is there, but nothing could show it: ${String((err as Error)?.message ?? err)}`, confirm: "OK", cancel: "Close" })
  }
}

/** `git clone address target`; answers what git said that went wrong, nothing when it worked. */
async function clone(address: string, target: string): Promise<string | undefined> {
  const proc = Bun.spawn(["git", "clone", "--progress", address, target], { stdout: "pipe", stderr: "pipe", stdin: "ignore" })
  const [said, code] = await Promise.all([new Response(proc.stderr).text(), proc.exited])
  if (code === 0) return undefined
  // Progress goes to stderr too: what is left says why it stopped.
  const lines = said
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.includes("%") && !line.startsWith("Cloning into") && !line.startsWith("remote:"))
  return lines.slice(-6).join("\n") || `git clone exited ${code}`
}
