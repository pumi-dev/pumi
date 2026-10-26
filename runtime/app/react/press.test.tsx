// A press's events bubble from the element under the pointer to its ancestors, as the DOM's do.
import { describe, expect, test } from "bun:test"
import type { PressEvent } from "./index.ts"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

function clicks(stop: boolean) {
  const t = createTestRoot({ width: 200, height: 100 })
  const got: string[] = []
  t.render(
    <div testId="row" onClick={() => got.push("row")} onMouseDown={() => got.push("row down")} style={{ width: 200, height: 100 }}>
      <div
        testId="button"
        onClick={(e: PressEvent) => {
          got.push("button")
          if (stop) e.stopPropagation()
        }}
        style={{ width: 20, height: 20 }}
      />
    </div>,
  )
  const click = (x: number, y: number) => {
    got.length = 0
    t.renderer.nativeSimulateClick(x, y)
    return [...got]
  }
  return { click, unmount: t.unmount }
}

gpu("press propagation", () => {
  test("a click reaches the element under it, then its ancestors", () => {
    const { click, unmount } = clicks(false)
    expect(click(10, 10)).toEqual(["row down", "button", "row"])
    unmount()
  })

  test("a click stopped by a child does not reach its ancestors, and other events of the press still do", () => {
    const { click, unmount } = clicks(true)
    expect(click(10, 10)).toEqual(["row down", "button"])
    expect(click(100, 50)).toEqual(["row down", "row"])
    unmount()
  })
})
