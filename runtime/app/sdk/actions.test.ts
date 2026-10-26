// Actions as data: a menu's groups, and the keys written beside a command's.
import { describe, expect, test } from "bun:test"
import { grouped, heldShortcut, setChordFor, shortcut, type Action } from "./actions.ts"

const action = (id: string, group?: string): Action => ({ id, title: id, plugin: "pumi", group, run: () => {} })

describe("grouped", () => {
  test("consecutive actions of one group go together, in order", () => {
    const groups = grouped([action("a", "x"), action("b", "x"), action("c", "y"), action("d"), action("e")])
    expect(groups.map((g) => g.map((a) => a.id))).toEqual([["a", "b"], ["c"], ["d", "e"]])
  })
})

describe("shortcut", () => {
  test("a command's chords as its OS writes them, and nothing for one without", () => {
    setChordFor((command) => ({ split_horizontal: "cmd+shift+d", zoom: "cmd+enter", equalize_panes: "cmd+alt+equal" })[command])
    const mac = process.platform === "darwin"
    expect(shortcut("split_horizontal")).toBe(mac ? "⇧ ⌘ D" : "Win+Shift+D")
    expect(shortcut("zoom")).toBe(mac ? "⌘ ↵" : "Win+Enter")
    expect(shortcut("equalize_panes")).toBe(mac ? "⌥ ⌘ =" : "Win+Alt+=")
    expect(shortcut("nothing")).toBeUndefined()
  })
})

describe("heldShortcut", () => {
  test("a command's keys while just their modifiers are held, nothing otherwise", () => {
    setChordFor((command, index) =>
      command === "switch_tab" && index !== undefined && index >= 1 && index <= 9
        ? `cmd+${index}`
        : ({ split_horizontal: "shift+cmd+d", fullscreen: "f11", zoom: "ctrl+x z" } as Record<string, string>)[command],
    )
    const mac = process.platform === "darwin"
    expect(heldShortcut("cmd", "switch_tab", 3)).toBe(mac ? "⌘ 3" : "Win+3")
    expect(heldShortcut("shift+cmd", "split_horizontal")).toBe(mac ? "⇧ ⌘ D" : "Win+Shift+D")
    // More or fewer held than they need, none held, or no modifier to hold at all.
    expect(heldShortcut("shift+cmd", "switch_tab", 3)).toBeUndefined()
    expect(heldShortcut("cmd", "split_horizontal")).toBeUndefined()
    expect(heldShortcut("", "switch_tab", 3)).toBeUndefined()
    expect(heldShortcut("cmd", "switch_tab", 10)).toBeUndefined()
    expect(heldShortcut("cmd", "fullscreen")).toBeUndefined()
    expect(heldShortcut("ctrl", "zoom")).toBeUndefined()
  })
})
