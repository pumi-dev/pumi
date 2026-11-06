import { describe, expect, test } from "bun:test"
import { arrange, grows, mergeGroup, moveGroup, moveSection, open as openParts, resize, shareOf, shown, splitOut, TREE, type Arrangement } from "./sidebar.ts"

const two: Arrangement = {
  groups: [
    { id: "a", sections: ["agents", "files", "git"] },
    { id: "b", sections: ["npm"] },
  ],
  active: "a",
}

const ids = (a: Arrangement) => a.groups.map((g) => `${g.id}:${g.sections.join(",")}`)

describe("arrange", () => {
  test("a section not seen before is a page of its own", () => {
    const first = arrange(undefined, ["agents", "files"])
    expect(first.groups.map((g) => g.sections)).toEqual([["agents"], ["files"]])
    expect(first.active).toBe(first.groups[0]!.id)
    // Made again from nothing on every edit: the same pages each time, so an edit finds them.
    expect(arrange(undefined, ["agents"]).groups[0]!.id).toBe(first.groups[0]!.id)
    expect(ids(arrange(two, ["agents", "npm", "tide"]))).toEqual(["a:agents,files,git", "b:npm", "tide:tide"])
  })

  test("a section off for now keeps its place; one seen twice stays in the first", () => {
    const stored = { groups: [{ id: "a", sections: ["x", "y"] }, { id: "b", sections: ["y"] }], active: "b" }
    expect(ids(arrange(stored, ["y"]))).toEqual(["a:x,y"])
    expect(shown(arrange(two, ["agents", "npm"]), ["agents", "npm"]).map((g) => g.sections)).toEqual([["agents"], ["npm"]])
  })

  test("a broken stored value is ignored", () => {
    expect(ids(arrange({ groups: "no" }, ["agents"]))[0]).toMatch(/:agents$/)
  })
})

describe("moves", () => {
  test("sorting within a group", () => {
    expect(ids(moveSection(two, "git", "a", "agents"))).toEqual(["a:git,agents,files", "b:npm"])
    expect(ids(moveSection(two, "agents", "a"))).toEqual(["a:files,git,agents", "b:npm"])
    expect(moveSection(two, "git", "a", "git")).toBe(two)
  })

  test("into another group; the emptied one goes and the target is shown", () => {
    const moved = moveSection(two, "npm", "a", "files")
    expect(ids(moved)).toEqual(["a:agents,npm,files,git"])
    expect(moved.active).toBe("a")
    const out = moveSection({ ...two, active: "b" }, "npm", "a")
    expect(out.active).toBe("a")
  })

  test("out to a group of its own, shown", () => {
    const out = splitOut(two, "git")
    expect(out.groups.map((g) => g.sections)).toEqual([["agents", "files"], ["npm"], ["git"]])
    expect(out.active).toBe(out.groups[2]!.id)
    const front = splitOut(two, "files", "a")
    expect(front.groups.map((g) => g.sections)).toEqual([["files"], ["agents", "git"], ["npm"]])
    // Alone already: its group moves rather than a new one being made.
    const alone = splitOut(two, "npm", "a")
    expect(ids(alone)).toEqual(["b:npm", "a:agents,files,git"])
    expect(alone.active).toBe("b")
  })

  test("groups sort and merge", () => {
    expect(ids(moveGroup(two, "b", "a"))).toEqual(["b:npm", "a:agents,files,git"])
    expect(ids(moveGroup(two, "a"))).toEqual(["b:npm", "a:agents,files,git"])
    const merged = mergeGroup(two, "a", "b")
    expect(ids(merged)).toEqual(["b:npm,agents,files,git"])
    expect(merged.active).toBe("b")
  })
})

describe("the tree", () => {
  test("seen again, it keeps the page it is in, and heads it", () => {
    expect(ids(arrange({ groups: [{ id: "a", sections: ["files"] }] }, [TREE, "files"]))).toEqual(["a:files", "tree:tree"])
    expect(ids(arrange({ groups: [{ id: "a", sections: ["files", TREE] }] }, [TREE, "files"]))).toEqual(["a:tree,files"])
  })

  test("the last section beside it leaves for a group of its own, and comes back after it", () => {
    const both = arrange({ groups: [{ id: "main", sections: [TREE, "files"] }] }, [TREE, "files"])
    const out = splitOut(both, "files")
    expect(out.groups.map((g) => g.sections)).toEqual([[TREE], ["files"]])
    expect(out.active).toBe(out.groups[1]!.id)
    expect(ids(moveSection(out, "files", "main"))).toEqual(["main:tree,files"])
  })

  test("merged into another group, it heads that one", () => {
    const two = arrange({ groups: [{ id: "a", sections: [TREE] }, { id: "b", sections: ["git"] }] }, [TREE, "git"])
    expect(ids(mergeGroup(two, "a", "b"))).toEqual(["b:tree,git"])
    expect(ids(moveGroup(two, "a"))).toEqual(["b:git", "a:tree"])
  })
})

describe("shares", () => {
  test("what is not a share is one", () => {
    expect(shareOf(undefined, "git")).toBe(1)
    expect(shareOf({ git: "big", npm: -2, tide: 3 }, "git")).toBe(1)
    expect(shareOf({ git: "big", npm: -2, tide: 3 }, "npm")).toBe(1)
    expect(shareOf({ tide: 3 }, "tide")).toBe(3)
  })

  test("an edge dragged trades the two beside it, and leaves the rest", () => {
    // A share is 100 px: the tree and git 200 px together, the edge 50 px down.
    expect(resize({ npm: 2 }, TREE, "git", 50, 100, 40)).toEqual({ npm: 2, [TREE]: 1.5, git: 0.5 })
    expect(resize(undefined, TREE, "git", -30, 100, 40)).toEqual({ [TREE]: 0.7, git: 1.3 })
  })

  test("neither side goes under the least, and two too small to hold it stay", () => {
    expect(resize({}, TREE, "git", 500, 100, 40)).toEqual({ [TREE]: 1.6, git: 0.4 })
    expect(resize({}, TREE, "git", -500, 100, 40)).toEqual({ [TREE]: 0.4, git: 1.6 })
    const small = { git: 0.1 }
    expect(resize(small, TREE, "git", 20, 30, 40)).toBe(small)
  })

  test("the open parts' grows fill the page, whatever their shares add up to", () => {
    // Git kept 0.8 from a page it once split with the tree; shown alone it still fills it.
    expect(grows({ [TREE]: 1.2, git: 0.8 }, ["git"])).toEqual({ git: 1 })
    expect(grows({ [TREE]: 1.2, git: 0.8 }, [TREE, "git"])).toEqual({ [TREE]: 0.6, git: 0.4 })
    expect(grows(undefined, ["a", "b"])).toEqual({ a: 0.5, b: 0.5 })
    expect(grows({}, [])).toEqual({})
  })
})

describe("folding", () => {
  test("a page's first section stays open, however it was put away", () => {
    expect(openParts(["files", "git", "npm"], [])).toEqual(["files", "git", "npm"])
    expect(openParts(["files", "git", "npm"], ["files"])).toEqual(["files", "git", "npm"])
    expect(openParts(["files", "git", "npm"], ["files", "git", "npm"])).toEqual(["files"])
  })

  test("the tree heads its page open, so the sections after it fold as they like", () => {
    expect(openParts([TREE, "files", "git"], ["files"])).toEqual([TREE, "git"])
    expect(openParts([TREE, "files"], [TREE, "files"])).toEqual([TREE])
  })
})
