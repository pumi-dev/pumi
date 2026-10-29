// A text field says where its caret is (`change` and `select` carry the selection), puts it where
// it is told when its value is set from outside (`caret`), and has inline nodes (`inlines`), drawn
// in the line by its children: one character to the caret, taken whole by any edit that reaches
// into one, kept as their text in the value.
import { describe, expect, test } from "bun:test"
import { useState } from "react"
import type { EventPayload } from "./index.ts"
import { flushSync } from "./reconciler/reconciler.js"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

const NODE = { text: "@src/a.rs", label: "a.rs" }

function field(initial: string, caret?: number, width = 400) {
  const t = createTestRoot({ width, height: 120 })
  const hovers: boolean[] = []
  const changes: EventPayload[] = []
  const selects: EventPayload[] = []
  let set: (value: string) => void = () => {}
  function Field() {
    const [value, setValue] = useState(initial)
    set = setValue
    return (
      <textarea
        testId="field"
        autoFocus
        value={value}
        caret={caret}
        inlines={[NODE]}
        onChange={(e) => {
          changes.push(e)
          setValue(e.value ?? "")
        }}
        onSelect={(e) => selects.push(e)}
        style={{ width, height: 80, fontSize: 14, lineHeight: 20, fontFamily: "Menlo" }}
      >
        <div testId="node" onMouseEnter={() => hovers.push(true)} onMouseLeave={() => hovers.push(false)} style={{ display: "flex", flexDirection: "row", paddingLeft: 6, paddingRight: 6 }}>
          <text style={{ fontSize: 12 }}>[a.rs]</text>
        </div>
      </textarea>
    )
  }
  t.render(<Field />)
  const id = t.renderer.findByTestId("field")!.id
  const keys = (keystrokes: string) => {
    t.renderer.nativeSimulateKeystrokes(id, keystrokes)
    t.renderer.flush()
  }
  // A node's width is the slot's from the frame after it is first measured.
  t.renderer.flush()
  const bounds = (testId: string) => t.renderer.getElementBounds(t.renderer.findByTestId(testId)!.id)!
  return { t, keys, changes, selects, hovers, bounds, set: (value: string) => (flushSync(() => set(value)), t.renderer.flush(), t.renderer.flush()) }
}

const caretOf = (events: EventPayload[]) => events.at(-1)?.selectionEnd

gpu("a text field's caret", () => {
  test("a change and a move say where the caret is", () => {
    const { t, keys, changes, selects } = field("")
    keys("a b c")
    expect(changes.at(-1)?.value).toBe("abc")
    expect(caretOf(changes)).toBe(3)
    keys("left left")
    expect(caretOf(selects)).toBe(1)
    expect(selects.at(-1)?.selectionStart).toBe(1)
    t.unmount()
  })

  test("a value set from outside puts the caret where `caret` says", () => {
    const { t, keys, set, changes } = field("", 2)
    set("hello")
    keys("x")
    expect(changes.at(-1)?.value).toBe("hexllo")
    expect(caretOf(changes)).toBe(3)
    t.unmount()
  })

  test("a key handed to onKeyDown (`interceptKeys`) reaches it after a letter's keyDown was stopped", () => {
    const t = createTestRoot({ width: 400, height: 100 })
    const got: string[] = []
    t.render(
      <textarea
        testId="field"
        value=""
        interceptKeys={["down", "enter"]}
        onKeyDown={(e) => {
          got.push(e.key ?? "")
          if (e.key === "a") e.stopPropagation()
        }}
        onSubmit={() => got.push("submit")}
        onChange={() => {}}
        style={{ width: 380 }}
      />,
    )
    const id = t.renderer.findByTestId("field")!.id
    for (const key of ["a", "down", "enter"]) t.renderer.nativeSimulateKeystrokes(id, key)
    expect(got).toEqual(["a", "down", "enter"])
    t.unmount()
  })
})

gpu("inline nodes", () => {
  test("a node is drawn by its child in the line, and is one character to the caret", () => {
    const { t, keys, selects, bounds } = field("see @src/a.rs now")
    const painted = t.renderer.getPaintedText().join("\n")
    expect(painted).toContain("[a.rs]")
    expect(painted).not.toContain("@src/a.rs")
    const node = bounds("node")
    const box = bounds("field")
    // After "see " (four Menlo glyphs, about 8.4 px each), on the first line.
    expect(node.x).toBeGreaterThan(box.x + 30)
    expect(node.y).toBeLessThan(box.y + 20)
    keys("left left left left")
    expect(caretOf(selects)).toBe(13)
    keys("left")
    expect(caretOf(selects)).toBe(4)
    keys("right")
    expect(caretOf(selects)).toBe(13)
    t.unmount()
  })

  test("Backspace at a node's end takes it whole, and the value keeps a node's text", () => {
    const { t, keys, changes } = field("see @src/a.rs now", 13)
    keys("backspace")
    expect(changes.at(-1)?.value).toBe("see  now")
    keys("cmd-z")
    expect(changes.at(-1)?.value).toBe("see @src/a.rs now")
    t.unmount()
  })

  test("text typed against a node leaves it a node, and deleting into it takes it whole", () => {
    const { t, keys, changes, set } = field("")
    set("@src/a.rs")
    keys("x")
    expect(changes.at(-1)?.value).toBe("@src/a.rsx")
    expect(t.renderer.getPaintedText().join("\n")).not.toContain("@src/a.rs")
    keys("backspace")
    expect(changes.at(-1)?.value).toBe("@src/a.rs")
    keys("alt-backspace")
    expect(changes.at(-1)?.value).toBe("")
    t.unmount()
  })

  test("a selection that reaches into a node takes it whole when typed over", () => {
    const { t, keys, changes } = field("ab @src/a.rs", 1)
    keys("shift-right shift-right shift-right shift-right")
    keys("z")
    expect(changes.at(-1)?.value).toBe("az")
    t.unmount()
  })

  test("a node that wraps is drawn whole at the start of the next line", () => {
    const { t, bounds } = field("aaaa bbbb cccc @src/a.rs", undefined, 160)
    const node = bounds("node")
    const box = bounds("field")
    expect(node.y).toBeGreaterThanOrEqual(box.y + 19)
    expect(node.x).toBeLessThan(box.x + 12)
    t.unmount()
  })

  test("its child takes the pointer: a hover reaches it", () => {
    const { t, hovers, bounds } = field("see @src/a.rs now")
    const node = bounds("node")
    t.renderer.nativeSimulateMouseMove(node.x + node.width / 2, node.y + node.height / 2)
    t.renderer.flush()
    expect(hovers.at(-1)).toBe(true)
    t.unmount()
  })
})
