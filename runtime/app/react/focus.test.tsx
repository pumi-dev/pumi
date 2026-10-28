// An element's ref gives it the keyboard focus, as the DOM's `focus()`: a field, or an element
// with a `tabIndex`.
import { describe, expect, test } from "bun:test"
import { createRef } from "react"
import type { PublicInstance } from "./index.ts"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

gpu("focus()", () => {
  test("a ref's focus() moves the keyboard to its element", () => {
    const t = createTestRoot({ width: 200, height: 100 })
    const field = createRef<PublicInstance>()
    const box = createRef<PublicInstance>()
    t.render(
      <div style={{ display: "flex", flexDirection: "column", width: 200, height: 100 }}>
        <textarea ref={field} testId="field" autoFocus style={{ width: 200, height: 40 }} />
        <div ref={box} testId="box" tabIndex={0} style={{ width: 200, height: 40 }} />
      </div>,
    )
    const id = (testId: string) => t.renderer.findByTestId(testId)!.id
    expect(t.renderer.getFocusedElementId()).toBe(id("field"))
    box.current!.focus!()
    t.renderer.flush()
    expect(t.renderer.getFocusedElementId()).toBe(id("box"))
    field.current!.focus!()
    t.renderer.flush()
    expect(t.renderer.getFocusedElementId()).toBe(id("field"))
    t.unmount()
  })
})
