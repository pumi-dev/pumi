// A tab's icon: its own `@icon`, its agent's mark, its block's, the core's; and the forms an icon
// may take.
import { describe, expect, test } from "bun:test"
import { mkdtempSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { assets } from "../app/assets.ts"
import { agentIcon, iconSource, showsStatus, tabIcon } from "./icons.ts"

const SVG = "data:image/svg+xml,<svg/>"
const PNG = "data:image/png;base64,AAAA"

describe("iconSource", () => {
  test("an app icon's name is its file, tinted", () => {
    expect(iconSource("terminal")).toEqual({ src: assets.icon("terminal")!, tint: true })
    expect(iconSource("no_such_icon")).toBeUndefined()
    expect(iconSource("Terminal")).toBeUndefined()
  })

  test("an inline image is drawn as it is given: an SVG tinted, a raster in its own colours", () => {
    expect(iconSource(SVG)).toEqual({ src: SVG, tint: true })
    expect(iconSource(PNG)).toEqual({ src: PNG, tint: false })
    expect(iconSource({ src: SVG, tint: false })).toEqual({ src: SVG, tint: false })
    expect(iconSource("data:text/html,<b>")).toBeUndefined()
    expect(iconSource(`data:image/svg+xml,${"x".repeat(16 * 1024)}`)).toBeUndefined()
  })

  test("a remote image is not fetched", () => {
    expect(iconSource("https://example.com/favicon.ico")).toBeUndefined()
    expect(iconSource({ src: "https://example.com/favicon.svg" })).toBeUndefined()
  })

  test("a plugin's file is found in the plugin, and nowhere else", () => {
    const dir = mkdtempSync(join(tmpdir(), "pumi-icon-"))
    writeFileSync(join(dir, "board.svg"), "<svg/>")
    writeFileSync(join(dir, "photo.png"), "")
    expect(iconSource("plugin:board.svg", dir)).toEqual({ src: join(dir, "board.svg"), tint: true })
    expect(iconSource("plugin:photo.png", dir)).toEqual({ src: join(dir, "photo.png"), tint: false })
    expect(iconSource("plugin:board.svg")).toBeUndefined()
    expect(iconSource("plugin:missing.svg", dir)).toBeUndefined()
    expect(iconSource("plugin:../board.svg", join(dir, "sub"))).toBeUndefined()
    expect(iconSource(`plugin:${join(dir, "board.svg")}`, dir)).toBeUndefined()
  })
})

describe("tabIcon", () => {
  const terminal = { block: "terminal" }

  test("a tab's own icon comes first, then its block's, the core's", () => {
    expect(tabIcon({ ...terminal, agent: "claude", options: { "@icon": SVG } })).toMatchObject({ src: SVG, from: "tab" })
    expect(tabIcon({ ...terminal, agent: "claude" })).toMatchObject({ src: assets.icon("terminal"), from: "core" })
    expect(tabIcon({ block: "me.board.board" }, { icon: "list" })).toMatchObject({ src: assets.icon("list"), from: "block" })
    expect(tabIcon(terminal)).toMatchObject({ src: assets.icon("terminal"), from: "core" })
    expect(tabIcon({ block: "me.board.board" })).toMatchObject({ src: assets.icon("box"), from: "core" })
  })

  test("what does not resolve is passed over", () => {
    expect(tabIcon({ ...terminal, agent: "someone_new", options: { "@icon": "https://x.test/a.png" } })).toMatchObject({ from: "core" })
    expect(tabIcon({ block: "me.board.board" }, { icon: "plugin:gone.svg", dir: tmpdir() })).toMatchObject({ from: "core" })
  })

  test("the app ships no agent marks", () => {
    for (const agent of ["pi", "omp", "claude", "codex"]) expect(agentIcon(agent)).toBeUndefined()
  })
})

describe("showsStatus", () => {
  test("a status that says something happened shows in place of any icon", () => {
    for (const status of ["working", "done", "blocked", "error"]) expect(showsStatus(status, { from: "agent" })).toBe(true)
  })

  test("an idle tab shows its own icon, unless it has only the core's", () => {
    expect(showsStatus("idle", { from: "agent" })).toBe(false)
    expect(showsStatus("unknown", { from: "tab" })).toBe(false)
    expect(showsStatus("idle", { from: "block" })).toBe(false)
    expect(showsStatus("idle", { from: "core" })).toBe(true)
    expect(showsStatus(undefined, { from: "core" })).toBe(false)
  })
})
