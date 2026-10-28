// Layout keeps up with deep trees: taffy 0.12–0.13 laid out each level of stretched flex columns
// several times over, so twenty of them took over a second a frame (the settings page lagged).
import { describe, expect, test } from "bun:test"
import type { ReactNode } from "react"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

function columns(depth: number, leaf: ReactNode): ReactNode {
  return depth === 0 ? leaf : <div style={{ display: "flex", flexDirection: "column" }}>{columns(depth - 1, leaf)}</div>
}

gpu("layout", () => {
  test("twenty stretched flex columns, one in another, lay out in a frame", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    t.render(columns(20, <div testId="leaf" style={{ width: 10, height: 10 }} />) as any)
    t.renderer.flush()
    const start = performance.now()
    for (let i = 0; i < 5; i++) t.renderer.flush()
    // Well over the ~0.1ms it takes, well under the seconds the regression took.
    expect((performance.now() - start) / 5).toBeLessThan(50)
    // A leaf is still sized from its style (taffy 0.14 lays leaves out through gpui's measure).
    const leaf = t.renderer.getElementBounds(t.renderer.findByTestId("leaf")!.id)!
    expect([leaf.width, leaf.height]).toEqual([10, 10])
    t.unmount()
  })

  // A float (`theme/default.tsx`'s `placement`) says where it sits in shares of the view it
  // floats over: its anchor's corner, or half the room left over when it is centred. These are
  // offsets, and a batch that carried one used to be dropped whole, which blanked the window.
  test("an offset is a share of the parent", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    t.render(
      <div style={{ position: "relative", width: "100%", height: "100%", display: "flex" }}>
        <div
          testId="centred"
          style={{ position: "absolute", left: "25%", top: "10%", width: "50%", height: "80%", display: "flex" }}
        />
      </div> as any,
    )
    t.renderer.flush()
    const box = t.renderer.getElementBounds(t.renderer.findByTestId("centred")!.id)!
    // Whole pixels at any scale: a window at 1x rounds a half pixel away.
    expect([box.x, box.y, box.width, box.height]).toEqual([100, 30, 200, 240])
    t.unmount()
  })
})
