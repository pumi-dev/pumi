// The git projects this Mac has been working in lately, found to fill the projects list (the
// picker, and the start screen): under the projects pumi remembers, so a first run starts from
// what the user was already working on, and later runs can still find a project they have not
// opened here. Once one is opened it is a project, and memory puts it on top.
//
// Finding them: the home folder a few levels deep (the folders macOS keeps private and the ones
// builds fill aside, hidden ones never), every folder holding a `.git`, the latest commit first.
// The worktrees of one repository are one project: its main checkout. One `git` runs per project,
// none for a `.git` that names its own way around, and the answer is kept for a run (`gitProjects`)
// and between them (`default.projects.found` in `window.json`, a theme's).
// Git runs here, so only for this machine's projects (`worktrees.ts` runs it the same way).
import type { Dirent } from "node:fs"
import { readdir, readFile, stat } from "node:fs/promises"
import { homedir } from "node:os"
import { basename, dirname, join } from "node:path"

/** A git project: its folder, what it is called, and when its last commit was (ms since the epoch). */
export interface GitProject {
  path: string
  name: string
  /** Its last commit's time; an empty repository's, its `.git`'s. */
  when: number
}

/** How far under the home folder to look: `~` is 0, so `~/dev/org/project` (3) is found. */
const DEPTH = 4
/**
 * Folders never looked into: macOS's own and the media libraries (reading them asks the user for
 * permission, and no project is in them), and what builds leave where a project's own `.git` is
 * the one that counts.
 */
const SKIP = new Set([
  "Applications",
  "Desktop",
  "Documents",
  "Downloads",
  "Library",
  "Movies",
  "Music",
  "Pictures",
  "Public",
  "build",
  "DerivedData",
  "dist",
  "node_modules",
  "Pods",
  "target",
  "vendor",
])
/** The most projects kept: the picker searches them all, and a Mac rarely has more. */
const MOST = 500
/** The most folders taken for projects before they are asked of at all, in case one is huge. */
const FOUND_AT = 500
/** The most projects `git` is asked about: the ones their `.git` says were worked in lately. */
const ASKED = 120
/** How many `git log`s run at once, and how long one may take before the folder's `.git` speaks. */
const JOBS = 8
const TIMEOUT = 4000

/** A folder is a project when its `.git` is in it: a repository's, a worktree's (a file), or a submodule's. */
function isProject(entries: { name: string }[]): boolean {
  return entries.some((e) => e.name === ".git")
}

/** The folders under `dir` holding a `.git`, to `DEPTH`, in no order; a project's own folders are not looked into. */
async function walk(dir: string, depth: number, found: string[]): Promise<void> {
  if (found.length >= FOUND_AT) return
  let entries: Dirent[]
  try {
    entries = await readdir(dir, { withFileTypes: true })
  } catch {
    return
  }
  if (isProject(entries)) {
    found.push(dir)
    return
  }
  if (depth >= DEPTH) return
  for (const entry of entries) {
    // A symlink is not followed: it may be a loop, and the folder it leads to is found where it is.
    if (!entry.isDirectory() || entry.name.startsWith(".") || SKIP.has(entry.name)) continue
    await walk(join(dir, entry.name), depth + 1, found)
  }
}

async function spawnText(argv: string[]): Promise<string> {
  const proc = Bun.spawn(argv, { stdout: "pipe", stderr: "ignore", stdin: "ignore" })
  const timer = setTimeout(() => proc.kill(), TIMEOUT)
  try {
    const [out, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited])
    if (code !== 0) throw new Error(`git exited ${code}`)
    return out
  } finally {
    clearTimeout(timer)
  }
}

/** When `dir`'s last commit was; an empty repository (git refuses to log), its `.git`'s own time. */
async function lastCommit(dir: string): Promise<number> {
  try {
    const out = await spawnText(["git", "-C", dir, "log", "-1", "--format=%ct"])
    const seconds = Number(out.trim())
    if (Number.isFinite(seconds) && seconds > 0) return seconds * 1000
  } catch {
    // An empty repository (git refuses to log it) or no git at all: its `.git`'s time stands in.
  }
  try {
    const git = await stat(join(dir, ".git"))
    return git.mtimeMs
  } catch {
    return 0
  }
}

/** `f` over `items`, `limit` of them at a time, in the order they were given. */
async function mapLimit<T, R>(items: T[], limit: number, f: (item: T) => Promise<R>): Promise<R[]> {
  const out = new Array<R>(items.length)
  let next = 0
  const worker = async () => {
    while (next < items.length) {
      const at = next++
      out[at] = await f(items[at]!)
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker))
  return out
}

/**
 * The checkout a folder's repository is worked in: the folder itself, or, for a worktree (its
 * `.git` a file naming `<the repository's .git>/worktrees/<name>`), the repository's own checkout.
 */
async function mainCheckout(dir: string): Promise<string> {
  try {
    const entry = await stat(join(dir, ".git"))
    if (entry.isDirectory()) return dir
    const text = await readFile(join(dir, ".git"), "utf8")
    const named = text.replace(/^gitdir:\s*/, "").trim()
    const at = named.indexOf("/worktrees/")
    if (at < 0) return dir
    const common = named.slice(0, at)
    return basename(common) === ".git" ? dirname(common) : dir
  } catch {
    // A `.git` that cannot be read (or a submodule's, in another repository's modules): the folder.
    return dir
  }
}

/** When `dir`'s repository was worked in, as the file system has it: its `.git`'s or its index's time. */
async function touched(dir: string): Promise<number> {
  for (const at of [join(dir, ".git", "index"), join(dir, ".git")]) {
    try {
      const entry = await stat(at)
      return entry.mtimeMs
    } catch {
      // A worktree's `.git` is a file, not a folder: the next one says when.
    }
  }
  return 0
}

/**
 * The git projects under `root` (the home folder), the latest commit first, at most `most`. Hidden
 * folders are not looked into, nor `SKIP`'s, nor the folders of a project already found. Every one
 * found is listed: `git` is asked about the `ASKED` that were worked in most recently, and the rest
 * come after them by folder.
 */
export async function findGitProjects({ root = homedir(), most = MOST }: { root?: string; most?: number } = {}): Promise<GitProject[]> {
  const found: string[] = []
  await walk(root, 0, found)
  // One project however many of its worktrees were found, touched when the latest of them was.
  const touchedAt = new Map<string, number>()
  for (const dir of found) {
    const path = await mainCheckout(dir)
    touchedAt.set(path, Math.max(touchedAt.get(path) ?? 0, await touched(dir)))
  }
  const asked = [...touchedAt].sort((a, b) => b[1] - a[1]).slice(0, ASKED)
  const commits = new Map<string, number>()
  await mapLimit(asked, JOBS, async ([path]) => {
    commits.set(path, await lastCommit(path))
  })
  const projects = [...touchedAt].map(([path, at]) => ({ path, name: basename(path) || path, when: commits.get(path) ?? 0, at }))
  projects.sort((a, b) => b.when - a.when || b.at - a.at || a.path.localeCompare(b.path))
  return projects.slice(0, most).map(({ path, name, when }) => ({ path, name, when }))
}

let cached: Promise<GitProject[]> | undefined

/** `findGitProjects`, found once: the projects list asks on every render, and the answer is the same. */
export function gitProjects(): Promise<GitProject[]> {
  cached ??= findGitProjects().catch(() => [])
  return cached
}

/** How long a found list is worth keeping before it is looked for again: it is a fallback, not a live index. */
export const FOUND_FOR = 30 * 60 * 1000
