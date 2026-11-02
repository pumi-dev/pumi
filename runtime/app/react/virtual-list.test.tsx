// A `<virtual-list>`'s `scrollToItem` lands where it was asked, in the window's own tree and inside
// a compartment (a plugin's component, drawn by a cached view of its own): the list scrolls once
// its compartment draws again, even with nothing else in it changed.
import { describe, expect, test } from "bun:test"
import { useRef } from "react"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

const ROWS = 200

function list(compartment: boolean) {
  const t = createTestRoot({ width: 400, height: 400 })
  let ref: { current: any } = { current: null }
  function List() {
    ref = useRef<any>(null)
    const rows = (
      <virtual-list ref={ref} followTail alignment="top" style={{ width: 400, height: 400 }}>
        {Array.from({ length: ROWS }, (_, i) => (
          <div key={i} style={{ display: "flex", width: "100%", height: 30 + (i % 5) * 10 }}>
            <text>{`row ${i}`}</text>
          </div>
        ))}
      </virtual-list>
    )
    return compartment ? (
      <div compartment="me/chat" style={{ width: 400, height: 400 }}>
        <div style={{ display: "flex", flexDirection: "column", width: 400, height: 400 }}>{rows}</div>
      </div>
    ) : (
      rows
    )
  }
  t.render(<List />)
  t.renderer.flush()
  t.renderer.flush()
  const scroll = (index: number) => {
    ref.current.scrollToItem(index, 0)
    t.renderer.flush()
    t.renderer.flush()
    return ref.current.listScrollTop()?.[0]
  }
  return { top: () => ref.current.listScrollTop()?.[0], scroll }
}

gpu("a virtual list's scrollToItem", () => {
  for (const compartment of [false, true]) {
    test(compartment ? "in a compartment" : "in the window's tree", () => {
      const l = list(compartment)
      // Following its tail: at the end.
      expect(l.top()).toBeGreaterThan(150)
      expect(l.scroll(0)).toBe(0)
      expect(l.scroll(120)).toBe(120)
      expect(l.scroll(40)).toBe(40)
    })
  }
})
