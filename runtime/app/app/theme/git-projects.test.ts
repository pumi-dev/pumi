import { describe, expect, test } from "bun:test"
import { mkdirSync, mkdtempSync, realpathSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { findGitProjects } from "./git-projects.ts"

function run(args: string[], env: Record<string, string> = {}) {
  const r = Bun.spawnSync(["git", ...args], { stdout: "ignore", stderr: "pipe", env: { ...process.env, ...env } })
  if (r.exitCode !== 0) throw new Error(`git ${args[0]}: ${r.stderr.toString()}`)
}

/** A repository at `dir`, committed at `when`; `commit` false, one with no commits at all. */
function repo(dir: string, when: Date, commit = true) {
  mkdirSync(dir, { recursive: true })
  run(["init", "-q", "-b", "main", dir])
  if (!commit) return
  run(["-C", dir, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "a"], {
    GIT_AUTHOR_DATE: when.toISOString(),
    GIT_COMMITTER_DATE: when.toISOString(),
  })
}

describe("findGitProjects", () => {
  test("the latest commit first, whatever folder found them", async () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), "pumi-projects-")))
    try {
      repo(join(root, "old"), new Date("2020-01-01T00:00:00Z"))
      repo(join(root, "dev", "new"), new Date("2024-01-01T00:00:00Z"))
      repo(join(root, "dev", "mid"), new Date("2022-01-01T00:00:00Z"))
      const found = await findGitProjects({ root })
      expect(found.map((p) => p.name)).toEqual(["new", "mid", "old"])
      expect(found[0]!.path).toBe(join(root, "dev", "new"))
      expect(found[0]!.when).toBe(new Date("2024-01-01T00:00:00Z").getTime())
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  test("a project's own folders are not looked into, nor builds', nor hidden ones", async () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), "pumi-projects-")))
    try {
      repo(join(root, "outer"), new Date("2023-01-01T00:00:00Z"))
      // A repository inside another's folder is not one to start from: the outer one is the project.
      repo(join(root, "outer", "node_modules", "inner"), new Date("2024-01-01T00:00:00Z"))
      repo(join(root, ".hidden", "secret"), new Date("2024-01-01T00:00:00Z"))
      repo(join(root, "dev", "node_modules", "dep"), new Date("2024-01-01T00:00:00Z"))
      repo(join(root, "dev", "target", "built"), new Date("2024-01-01T00:00:00Z"))
      const found = await findGitProjects({ root })
      expect(found.map((p) => p.name)).toEqual(["outer"])
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  test("a repository with no commits is dated by its .git", async () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), "pumi-projects-")))
    try {
      repo(join(root, "fresh"), new Date(), false)
      const found = await findGitProjects({ root })
      expect(found.map((p) => p.name)).toEqual(["fresh"])
      expect(found[0]!.when).toBeGreaterThan(0)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  test("the worktrees of one repository are one project, its main checkout", async () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), "pumi-projects-")))
    try {
      const main = join(root, "repo")
      repo(main, new Date("2023-01-01T00:00:00Z"))
      run(["-C", main, "worktree", "add", "-q", join(root, "wt", "a"), "-b", "a"])
      const found = await findGitProjects({ root })
      expect(found.map((p) => [p.path, p.name])).toEqual([[main, "repo"]])
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  test("keeps the latest `most`", async () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), "pumi-projects-")))
    try {
      for (const day of [1, 2, 3]) repo(join(root, `p${day}`), new Date(`2024-01-0${day}T00:00:00Z`))
      const found = await findGitProjects({ root, most: 2 })
      expect(found.map((p) => p.name)).toEqual(["p3", "p2"])
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  test("a folder that is not there has no projects", async () => {
    expect(await findGitProjects({ root: join(tmpdir(), "pumi-no-such-folder") })).toEqual([])
  })
})
