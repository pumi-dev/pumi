import { describe, expect, test } from "bun:test"
import { beyond, projectRows, within, type Listing } from "./project-list.ts"
import type { GitProject } from "./git-projects.ts"

/** What a scan of this Mac's home folder found: the user's folder, and two others. */
const git: GitProject[] = [
  { path: "/Users/me/Kitchen/pumi-all/pumi", name: "pumi", when: 3 },
  { path: "/Users/me/Kitchen/mpx", name: "mpx", when: 2 },
  { path: "/Users/me/dev/site", name: "site", when: 1 },
]
const short = (path: string) => path.replace("/Users/me", "~")

const listing = (over: Partial<Listing> = {}): Listing => ({
  here: [],
  recent: [],
  projects: [],
  query: "",
  short,
  ...over,
})

describe("projectRows", () => {
  test("the projects pumi remembers on top, the git projects found on this Mac under them", () => {
    const projects = [{ id: "$1", label: "pumi", path: "/Users/me/Kitchen/pumi-all/pumi" }]
    const rows = projectRows(listing({ here: ["$1"], projects, found: git }))
    expect(rows.here.map((r) => r.label)).toEqual(["pumi"])
    // The project on top is not added again as a found one: its path is the same.
    expect(rows.found.map((r) => [r.label, r.detail])).toEqual([
      ["mpx", "~/Kitchen/mpx"],
      ["site", "~/dev/site"],
    ])
  })

  test("a project inside one pumi remembers is not added under it", () => {
    const projects = [{ id: "$1", label: "Kitchen", path: "/Users/me/Kitchen" }]
    const rows = projectRows(listing({ here: ["$1"], projects, found: git }))
    expect(rows.found.map((r) => r.label)).toEqual(["site"])
  })

  test("a found project a remembered one is a folder of stays", () => {
    const projects = [{ id: "$1", label: "pumi", path: "/Users/me/Kitchen/pumi-all/pumi/runtime" }]
    const rows = projectRows(listing({ here: ["$1"], projects, found: git }))
    expect(rows.found.map((r) => r.label)).toEqual(["pumi", "mpx", "site"])
  })

  test("with nothing remembered, every git project found", () => {
    expect(projectRows(listing({ found: git })).found.map((r) => r.label)).toEqual(["pumi", "mpx", "site"])
  })

  test("before the scan answers, no found rows", () => {
    expect(projectRows(listing()).found).toEqual([])
  })

  test("this window's projects first, in the order it opened them, the window's own added last", () => {
    const projects = [
      { id: "$1", label: "one" },
      { id: "$2", label: "two" },
      { id: "$3" },
    ]
    const rows = projectRows(listing({ here: ["$2"], current: "$1", projects, recent: ["$1", "$2", "$3"] }))
    expect(rows.here.map((r) => [r.id, r.label, r.current])).toEqual([
      ["$2", "two", false],
      ["$1", "one", true],
    ])
    // The rest, the recent order first, then any the app never showed; a project only once.
    expect(rows.recent.map((r) => [r.id, r.label])).toEqual([["$3", "$3"]])
  })

  test("projects the server no longer lists are not rows", () => {
    const rows = projectRows(listing({ here: ["$9"], recent: ["$8"], projects: [], current: "$9" }))
    expect(rows.here).toEqual([])
    expect(rows.recent).toEqual([])
  })

  test("one search takes the remembered projects (by name or folder) and the found ones", () => {
    const projects = [
      { id: "$1", label: "pumi", path: "/Users/me/Kitchen/pumi-all/pumi" },
      { id: "$2", label: "site", path: "/Users/me/dev/site" },
    ]
    const rows = (query: string) => projectRows(listing({ found: git, projects, recent: ["$1", "$2"], query }))
    // A remembered project answers to its folder too; it stays on top, not twice.
    expect(rows("kitchen").recent.map((r) => r.id)).toEqual(["$1"])
    expect(rows("kitchen").found.map((r) => r.label)).toEqual(["mpx"])
    expect(rows("SIT").recent.map((r) => r.id)).toEqual(["$2"])
    expect(rows("SIT").found).toEqual([])
    // A project pumi does not remember is still found by name.
    expect(rows("mpx").recent).toEqual([])
    expect(rows("mpx").found.map((r) => r.label)).toEqual(["mpx"])
    expect(rows("  ").found).toHaveLength(1)
    expect(rows("nope").found).toEqual([])
  })
})

describe("where a found project sits", () => {
  test("within: the same folder, or inside it", () => {
    expect(within("/a/b", "/a/b")).toBe(true)
    expect(within("/a/b/", "/a/b")).toBe(true)
    expect(within("/a/b/c", "/a/b")).toBe(true)
    expect(within("/a/bc", "/a/b")).toBe(false)
    expect(within("/a", "/a/b")).toBe(false)
  })

  test("beyond: the found ones no known folder is", () => {
    const found = [
      { path: "/a/b", name: "b", when: 1 },
      { path: "/a/b/c", name: "c", when: 2 },
      { path: "/d", name: "d", when: 3 },
    ]
    expect(beyond(found, ["/a/b", undefined]).map((p) => p.name)).toEqual(["d"])
    expect(beyond(found, []).map((p) => p.name)).toEqual(["b", "c", "d"])
  })
})
