// A session's git worktrees: the checkouts of its folder's repository (`git worktree list`), and
// new ones, made under `worktree_dir` on a branch of their own. A view is in a worktree when its
// `worktree` option names one (the server starts its terminals there). Apart from the engine, so
// the parsing and naming are tested without the addon; git runs here, so only for this machine's
// sessions.
import { homedir } from "node:os"
import { basename, join } from "node:path"

export interface Worktree {
  /** Its folder. */
  path: string
  /** Its branch (`main`), none when its HEAD is detached. */
  branch?: string
  head?: string
  /** The repository's own checkout: the first `git worktree list` gives. */
  main: boolean
  bare?: boolean
  locked?: boolean
  prunable?: boolean
}

export const DEFAULT_DIR = "~/.pumi/worktrees"

/** `git worktree list --porcelain`, read: one block a worktree, the main checkout first. */
export function parse(porcelain: string): Worktree[] {
  const out: Worktree[] = []
  for (const block of porcelain.split(/\n\s*\n/)) {
    const lines = block.split("\n").filter(Boolean)
    const path = lines.find((l) => l.startsWith("worktree "))?.slice("worktree ".length)
    if (!path) continue
    const field = (name: string) => lines.find((l) => l === name || l.startsWith(`${name} `))
    const branch = field("branch")?.slice("branch ".length).replace(/^refs\/heads\//, "")
    out.push({
      path,
      branch: branch || undefined,
      head: field("HEAD")?.slice("HEAD ".length),
      main: out.length === 0,
      ...(field("bare") ? { bare: true } : {}),
      ...(field("locked") ? { locked: true } : {}),
      ...(field("prunable") ? { prunable: true } : {}),
    })
  }
  return out
}

/** What a worktree is called: its branch, else its folder's name. */
export function nameOf(worktree: Pick<Worktree, "path" | "branch">): string {
  return worktree.branch || basename(worktree.path.replace(/\/+$/, "")) || worktree.path
}

/** A folder's name as a worktree's view shows it: the last part of its path. */
export function shortName(path: string): string {
  return basename(path.replace(/\/+$/, "")) || path
}

/** `dir` (the `worktree_dir` option) with `~` for the home folder. */
export function expandHome(dir: string, home = homedir()): string {
  if (dir === "~") return home
  return dir.startsWith("~/") ? join(home, dir.slice(2)) : dir
}

/** Where a new worktree named `name` of the repository at `repo` goes. */
export function placeFor(dir: string | undefined, repo: string, name: string, home = homedir()): string {
  return join(expandHome(dir?.trim() || DEFAULT_DIR, home), shortName(repo), name)
}

const ADJECTIVES = ["avid", "bold", "brisk", "calm", "clever", "deft", "eager", "fair", "fleet", "gentle", "glad", "keen", "lucid", "merry", "nimble", "quiet", "rapid", "sly", "steady", "swift", "tidy", "vivid", "warm", "wry"]
const NOUNS = ["brook", "cedar", "comet", "dune", "ember", "fern", "field", "grove", "harbor", "lark", "maple", "meadow", "moss", "orbit", "pine", "quartz", "reef", "ridge", "river", "spark", "storm", "tide", "willow", "wren"]

/** A fresh name (`avid-storm`), none of `taken`; a number on the end once the pairs run out. */
export function newName(taken: Iterable<string>, random: () => number = Math.random): string {
  const used = new Set(taken)
  const pick = <T>(list: T[]) => list[Math.floor(random() * list.length) % list.length]!
  for (let tries = 0; tries < 64; tries++) {
    const name = `${pick(ADJECTIVES)}-${pick(NOUNS)}`
    if (!used.has(name)) return name
  }
  const base = `${pick(ADJECTIVES)}-${pick(NOUNS)}`
  for (let n = 2; ; n++) if (!used.has(`${base}-${n}`)) return `${base}-${n}`
}

async function git(cwd: string, args: string[]): Promise<string> {
  const proc = Bun.spawn(["git", "-C", cwd, ...args], { stdout: "pipe", stderr: "pipe", stdin: "ignore" })
  const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited])
  if (code !== 0) throw new Error(err.trim() || `git ${args[0]} exited ${code}`)
  return out
}

/** The worktrees of the repository `cwd` is in; none when it is in none (or git is missing). */
export async function list(cwd: string): Promise<Worktree[]> {
  try {
    return parse(await git(cwd, ["worktree", "list", "--porcelain"])).filter((w) => !w.bare)
  } catch {
    return []
  }
}

/** The local branches of the repository `cwd` is in; none when it is in none. */
export async function branches(cwd: string): Promise<string[]> {
  try {
    return (await git(cwd, ["for-each-ref", "--format=%(refname:short)", "refs/heads"])).split("\n").filter(Boolean)
  } catch {
    return []
  }
}

/**
 * A new worktree of the repository `cwd` is in, on a new branch `name` from its HEAD (or on the
 * branch `name`, when there is one already), at `placeFor(dir, …)`. Returns its folder.
 */
export async function create(cwd: string, name: string, dir?: string): Promise<string> {
  const worktrees = await list(cwd)
  const main = worktrees.find((w) => w.main)
  if (!main) throw new Error(`${cwd} is not in a git repository`)
  await git(main.path, ["check-ref-format", "--branch", name]).catch(() => {
    throw new Error(`${name} is not a branch name`)
  })
  const path = placeFor(dir, main.path, name)
  const exists = await git(main.path, ["rev-parse", "--verify", "--quiet", `refs/heads/${name}`]).then(
    () => true,
    () => false,
  )
  await git(main.path, exists ? ["worktree", "add", path, name] : ["worktree", "add", "-b", name, path])
  return path
}
