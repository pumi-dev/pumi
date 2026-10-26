// `diagnostics.ts` must load before React's reconciler: it installs the hook React reports commits to.
import { connect, countCall, countEvent, sample, setWatching } from "./diagnostics.ts"
import { describe, expect, test } from "bun:test"
import { memo, useState } from "react"
import { flushSync } from "../react/reconciler/reconciler.ts"
import { createTestRoot, hasNativeTestRenderer } from "../react/testing.ts"

describe("a second", () => {
  test("is counted once, and the counts start again", () => {
    countEvent("state", 2)
    countEvent("state", 3)
    countEvent("terminal", 1)
    countCall("tab.list", 4)
    countCall("session.get", 40)
    const second = sample()
    expect(second.events[0]).toMatchObject({ name: "state" })
    expect(second.events.map((e) => e.name)).toEqual(["state", "terminal"])
    expect(second.callMaxMs).toBe(40)
    expect(second.callSlowest).toBe("session.get")
    const next = sample()
    expect(next.events).toEqual([])
    expect(next.callSlowest).toBeUndefined()
  })

  test("goes out only while watched, and the startup's marks with the first", () => {
    const sent: any[] = []
    connect((report) => sent.push(report))
    setWatching(true)
    expect(sent[0].startup).toBeArray()
    setWatching(false)
    expect(sent.length).toBe(1)
    connect(() => {})
  })
})

describe.skipIf(!hasNativeTestRenderer)("React", () => {
  test("counts the components a commit re-rendered, not those it skipped", () => {
    const t = createTestRoot({ width: 200, height: 100 })
    let bump = () => {}
    function Leaf({ n }: { n: number }) {
      return <text>{n}</text>
    }
    const Still = memo(function Still() {
      return <text>still</text>
    })
    function Counter() {
      const [n, set] = useState(0)
      bump = () => set((v) => v + 1)
      return (
        <div>
          <Leaf n={n} />
          <Still />
        </div>
      )
    }
    t.render(<Counter />)
    setWatching(true)
    sample()
    for (let i = 0; i < 5; i++) flushSync(() => bump())
    const second = sample()
    setWatching(false)
    const hot = Object.fromEntries(second.hot.map((h) => [h.name, h.perS]))
    expect(hot.Counter).toBeGreaterThan(0)
    expect(hot.Leaf).toBe(hot.Counter)
    expect(hot.Still).toBeUndefined()
    expect(second.commitsPerS).toBeGreaterThan(0)
    t.unmount()
  })
})
