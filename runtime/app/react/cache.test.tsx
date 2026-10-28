// `cache`: a subtree drawn by a view of its own, drawn again as it was while nothing in it changes.
import { describe, expect, test } from "bun:test"
import { useState } from "react"
import { onSlowLoop } from "./components/index.ts"
import { motion } from "./index.ts"
import { flushSync } from "./reconciler/reconciler.ts"
import { createTestRoot, hasNativeTestRenderer } from "./testing.ts"

const gpu = hasNativeTestRenderer ? describe : describe.skip

function Rows({ prefix, n }: { prefix: string; n: number }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
      {Array.from({ length: n }, (_, i) => (
        <text key={i} testId={`${prefix}${i}`}>
          {`${prefix} row ${i}`}
        </text>
      ))}
    </div>
  )
}

gpu("cache", () => {
  test("a cached subtree's text and boxes stay registered on frames it is not drawn", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    let bump = () => {}
    function App() {
      const [n, setN] = useState(0)
      bump = () => setN((v) => v + 1)
      return (
        <div style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%" }}>
          <div cache style={{ width: 150, height: "100%" }}>
            <Rows prefix="side" n={3} />
          </div>
          <div style={{ flexGrow: 1 }}>
            <text>{`count ${n}`}</text>
          </div>
        </div>
      )
    }
    t.render(<App />)
    t.renderer.flush()
    const before = t.renderer.getPaintedText()
    expect(before).toContain("side row 1")
    // A change outside the cached subtree: the frame reuses it.
    flushSync(() => bump())
    t.renderer.flush()
    t.renderer.flush()
    const after = t.renderer.getPaintedText()
    expect(after).toContain("side row 1")
    expect(after).toContain("count 1")
    // In the same paint order as before.
    expect(after.filter((s) => s.startsWith("side"))).toEqual(before.filter((s) => s.startsWith("side")))
    t.unmount()
  })

  test("a change inside a cached subtree draws it again", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    let set: (label: string) => void = () => {}
    function App() {
      const [label, setLabel] = useState("first")
      set = setLabel
      return (
        <div style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%" }}>
          <div cache style={{ width: 200, height: "100%" }}>
            <text>{label}</text>
          </div>
          <text>outside</text>
        </div>
      )
    }
    t.render(<App />)
    t.renderer.flush()
    expect(t.renderer.getPaintedText()).toContain("first")
    flushSync(() => set("second"))
    t.renderer.flush()
    const painted = t.renderer.getPaintedText()
    expect(painted).toContain("second")
    expect(painted).not.toContain("first")
    t.unmount()
  })

  test("a cached subtree is laid out from its own style, and hit like any other", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    const clicks: string[] = []
    function App() {
      const [n, setN] = useState(0)
      return (
        <div style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%" }}>
          <div style={{ width: 50 }} />
          <div cache style={{ width: 120, height: 80, padding: 10, marginTop: 20 }}>
            <div
              style={{ width: "100%", height: 30 }}
              onClick={() => {
                clicks.push("inner")
                setN((v) => v + 1)
              }}
            >
              <text>{`clicked ${n}`}</text>
            </div>
          </div>
        </div>
      )
    }
    t.render(<App />)
    t.renderer.flush()
    t.renderer.flush()
    // The click lands inside the cached subtree on a frame that reused it.
    t.renderer.nativeSimulateClick(80, 40, 0, "")
    t.renderer.flush()
    expect(clicks).toEqual(["inner"])
    expect(t.renderer.getPaintedText()).toContain("clicked 1")
    const hit = t.renderer.getElementsAt(80, 40)
    expect(hit.length).toBeGreaterThan(0)
    const box = t.renderer.getElementBounds(hit[0]!)
    // The inner row: inside the padding of a box at x 50, y 20.
    expect(box?.x).toBe(60)
    expect(box?.y).toBe(30)
    expect(box?.width).toBe(100)
    t.unmount()
  })

  test("a cached subtree inside one drawn again is reused, and stays right when its parent is reused", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    const clicks: number[] = []
    let bumpOuter = () => {}
    let bumpInner = () => {}
    function Inner() {
      const [n, setN] = useState(0)
      bumpInner = () => setN((v) => v + 1)
      return (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ width: 100, height: 20 }} onClick={() => clicks.push(n)}>
            <text>{`inner ${n}`}</text>
          </div>
          <Rows prefix="nested" n={3} />
        </div>
      )
    }
    function App() {
      const [n, setN] = useState(0)
      bumpOuter = () => setN((v) => v + 1)
      return (
        <div style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%" }}>
          <div cache style={{ display: "flex", flexDirection: "column", width: 200, height: "100%" }}>
            <text>{`outer ${n}`}</text>
            <div cache style={{ width: 200, height: 200 }}>
              <Inner />
            </div>
          </div>
          <text>{`beside ${n}`}</text>
        </div>
      )
    }
    t.render(<App />)
    t.renderer.flush()
    t.renderer.flush()
    const nested = (painted: string[]) => painted.filter((s) => s.startsWith("nested") || s.startsWith("inner"))
    const first = nested(t.renderer.getPaintedText())
    expect(first).toEqual(["inner 0", "nested row 0", "nested row 1", "nested row 2"])
    // The outer subtree changes, the inner does not: again and again, then the outer is reused
    // whole (frames with nothing new), then it changes again. Each time the inner subtree is
    // drawn as it was, and is hit where it is.
    for (const quiet of [0, 0, 2, 0, 3]) {
      flushSync(() => bumpOuter())
      t.renderer.flush()
      for (let i = 0; i < quiet; i++) t.renderer.flush()
      expect(nested(t.renderer.getPaintedText())).toEqual(first)
    }
    expect(t.renderer.getPaintedText()).toContain("outer 5")
    t.renderer.nativeSimulateClick(20, 30, 0, "")
    t.renderer.flush()
    expect(clicks).toEqual([0])
    // A change inside the inner subtree still draws it again.
    flushSync(() => bumpInner())
    t.renderer.flush()
    expect(t.renderer.getPaintedText()).toContain("inner 1")
    flushSync(() => bumpOuter())
    t.renderer.flush()
    t.renderer.flush()
    expect(nested(t.renderer.getPaintedText())[0]).toBe("inner 1")
    t.renderer.nativeSimulateClick(20, 30, 0, "")
    t.renderer.flush()
    expect(clicks).toEqual([0, 1])
    t.unmount()
  })

  test("a motion in a cached subtree keeps moving", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    t.renderer.clockPause()
    t.render(
      <div style={{ display: "flex", width: "100%", height: "100%" }}>
        <div cache style={{ width: 200, height: 100, position: "relative" }}>
          <motion.div
            animate={{ left: [0, 100, 0] }}
            transition={{ duration: 1, repeat: Infinity, ease: "linear" }}
            style={{ position: "absolute", top: 0, left: 0, width: 4, height: 4 }}
          />
        </div>
      </div>,
    )
    t.renderer.flush()
    const at = (ms: number) => {
      t.renderer.clockFastForward(ms)
      t.renderer.flush()
      const tree = JSON.parse(t.renderer.getAutomationTree())
      // The dot: the only 4 px box.
      const find = (n: any): any => (n.bounds?.width === 4 ? n : (n.children ?? []).map(find).find(Boolean))
      return find(tree)?.bounds?.x
    }
    const first = at(0)
    const later = at(250)
    expect(typeof first).toBe("number")
    expect(later).toBeGreaterThan(first + 20)
    t.unmount()
  })
})

gpu("compartment", () => {
  const boxOf = (t: ReturnType<typeof createTestRoot>, testId: string) => {
    const tree = JSON.parse(t.renderer.getAutomationTree())
    const find = (n: any): any => (n.testId === testId ? n : (n.children ?? []).map(find).find(Boolean))
    return find(tree)?.bounds
  }

  test("sizes as its child would without it, through frames that reuse it", () => {
    let bump = () => {}
    let grow = () => {}
    function Content() {
      const [n, setN] = useState(2)
      grow = () => setN((v) => v + 1)
      return (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <Rows prefix="c" n={n} />
        </div>
      )
    }
    function App({ compartment }: { compartment: boolean }) {
      const [n, setN] = useState(0)
      bump = () => setN((v) => v + 1)
      const content = <Content />
      return (
        <div style={{ display: "flex", flexDirection: "column", width: "100%", height: "100%" }}>
          <text>{`top ${n}`}</text>
          {compartment ? <div compartment>{content}</div> : content}
          <div testId="after" style={{ height: 10 }} />
        </div>
      )
    }
    const plain = createTestRoot({ width: 400, height: 300 })
    plain.render(<App compartment={false} />)
    plain.renderer.flush()
    const expected = boxOf(plain, "after")
    plain.unmount()

    const t = createTestRoot({ width: 400, height: 300 })
    t.render(<App compartment />)
    t.renderer.flush()
    expect(boxOf(t, "after")).toEqual(expected)
    // Frames where only what is outside changes: the compartment is reused at the size it measured.
    for (let i = 0; i < 3; i++) {
      flushSync(() => bump())
      t.renderer.flush()
      expect(boxOf(t, "after")).toEqual(expected)
      expect(t.renderer.getPaintedText()).toContain("c row 1")
    }
    // Its contents grow: what follows it moves down.
    flushSync(() => grow())
    t.renderer.flush()
    t.renderer.flush()
    expect(boxOf(t, "after").y).toBeGreaterThan(expected.y)
    expect(t.renderer.getPaintedText()).toContain("c row 2")
    t.unmount()
  })

  test("follows its box when the box changes", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    let widen = () => {}
    function App() {
      const [width, setWidth] = useState(200)
      widen = () => setWidth(300)
      return (
        <div style={{ display: "flex", flexDirection: "column", width, height: "100%" }}>
          <div compartment>
            <div testId="inner" style={{ height: 20 }} />
          </div>
        </div>
      )
    }
    t.render(<App />)
    t.renderer.flush()
    t.renderer.flush()
    expect(boxOf(t, "inner").width).toBe(200)
    flushSync(() => widen())
    for (let i = 0; i < 3; i++) t.renderer.flush()
    expect(boxOf(t, "inner").width).toBe(300)
    t.unmount()
  })

  test("a flex child keeps flexing", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    let bump = () => {}
    function App() {
      const [n, setN] = useState(0)
      bump = () => setN((v) => v + 1)
      return (
        <div style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%" }}>
          <div style={{ width: 100 }}>
            <text>{`side ${n}`}</text>
          </div>
          <div compartment>
            <div testId="grow" style={{ flexGrow: 1, height: "100%" }}>
              <text>body</text>
            </div>
          </div>
        </div>
      )
    }
    t.render(<App />)
    t.renderer.flush()
    expect(boxOf(t, "grow")).toMatchObject({ x: 100, width: 300, height: 300 })
    flushSync(() => bump())
    t.renderer.flush()
    t.renderer.flush()
    expect(boxOf(t, "grow")).toMatchObject({ x: 100, width: 300, height: 300 })
    t.unmount()
  })

  test("a slow one draws on a budget, and what it was given shows in its turn", () => {
    const t = createTestRoot({ width: 400, height: 600 })
    let set = (_: string) => {}
    let tick = () => {}
    function Slow() {
      const [label, setLabel] = useState("first")
      set = setLabel
      return (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <text>{label}</text>
          <Rows prefix="slow" n={2500} />
        </div>
      )
    }
    function Beside() {
      const [n, setN] = useState(0)
      tick = () => setN((v) => v + 1)
      return <text>{`beside ${n}`}</text>
    }
    t.render(
      <div style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%" }}>
        <div style={{ display: "flex", flexDirection: "column", width: 200, height: "100%", overflowY: "scroll" }}>
          <div compartment="me/slow">
            <Slow />
          </div>
        </div>
        <Beside />
      </div>,
    )
    // Drawn in place, then reused: the second frame is its cached render, which it is timed by.
    for (let i = 0; i < 3; i++) t.renderer.flush()
    const painted = () => t.renderer.getPaintedText()
    expect(painted()).toContain("first")
    // Changed twice in a row: what is beside it draws every frame meanwhile, and it shows the
    // last change once its turn comes.
    flushSync(() => set("second"))
    t.renderer.flush()
    flushSync(() => set("third"))
    flushSync(() => tick())
    t.renderer.flush()
    expect(painted()).toContain("beside 1")
    const until = performance.now() + 3000
    while (!painted().includes("third") && performance.now() < until) {
      Bun.sleepSync(20)
      t.renderer.flush()
    }
    expect(painted()).toContain("third")
    expect(painted()).not.toContain("second")
    // Its cost, by its name: what the performance overlay shows.
    const stats = t.renderer.getCompartmentStats()
    expect(stats.find((s) => s.name === "me/slow")).toMatchObject({ count: 1 })
    expect(stats.find((s) => s.name === "me/slow")!.renders).toBeGreaterThan(0)
    expect(stats.find((s) => s.name === "me/slow")!.deferred).toBeGreaterThan(0)
    t.unmount()
  })

  test("a motion draws again the compartment it is in, not the ones beside it", () => {
    const t = createTestRoot({ width: 400, height: 300 })
    t.renderer.clockPause()
    t.render(
      <div cache style={{ display: "flex", flexDirection: "column", width: 200, height: "100%" }}>
        <div compartment="me/moving">
          <div style={{ position: "relative", width: 14, height: 14 }}>
            <motion.div
              animate={{ left: [0, 10, 0] }}
              transition={{ duration: 1, repeat: Infinity, ease: "linear", fps: 16 }}
              style={{ position: "absolute", top: 0, left: 0, width: 4, height: 4 }}
            />
          </div>
        </div>
        <div compartment="me/still">
          <Rows prefix="still" n={3} />
        </div>
      </div>,
    )
    for (let i = 0; i < 3; i++) t.renderer.flush()
    // The motion's first frame draws the whole tree once, as it starts; what is measured is after.
    t.renderer.clockFastForward(63)
    t.renderer.flush()
    t.renderer.getCompartmentStats()
    for (let i = 0; i < 8; i++) {
      t.renderer.clockFastForward(63)
      t.renderer.flush()
    }
    const stats = t.renderer.getCompartmentStats()
    expect(stats.find((s) => s.name === "me/moving")!.renders).toBeGreaterThan(4)
    expect(stats.find((s) => s.name === "me/still")?.renders ?? 0).toBe(0)
    expect(t.renderer.getPaintedText()).toContain("still row 2")
    t.unmount()
  })

  test("an endless motion whose frames draw too much of the window is slowed, and says so once", () => {
    const spinner = (
      <div style={{ position: "relative", width: 14, height: 14 }}>
        <motion.div
          animate={{ left: [0, 10, 0] }}
          transition={{ duration: 1, repeat: Infinity, ease: "linear", fps: 16 }}
          style={{ position: "absolute", top: 0, left: 0, width: 4, height: 4 }}
        />
      </div>
    )
    const row = (i: number) => (
      <div key={i} style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, minHeight: 28 }}>
        <div style={{ width: 12, height: 12 }} />
        <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0 }}>
          <text>{`row ${i}`}</text>
          <text>{`/a/path/${i}`}</text>
        </div>
        <div style={{ display: "flex", flexDirection: "row", gap: 2 }}>
          <div style={{ width: 16, height: 16 }} />
          <div style={{ width: 16, height: 16 }} />
        </div>
      </div>
    )
    const told = (rowsAlone: boolean) => {
      const names: (string | undefined)[] = []
      const restore = onSlowLoop((name) => names.push(name))
      const t = createTestRoot({ width: 400, height: 300 })
      // A side bar: a cached tree, drawn again whole whenever anything in it moves, unless its
      // rows are kept apart. (Not in a compartment: one that is slow draws on a budget already.)
      const rows = Array.from({ length: 400 }, (_, i) => row(i))
      t.render(
        <div cache style={{ display: "flex", flexDirection: "column", width: 300, height: 300 }}>
          {spinner}
          {rowsAlone ? (
            <div compartment="me/rows">
              <div style={{ display: "flex", flexDirection: "column" }}>{rows}</div>
            </div>
          ) : (
            rows
          )}
        </div>,
      )
      // Its own timed repaints draw the frames it is judged by: real time passes.
      for (let i = 0; i < 12; i++) {
        Bun.sleepSync(70)
        t.renderer.flush()
      }
      t.renderer.dispatchNativeEvents()
      t.unmount()
      restore()
      return names
    }
    // Each frame it moves in lays every row out again: told once (it is in no named compartment).
    expect(told(false)).toEqual([undefined])
    // With the rows in a compartment of their own, a frame it moves in draws little.
    expect(told(true)).toEqual([])
    // Two windows of 400 rows, each drawn for most of a second: a CI machine takes over bun's 5s.
  }, 30_000)
})
