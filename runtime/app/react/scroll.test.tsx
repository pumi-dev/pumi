// A wheel scrolls a box along the axes it scrolls on, as a browser's does: a sideways swipe
// over a horizontal strip in a vertical page moves the strip, not the page.
import { describe, expect, test } from "bun:test"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

function page() {
  const t = createTestRoot({ width: 300, height: 200 })
  t.render(
    <div testId="page" style={{ display: "flex", flexDirection: "column", width: 300, height: 200, overflowY: "scroll" }}>
      <div testId="strip" style={{ display: "flex", flexDirection: "row", width: "100%", height: 50, flexShrink: 0, overflowX: "scroll" }}>
        <div style={{ width: 1000, height: 50, flexShrink: 0 }} />
      </div>
      <div style={{ width: 300, height: 1000, flexShrink: 0 }} />
    </div>,
  )
  const offset = (testId: string) => t.renderer.getScrollOffset(t.renderer.findByTestId(testId)!.id)
  return { t, offset }
}

gpu("wheel axes", () => {
  test("a sideways wheel over a horizontal strip scrolls the strip and not its vertical page", () => {
    const { t, offset } = page()
    t.renderer.nativeSimulateScrollWheel(100, 25, -40, 0)
    t.renderer.flush()
    expect(offset("strip")![0]).not.toBe(0)
    expect(offset("page")).toEqual([0, 0])
    t.unmount()
  })

  test("a sideways wheel does not scroll a vertical page", () => {
    const { t, offset } = page()
    t.renderer.nativeSimulateScrollWheel(100, 150, -40, 0)
    t.renderer.flush()
    expect(offset("page")).toEqual([0, 0])
    t.unmount()
  })

  test("a vertical wheel still scrolls the page, over the strip too", () => {
    const { t, offset } = page()
    t.renderer.nativeSimulateScrollWheel(100, 25, 0, -40)
    t.renderer.flush()
    expect(offset("page")![1]).not.toBe(0)
    expect(offset("strip")).toEqual([0, 0])
    t.unmount()
  })
})
