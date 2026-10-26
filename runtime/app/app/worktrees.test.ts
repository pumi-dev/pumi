import { describe, expect, test } from "bun:test"
import { mkdtempSync, realpathSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { branches, create, expandHome, list, nameOf, newName, parse, placeFor } from "./worktrees.ts"

const PORCELAIN = `worktree /src/mpx
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /Users/me/.pumi/worktrees/mpx/avid-storm
HEAD 2222222222222222222222222222222222222222
branch refs/heads/avid-storm
locked

worktree /tmp/detached
HEAD 3333333333333333333333333333333333333333
detached
prunable gitdir file points to non-existent location
`

describe("parse", () => {
  test("the main checkout first, then each linked one, as git lists them", () => {
    const got = parse(PORCELAIN)
    expect(got.map((w) => [w.path, w.branch, w.main])).toEqual([
      ["/src/mpx", "main", true],
      ["/Users/me/.pumi/worktrees/mpx/avid-storm", "avid-storm", false],
      ["/tmp/detached", undefined, false],
    ])
    expect(got[1]!.locked).toBe(true)
    expect(got[2]!.prunable).toBe(true)
    expect(parse("")).toEqual([])
  })

  test("named for the branch, else the folder", () => {
    const [main, , detached] = parse(PORCELAIN)
    expect(nameOf(main!)).toBe("main")
    expect(nameOf(detached!)).toBe("detached")
  })
})

describe("where and what", () => {
  test("under worktree_dir, by repository, ~ the home folder", () => {
    expect(expandHome("~/w", "/h")).toBe("/h/w")
    expect(expandHome("~", "/h")).toBe("/h")
    expect(expandHome("/abs", "/h")).toBe("/abs")
    expect(placeFor(undefined, "/src/mpx/", "avid-storm", "/h")).toBe("/h/.pumi/worktrees/mpx/avid-storm")
    expect(placeFor("  ", "/src/mpx", "x", "/h")).toBe("/h/.pumi/worktrees/mpx/x")
    expect(placeFor("/wt", "/src/mpx", "x", "/h")).toBe("/wt/mpx/x")
  })

  test("a new name is none of those taken", () => {
    const always = () => 0
    const first = newName([], always)
    expect(first).toMatch(/^[a-z]+-[a-z]+$/)
    expect(newName([first], always)).toBe(`${first}-2`)
    expect(newName([first, `${first}-2`], always)).toBe(`${first}-3`)
  })
})

describe("git", () => {
  test("lists and makes worktrees of a real repository", async () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), "pumi-wt-")))
    try {
      const repo = join(root, "repo")
      const run = (args: string[], cwd = root) => {
        const r = Bun.spawnSync(["git", ...args], { cwd, stdout: "ignore", stderr: "pipe" })
        if (r.exitCode !== 0) throw new Error(r.stderr.toString())
      }
      run(["init", "-q", "-b", "main", repo])
      run(["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "first"], repo)
      expect((await list(repo)).map((w) => [w.path, w.branch])).toEqual([[repo, "main"]])
      const made = await create(repo, "avid-storm", join(root, "wt"))
      expect(made).toBe(join(root, "wt", "repo", "avid-storm"))
      const all = await list(made)
      expect(all.map((w) => [w.path, w.branch, w.main])).toEqual([
        [repo, "main", true],
        [made, "avid-storm", false],
      ])
      expect(await list(root)).toEqual([])
      expect(await branches(repo)).toEqual(["avid-storm", "main"])
      expect(await branches(root)).toEqual([])
      await expect(create(repo, "no..name", join(root, "wt"))).rejects.toThrow("not a branch name")
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
})
