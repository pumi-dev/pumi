// A file dragged in from the OS reaches the element under it, whatever the last input was: the
// OS sends no mouse moves while it drags, so a drop after typing must not find nothing hovered.
import { describe, expect, test } from "bun:test"
import { useState } from "react"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

gpu("file drop", () => {
  test("a drop after typing in a field reaches the element under it", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    const got: string[] = []
    function Box() {
      const [value, setValue] = useState("")
      return (
        <div onFileDrop={(e) => got.push(...(e.paths ?? []))} style={{ display: "flex", flexDirection: "column", width: 400, height: 300 }}>
          <textarea testId="field" value={value} onChange={(e) => setValue(e.value ?? "")} style={{ width: 380 }} />
        </div>
      )
    }
    t.render(<Box />)
    t.renderer.nativeSimulateKeystrokes(t.renderer.findByTestId("field")!.id, "a b")
    t.renderer.flush()
    t.renderer.nativeSimulateFileDrop(200, 250, ["/tmp/a.png"])
    expect(got).toEqual(["/tmp/a.png"])
    t.unmount()
  })
})
