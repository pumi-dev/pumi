// `userSelect: "contain"`: a drag that starts inside the element selects inside it only, the way
// a pane's selection stops at its edge instead of running into the panes beside it.
import { describe, expect, test } from "bun:test"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

function Panes({ contain }: { contain: boolean }) {
  const pane = { display: "flex", flexDirection: "column", width: 300, height: 100, userSelect: contain ? "contain" : undefined } as const
  return (
    <div style={{ display: "flex", flexDirection: "row", width: 600, height: 100 }}>
      <div testId="left" style={pane}>
        <text style={{ fontSize: 14, lineHeight: 20 }}>left one</text>
        <text style={{ fontSize: 14, lineHeight: 20 }}>left two</text>
      </div>
      <div testId="right" style={pane}>
        <text style={{ fontSize: 14, lineHeight: 20 }}>right one</text>
        <text style={{ fontSize: 14, lineHeight: 20 }}>right two</text>
      </div>
    </div>
  )
}

gpu('userSelect: "contain"', () => {
  test("without it, a drag runs from one pane into the next", () => {
    const t = createTestRoot({ width: 600, height: 100 })
    t.render(<Panes contain={false} />)
    const right = t.renderer.getElementBounds(t.renderer.findByTestId("right")!.id)!
    const selected = t.renderer.dragSelect(1, 10, right.x + 200, 30)
    expect(selected).toContain("right")
    t.unmount()
  })

  test("a drag into the next pane stops at its own pane's text", () => {
    const t = createTestRoot({ width: 600, height: 100 })
    t.render(<Panes contain />)
    const right = t.renderer.getElementBounds(t.renderer.findByTestId("right")!.id)!
    const selected = t.renderer.dragSelect(1, 10, right.x + 200, 30)
    expect(selected).toBe("left one\nleft two")
    t.unmount()
  })

  test("a press beside a pane's text does not anchor in the pane next to it", () => {
    const t = createTestRoot({ width: 600, height: 100 })
    // The right pane's text sits lower, so its first row shares a band with the left pane's text only.
    t.render(
      <div style={{ display: "flex", flexDirection: "row", width: 600, height: 100 }}>
        <div style={{ display: "flex", flexDirection: "column", width: 300, height: 100, userSelect: "contain" }}>
          <text style={{ fontSize: 14, lineHeight: 20 }}>left one</text>
        </div>
        <div testId="right" style={{ display: "flex", flexDirection: "column", width: 300, height: 100, paddingTop: 50, userSelect: "contain" }}>
          <text style={{ fontSize: 14, lineHeight: 20 }}>right one</text>
        </div>
      </div>,
    )
    const right = t.renderer.getElementBounds(t.renderer.findByTestId("right")!.id)!
    expect(t.renderer.dragSelect(right.x + 250, 10, right.x + 260, 12)).toBeNull()
    t.unmount()
  })
})
