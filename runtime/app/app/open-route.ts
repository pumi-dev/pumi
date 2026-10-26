// Where a path macOS asked the app to open goes (`opened.ts`): which session, or a new one where.
// Apart from the engine, so it is tested without the addon.
import { existsSync } from "node:fs"
import { homedir } from "node:os"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

/** Where an opened path goes: a session there is, or a new one at a directory. */
export type Where = { session: string } | { create: string }

/** A directory as sessions are compared by it: `~` is home, no trailing separator (`C:\` keeps
 *  its own: `C:` is the current directory on that drive). */
export function normal(dir: string, home = homedir()): string {
  const full = dir === "~" ? home : /^~[\\/]/.test(dir) ? join(home, dir.slice(2)) : dir
  const cut = full.replace(/[\\/]+$/, "")
  return cut === "" || /^[A-Za-z]:$/.test(cut) ? full : cut
}

/** `path` as Windows compares two of them: by case, and `/` for `\` (`/` as the one spelling,
 *  which Windows takes too). */
const key = (path: string) => (process.platform === "win32" ? path.replace(/\\/g, "/").toLowerCase() : path)

/** Whether `path` is `dir` or inside it. */
const holds = (dir: string, path: string) => {
  const [d, p] = [key(dir), key(path)]
  return p === d || p.startsWith(d.endsWith("/") ? d : `${d}/`)
}

/**
 * Where `path` goes among `sessions` (this server's, each by its directory). A folder: the session
 * at it. A file: the one at the deepest directory holding it. Of equals, `shown`. Neither: a new
 * session at the folder, or at `rootOf` the file (its repository, else its folder).
 */
export function route(
  path: string,
  folder: boolean,
  sessions: { id: string; cwd?: string | null }[],
  shown: string | undefined,
  rootOf: (file: string) => string,
  home = homedir(),
): Where {
  let best: { id: string; depth: number } | undefined
  for (const s of sessions) {
    if (!s.cwd) continue
    const dir = normal(s.cwd, home)
    if (folder ? key(dir) !== key(path) : !holds(dir, path)) continue
    const better = !best || dir.length > best.depth || (dir.length === best.depth && s.id === shown)
    if (better) best = { id: s.id, depth: dir.length }
  }
  if (best) return { session: best.id }
  return { create: folder ? path : rootOf(path) }
}

/** The repository holding `file` (the nearest directory up with a `.git`), else its folder. */
export function repoOf(file: string): string {
  const folder = dirname(file)
  for (let dir = folder; ; dir = dirname(dir)) {
    if (existsSync(join(dir, ".git"))) return dir
    if (dir === dirname(dir)) return folder
  }
}

/** A `file://` URL's path; none for another scheme's. */
export function pathOf(url: string): string | undefined {
  if (!url.startsWith("file:")) return undefined
  try {
    return normal(fileURLToPath(url))
  } catch {
    return undefined
  }
}
