// A text field says where its caret is (`change` and `select` carry the selection), puts it where
// it is told when its value is set from outside (`caret`), and draws mentions as chips (`chips`):
// one character to the caret, taken whole by Backspace, kept as their text in the value.
import { describe, expect, test } from "bun:test"
import { useState } from "react"
import type { EventPayload } from "./index.ts"
import { flushSync } from "./reconciler/reconciler.js"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

const CHIP = { text: "@src/a.rs", label: "a.rs", icon: "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\"><rect width=\"16\" height=\"16\"/></svg>" }

function field(initial: string, caret?: number) {
  const t = createTestRoot({ width: 400, height: 120 })
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
        chips={[CHIP]}
        onChange={(e) => {
          changes.push(e)
          setValue(e.value ?? "")
        }}
        onSelect={(e) => selects.push(e)}
        style={{ width: 400, height: 80 }}
      />
    )
  }
  t.render(<Field />)
  const id = t.renderer.findByTestId("field")!.id
  const keys = (keystrokes: string) => {
    t.renderer.nativeSimulateKeystrokes(id, keystrokes)
    t.renderer.flush()
  }
  return { t, keys, changes, selects, set: (value: string) => (flushSync(() => set(value)), t.renderer.flush()) }
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

gpu("chips", () => {
  test("a chip is drawn as its label and is one character to the caret", () => {
    const { t, keys, selects } = field("see @src/a.rs now")
    expect(t.renderer.getPaintedText().join("\n")).toContain("a.rs")
    expect(t.renderer.getPaintedText().join("\n")).not.toContain("@src/a.rs")
    keys("left left left left")
    expect(caretOf(selects)).toBe(13)
    keys("left")
    expect(caretOf(selects)).toBe(4)
    keys("right")
    expect(caretOf(selects)).toBe(13)
    t.unmount()
  })

  test("Backspace at a chip's end takes it whole, and the value keeps a chip's text", () => {
    const { t, keys, changes } = field("see @src/a.rs now", 13)
    t.renderer.flush()
    keys("backspace")
    expect(changes.at(-1)?.value).toBe("see  now")
    keys("cmd-z")
    expect(changes.at(-1)?.value).toBe("see @src/a.rs now")
    t.unmount()
  })

  test("text that runs on from a chip's is text again", () => {
    const { t, keys, changes, set } = field("")
    set("@src/a.rs")
    keys("x")
    expect(changes.at(-1)?.value).toBe("@src/a.rsx")
    expect(t.renderer.getPaintedText().join("\n")).toContain("@src/a.rsx")
    t.unmount()
  })
})
