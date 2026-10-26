// The keymap's chord to show for a command, and for an index of a range command.
import { describe, expect, test } from "bun:test"
import { Keymap } from "./keymap.ts"

const keymap = new Keymap({
  commands: [
    { name: "switch_view", label: "Switch to view 1–9", group: "Views", scope: "global", range: true },
    { name: "zoom", label: "Zoom", group: "Panes", scope: "global" },
  ],
  defaults: [
    ["switch_view", "cmd+1..9"],
    ["zoom", "ctrl+x z"],
    ["zoom", "cmd+enter"],
  ],
})

describe("chordFor", () => {
  test("a single keystroke over a sequence, and no range without an index", () => {
    expect(keymap.chordFor("zoom")).toBe("cmd+enter")
    expect(keymap.chordFor("switch_view")).toBeUndefined()
  })

  test("of a range, the chord its range gives the index; none outside it", () => {
    expect(keymap.chordFor("switch_view", 1)).toBe("cmd+1")
    expect(keymap.chordFor("switch_view", 9)).toBe("cmd+9")
    expect(keymap.chordFor("switch_view", 10)).toBeUndefined()
    expect(keymap.chordFor("switch_view", -1)).toBeUndefined()
  })
})
