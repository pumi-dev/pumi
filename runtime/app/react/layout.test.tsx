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
})
