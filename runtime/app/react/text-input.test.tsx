// `<text-input>` takes the platform's text and hands it to JS; pointer events carry where in the
// element they happened (`localX`, `localY`).
import { describe, expect, test } from "bun:test"
import type { EventPayload } from "./index.ts"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

gpu("<text-input>", () => {
  test("typed text arrives as textInput, after the keystroke's keyDown, with the range it replaces", () => {
    const t = createTestRoot({ width: 200, height: 100 })
    const got: string[] = []
    t.render(
      <text-input
        testId="editor"
        text="hello"
        selection={[5, 5]}
        caret={{ x: 10, y: 4, height: 16 }}
        onKeyDown={(e) => got.push(`key ${e.key}`)}
        onTextInput={(e: EventPayload) => got.push(`text ${e.value} ${e.startIndex}..${e.endIndex}`)}
        style={{ width: 200, height: 100 }}
      />,
    )
    const id = t.renderer.findByTestId("editor")!.id
    // JS has not sent `text` back between the two: `y` goes after `x` anyway.
    t.renderer.nativeSimulateKeystrokes(id, "x y backspace")
    expect(got).toEqual(["key x", "text x 5..5", "key y", "text y 6..6", "key backspace"])
    t.unmount()
  })

  test("draws its children, which hear the pointer in their own coordinates, and a press focuses it", () => {
    const t = createTestRoot({ width: 200, height: 100 })
    const got: [number | undefined, number | undefined][] = []
    t.render(
      <div style={{ paddingLeft: 30, paddingTop: 20, width: 200, height: 100 }}>
        <text-input testId="editor" onFocus={() => undefined} style={{ width: 100, height: 50 }}>
          <div onMouseDown={(e) => got.push([e.localX, e.localY])} style={{ width: 100, height: 50 }} />
        </text-input>
      </div>,
    )
    t.renderer.nativeSimulateMouseDown(45, 27)
    t.renderer.nativeSimulateMouseUp(45, 27)
    expect(got).toEqual([[15, 7]])
    expect(t.renderer.getFocusedElementId()).toBe(t.renderer.findByTestId("editor")!.id)
    t.unmount()
  })
})
