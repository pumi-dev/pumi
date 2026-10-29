// `<text inline>`: its `<text>` children are the runs of one paragraph, drawn, wrapped and selected
// as one text, and a run with `onClick` is pressed by a click on its words.
import { describe, expect, test } from "bun:test"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

gpu("<text inline>", () => {
  test("its runs are one painted text, and a drag across them selects them as one", () => {
    const t = createTestRoot({ width: 600, height: 100 })
    t.render(
      <div style={{ display: "flex", flexDirection: "column", width: 600, height: 100 }}>
        <text testId="para" inline style={{ fontSize: 14, lineHeight: 20 }}>
          <text>See </text>
          <text style={{ fontWeight: 700 }}>bold</text>
          <text> and </text>
          <text style={{ fontFamily: "Menlo", backgroundColor: "#333333", borderRadius: 3, paddingLeft: 3 }}>code</text>
          <text> here.</text>
        </text>
      </div>,
    )
    expect(t.renderer.getPaintedText()).toContain("See bold and code here.")
    const box = t.renderer.getElementBounds(t.renderer.findByTestId("para")!.id)!
    const selected = t.renderer.dragSelect(box.x + 1, box.y + 10, box.x + 400, box.y + 10)
    expect(selected).toBe("See bold and code here.")
    t.unmount()
  })

  test("a click on a run with onClick presses it, and one beside it does not", () => {
    const t = createTestRoot({ width: 600, height: 100 })
    const pressed: string[] = []
    t.render(
      <div style={{ display: "flex", flexDirection: "column", width: 600, height: 100 }}>
        <text testId="para" inline style={{ fontSize: 14, lineHeight: 20, fontFamily: "Menlo" }}>
          <text>xxxxxxxxxx</text>
          <text onClick={() => pressed.push("link")}>linklinklink</text>
        </text>
      </div>,
    )
    const box = t.renderer.getElementBounds(t.renderer.findByTestId("para")!.id)!
    t.renderer.nativeSimulateClick(box.x + 2, box.y + 10)
    expect(pressed).toEqual([])
    // Menlo 14 is about 8.4 px a glyph: the fourteenth glyph is in the link.
    t.renderer.nativeSimulateClick(box.x + 8.4 * 14, box.y + 10)
    expect(pressed).toEqual(["link"])
    t.unmount()
  })
})
