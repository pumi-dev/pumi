import { describe, expect, test } from "bun:test"
import { assign, glob } from "./dock-panels.ts"

describe("glob", () => {
  test("a star is any run of characters, the rest is itself", () => {
    expect(glob("you/git/*", "you/git/Changes")).toBe(true)
    expect(glob("you/git/*", "you/gitx/Changes")).toBe(false)
    expect(glob("*/files", "me/x/files")).toBe(true)
    expect(glob("a.b/c", "aXb/c")).toBe(false)
    expect(glob("*", "anything/at/all")).toBe(true)
  })
})

describe("assign", () => {
  const keys = ["me/git/Changes", "me/files/Tree", "me/queue/Board"]

  test("a component goes to the first dock that names it, and * takes the rest", () => {
    const held = assign(
      [
        ["sidebar", ["me/files/*"]],
        ["surfaces", ["*"]],
        ["panel", ["me/git/*", "me/files/*"]],
      ],
      keys,
    )
    expect(held.get("sidebar")).toEqual(["me/files/Tree"])
    expect(held.get("panel")).toEqual(["me/git/Changes"])
    expect(held.get("surfaces")).toEqual(["me/queue/Board"])
  })

  test("without a * dock, a component no dock names is in none; a dock without panels holds none", () => {
    const held = assign([["left", undefined], ["right", ["me/git/*"]]], keys)
    expect(held.get("left")).toEqual([])
    expect(held.get("right")).toEqual(["me/git/Changes"])
  })
})
