import { expect, test } from "bun:test"
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { normal, pathOf, projectAt, repoOf, route } from "./open-route.ts"

const HOME = "/Users/me"
const projects = [
  { id: "$1", path: "~/code" },
  { id: "$2", path: "/Users/me/code/pumi/" },
  { id: "$3", path: "/Users/me/code/pumi" },
  { id: "$4", path: null },
]
const rootOf = (file: string) => `${file}#root`

test("a directory is compared without `~` or a trailing slash", () => {
  expect(normal("~/code/", HOME)).toBe("/Users/me/code")
  expect(normal("~", HOME)).toBe(HOME)
  expect(normal("/", HOME)).toBe("/")
})

test("a folder goes to the project at it, or a new one there", () => {
  expect(route("/Users/me/code", true, projects, undefined, rootOf, HOME)).toEqual({ project: "$1" })
  expect(route("/Users/me/code/pumi/crates", true, projects, undefined, rootOf, HOME)).toEqual({ create: "/Users/me/code/pumi/crates" })
})

test("the project at a folder is the one there, however its path is written", () => {
  expect(projectAt(projects, "/Users/me/code", HOME)).toBe("$1")
  expect(projectAt(projects, "~/code/", HOME)).toBe("$1")
  expect(projectAt(projects, "/Users/me/code/pumi/crates", HOME)).toBeUndefined()
})

test("a file goes to the deepest project holding it, the shown one of equals", () => {
  expect(route("/Users/me/code/pumi/src/a.rs", false, projects, undefined, rootOf, HOME)).toEqual({ project: "$2" })
  expect(route("/Users/me/code/pumi/src/a.rs", false, projects, "$3", rootOf, HOME)).toEqual({ project: "$3" })
  expect(route("/Users/me/code/other/b.md", false, projects, "$3", rootOf, HOME)).toEqual({ project: "$1" })
  expect(route("/Users/me/code-two/b.md", false, projects, undefined, rootOf, HOME)).toEqual({ create: "/Users/me/code-two/b.md#root" })
})

test("a new project for a file is at its repository, else its folder", () => {
  const dir = mkdtempSync(join(tmpdir(), "pumi-open-"))
  try {
    mkdirSync(join(dir, "repo", ".git"), { recursive: true })
    mkdirSync(join(dir, "repo", "src"))
    mkdirSync(join(dir, "loose"))
    writeFileSync(join(dir, "repo", "src", "a.ts"), "")
    expect(repoOf(join(dir, "repo", "src", "a.ts"))).toBe(join(dir, "repo"))
    expect(repoOf(join(dir, "loose", "b.txt"))).toBe(join(dir, "loose"))
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("only a file URL is a path", () => {
  expect(pathOf("file:///Users/me/My%20Notes/")).toBe("/Users/me/My Notes")
  expect(pathOf("pumi://settings/general")).toBeUndefined()
})
