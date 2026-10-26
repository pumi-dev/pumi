// Settings' pages and search as data: which pages there are, where a link goes, what the search
// finds without drawing a component; and what settings.json keeps.
import { describe, expect, mock, test } from "bun:test"
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import type { Decl } from "../../sdk/settings/options.ts"
import { otherGroups, pageOfLink, pluginPages, search, searchIndex, type PluginSettings } from "./pages.ts"

const decls: Decl[] = [
  { name: "ui_font_size", type: "number", label: "Text size", group: "Default theme" },
  { name: "terminal_colorscheme", type: "string", label: "Color scheme", group: "Terminal", control: "terminal_theme" },
  { name: "keymap_preset", type: "string", label: "Base keymap", group: "Keys", choices: ["default", "vscode"], labels: ["pumi", "VS Code"] },
  { name: "ui_pane_titles", type: "boolean", label: "Show pane title bars", group: "Default theme · Panes" },
  { name: "them.other.search", type: "string", label: "Search", plugin: "them/other" },
  { name: "exit_empty", type: "boolean", label: "Exit when empty", group: "Server" },
  { name: "default_size", type: "string" },
  { name: "me.remote.position", type: "string", plugin: "me/remote", desc: "Where the code shows." },
  { name: "you.git.fetch", type: "boolean", plugin: "you/git", label: "Fetch" },
  { name: "agents.permissions", type: "boolean", plugin: "agents", label: "Answer permissions in pumi" },
]

const remote: PluginSettings = {
  id: "me/remote",
  prefix: "me.remote.",
  name: "remote",
  pages: [{ id: "remote", title: "Remote", icon: "globe", component: "RemotePage", keywords: ["phone", "qr"] }],
  sections: [
    { id: "tailscale", page: "general", title: "Tailscale", component: "TailscaleSection", keywords: ["sign in"] },
    { id: "devices", page: "remote", title: "Devices", component: "Devices" },
  ],
}
const git: PluginSettings = { id: "you/git", prefix: "you.git.", name: "git" }

describe("pages", () => {
  test("a plugin's own pages and sections, and a page made of the options of one that says nothing of Settings", () => {
    const { pages, sections } = pluginPages([remote, git], decls, true)
    expect(pages.map((p) => [p.id, p.title, p.component])).toEqual([
      ["me.remote.remote", "Remote", "RemotePage"],
      ["you/git", "git", undefined],
      // A plugin the app has no manifest for (another app's): its page too.
      ["them/other", "other", undefined],
    ])
    expect(sections.map((s) => [s.id, s.page])).toEqual([
      ["me.remote.tailscale", "general"],
      ["me.remote.devices", "me.remote.remote"],
    ])
  })

  test("in safe mode a plugin's components are left out; its options' page is not", () => {
    const { pages, sections } = pluginPages([remote, git], decls, false)
    expect(pages.map((p) => p.id)).toEqual(["you/git", "them/other"])
    expect(sections).toEqual([])
  })

  test("a link goes to the page that draws what it names", () => {
    const { pages, sections } = pluginPages([remote, git], decls, true)
    const at = (link: string) => pageOfLink(link, decls, pages, sections)
    expect(at("ui_font_size")).toBe("appearance")
    expect(at("keymap_preset")).toBe("keys")
    expect(at("ui_pane_titles")).toBe("appearance")
    expect(at("them.other.search")).toBe("them/other")
    expect(at("me.remote.tailscale")).toBe("general")
    expect(at("me.remote.devices")).toBe("me.remote.remote")
    expect(at("me.remote.position")).toBe("me.remote.remote")
    expect(at("you.git.fetch")).toBe("you/git")
    expect(at("safe-mode")).toBe("plugins")
    expect(at("agents.permissions")).toBe("agents")
    expect(at("agent-hooks")).toBe("agents")
    expect(pages.some((p) => p.id === "agents"), "the agents' options are Agents', not a page of their own").toBe(false)
    expect(at("command:new_view")).toBe("keys")
    expect(at("nothing")).toBeUndefined()
  })

  test("General draws the built-in groups no other page claims, not the server's unlabelled options", () => {
    expect(otherGroups(decls)).toEqual(["Server"])
  })
})

describe("search", () => {
  const { pages, sections } = pluginPages([remote, git], decls, true)
  const index = searchIndex(decls, pages, sections, [{ name: "new_tab", label: "New tab", group: "App", keys: ["cmd+t"] }])
  const ids = (q: string) => search(index, q).map((e) => e.id)

  test("finds an option by its label, what it means, and its choices' labels", () => {
    expect(ids("text size")).toEqual(["ui_font_size"])
    expect(ids("vs code")).toEqual(["keymap_preset"])
    expect(ids("code shows")).toEqual(["me.remote.position"])
  })

  test("finds a plugin's page and section by their titles and keywords, and the built-in rows", () => {
    expect(ids("qr")).toEqual(["me.remote.remote"])
    expect(ids("sign in")).toEqual(["me.remote.tailscale"])
    expect(ids("tailscale")).toEqual(["me.remote.tailscale"])
    expect(ids("restart server")).toEqual(["server"])
  })

  test("finds a command by its name and keys, and nothing for an empty query", () => {
    expect(ids("new tab")).toEqual(["command:new_tab"])
    expect(ids("cmd+t")).toEqual(["command:new_tab"])
    expect(ids("  ")).toEqual([])
  })
})

// The app's engine, without starting it (`store.ts` sets options through it).
mock.module("../engine.ts", () => ({
  store: { get: () => ({ trees: {}, options: {} }), subscribe: () => () => {} },
  call: () => Promise.resolve(null),
  on: () => () => {},
  engine: { call: () => Promise.resolve(null) },
}))
const { keep } = await import("./store.ts")

describe("settings.json", () => {
  test("keeps a value under `options`, takes it out for null, and leaves the rest of the file", () => {
    const dir = mkdtempSync(join(tmpdir(), "pumi-settings-"))
    const file = join(dir, "settings.json")
    writeFileSync(file, JSON.stringify({ keys: { "cmd+k": "clear" }, options: { linespace: 1.4 } }))
    keep("guifont", "Menlo:h13", file)
    keep("linespace", null, file)
    const json = JSON.parse(readFileSync(file, "utf8"))
    expect(json).toEqual({ keys: { "cmd+k": "clear" }, options: { guifont: "Menlo:h13" } })
    keep("x", 1, join(dir, "new", "settings.json"))
    expect(JSON.parse(readFileSync(join(dir, "new", "settings.json"), "utf8"))).toEqual({ options: { x: 1 } })
  })
})
