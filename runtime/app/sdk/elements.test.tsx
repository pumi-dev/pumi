// An icon as the window draws it: tinted when it takes the theme's colour, in its own colours when not.
import { describe, expect, test } from "bun:test"
import { createTestRoot, hasNativeTestRenderer } from "../react/testing.ts"
import { IconImage } from "./elements.tsx"

const SVG = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='16'%3E%3Crect width='16' height='16'/%3E%3C/svg%3E"

describe.skipIf(!hasNativeTestRenderer)("IconImage", () => {
  test("a tinted icon is an svg in the colour given, one that keeps its colours an image", () => {
    const t = createTestRoot({ width: 100, height: 40 })
    t.render(
      <div style={{ display: "flex", flexDirection: "row" }}>
        <IconImage icon={{ src: SVG, tint: true }} size={12} color="#ff0000" />
        <IconImage icon={{ src: SVG, tint: false }} size={12} />
        <IconImage icon={{ src: "", tint: true }} />
      </div>,
    )
    expect(t.renderer.findByType("svg").map((el) => el.customProps?.src)).toEqual([SVG])
    expect(t.renderer.findByType("img").map((el) => el.customProps?.src)).toEqual([SVG])
    t.unmount()
  })
})
