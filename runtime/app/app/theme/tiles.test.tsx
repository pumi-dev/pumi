import { describe, expect, test } from "bun:test"
import { useEffect } from "react"
import { createTestRoot, hasNativeTestRenderer } from "../../react/testing.ts"
import type { Tree } from "../state.ts"
import { ThemeContext, type ThemeState } from "../../sdk/theme.ts"
import { useDraggable } from "../../sdk/drag.ts"
import type { Move } from "./moves.ts"
import { boxesOf, seamOf, TileMotion, Tiling } from "./tiles.tsx"

/** A pane holding one tab of the same number. */
const leaf = (pane: string): Tree => ({ pane, tabs: [pane], active: pane })
const row = (ratio: number, first: Tree, second: Tree): Tree => ({ axis: "row", ratio, first, second })
const col = (ratio: number, first: Tree, second: Tree): Tree => ({ axis: "column", ratio, first, second })

/** What the app folds `[%1, %2]` into, and `[%1, %3, %2]` once %1 is split to its right. */
const two = row(0.5, leaf("%1"), leaf("%2"))
const three = row(1 / 3, leaf("%1"), row(0.5, leaf("%3"), leaf("%2")))

describe("boxesOf", () => {
  test("each pane gets its share of the space", () => {
    const boxes = boxesOf(three)
    expect(boxes.get("%1")).toEqual({ x: 0, y: 0, w: 1 / 3, h: 1 })
    expect(boxes.get("%3")!.x).toBeCloseTo(1 / 3)
    expect(boxes.get("%2")!.x).toBeCloseTo(2 / 3)
  })
})

describe("seamOf", () => {
  test("a pane opened grows out of the seam it was split at", () => {
    // %3 went in between %1 and %2: it starts where they met, the whole height.
    expect(seamOf(three, "%3", boxesOf(two))).toEqual({ x: 0.5, y: 0, w: 0, h: 1 })
  })

  test("a pane opened below another grows up from its foot", () => {
    const split = row(0.5, leaf("%1"), col(0.5, leaf("%2"), leaf("%3")))
    expect(seamOf(split, "%3", boxesOf(two))).toEqual({ x: 0.5, y: 1, w: 0.5, h: 0 })
  })

  test("a pane closed shrinks into the seam its neighbours close up at", () => {
    expect(seamOf(three, "%3", boxesOf(two))).toEqual({ x: 0.5, y: 0, w: 0, h: 1 })
    expect(seamOf(two, "%2", boxesOf(leaf("%1")))).toEqual({ x: 1, y: 0, w: 0, h: 1 })
  })

  test("nothing when no neighbour was drawn", () => {
    expect(seamOf(two, "%2", new Map())).toBeUndefined()
  })
})

describe.skipIf(!hasNativeTestRenderer)("Tiling", () => {
  test("a split or a close reshapes the tree but mounts no pane again", () => {
    const mounts: string[] = []
    function Pane({ pane }: { pane: string }) {
      useEffect(() => void mounts.push(pane), [])
      return <text>{pane}</text>
    }
    const theme = { tokens: { border: "#333333", accent: "#4488ff" }, components: { Pane, Splitter: () => null } } as unknown as ThemeState
    const { render, renderer, unmount } = createTestRoot()
    const draw = (tree: Tree) =>
      render(
        <ThemeContext.Provider value={theme}>
          <Tiling tree={tree} />
        </ThemeContext.Provider>,
      )
    draw(leaf("%1"))
    draw(two)
    draw(three)
    expect(mounts).toEqual(["%1", "%2", "%3"])
    draw(two)
    draw(leaf("%1"))
    expect(mounts).toEqual(["%1", "%2", "%3"])
    expect(renderer.getAllText()).toContain("%1")
    unmount()
  })

  test("a zoomed pane stops short of the edges, over the others, which stay put", async () => {
    function Pane({ pane }: { pane: string }) {
      return <div testId={pane} style={{ flexGrow: 1 }} />
    }
    // SAFETY: the tiles read only these tokens and this one part.
    const theme = { tokens: { border: "#333333", accent: "#4488ff", editor: "#1e1f22" }, components: { Pane, Splitter: () => null } } as unknown as ThemeState
    const { render, renderer, unmount } = createTestRoot({ width: 900, height: 300 })
    const settle = async (ms: number) => {
      for (let waited = 0; waited <= ms; waited += 20) {
        renderer.flush()
        renderer.dispatchNativeEvents()
        await Bun.sleep(20)
      }
      renderer.flush()
    }
    const draw = (zoom?: string) =>
      render(
        <ThemeContext.Provider value={theme}>
          <TileMotion.Provider value={{ zoom }}>
            <Tiling tree={two} />
          </TileMotion.Provider>
        </ThemeContext.Provider>,
      )
    const bounds = (pane: string) => renderer.getElementBounds(renderer.findByTestId(pane)!.id)!
    draw()
    await settle(60)
    const before = bounds("%2")
    draw("%1")
    await settle(300)
    const zoomed = bounds("%1")
    expect(zoomed.x).toBeGreaterThan(10)
    expect(zoomed.x).toBeLessThan(20)
    expect(zoomed.width).toBeGreaterThan(860)
    expect(zoomed.width).toBeLessThan(880)
    expect(bounds("%2")).toEqual(before)
    unmount()
  })

  test("a press beside the zoomed pane unzooms, and does not reach the pane under it", async () => {
    const pressed: string[] = []
    function Pane({ pane }: { pane: string }) {
      return <div testId={pane} onMouseDown={() => pressed.push(pane)} style={{ flexGrow: 1 }} />
    }
    const theme = { tokens: { border: "#333333", accent: "#4488ff", editor: "#1e1f22" }, components: { Pane, Splitter: () => null } } as unknown as ThemeState
    const { render, renderer, unmount } = createTestRoot({ width: 900, height: 300 })
    let unzoomed = 0
    render(
      <ThemeContext.Provider value={theme}>
        <TileMotion.Provider value={{ zoom: "%1", unzoom: () => void unzoomed++ }}>
          <Tiling tree={two} />
        </TileMotion.Provider>
      </ThemeContext.Provider>,
    )
    for (let i = 0; i < 15; i++) {
      renderer.flush()
      renderer.dispatchNativeEvents()
      await Bun.sleep(20)
    }
    renderer.flush()
    // In the inset at the right, over %2.
    renderer.nativeSimulateMouseDown(895, 150)
    renderer.nativeSimulateMouseUp(895, 150)
    expect(unzoomed).toBe(1)
    expect(pressed).toEqual([])
    // On the zoomed pane itself: it gets the press.
    renderer.nativeSimulateMouseDown(450, 150)
    renderer.nativeSimulateMouseUp(450, 150)
    expect(unzoomed).toBe(1)
    expect(pressed).toEqual(["%1"])
    unmount()
  })

  test("a tab dragged onto a side of another pane splits it there; onto its middle, joins its tabs", async () => {
    // A pane whose whole box is its handle, as the default theme's title bar is.
    function Pane({ pane }: { pane: string }) {
      const drag = useDraggable({ kind: "pumi.tab", tab: pane })
      return <div testId={pane} {...drag.props} style={{ flexGrow: 1, backgroundColor: "#202020" }} />
    }
    const theme = { tokens: { border: "#333333", accent: "#4488ff", editor: "#1e1f22" }, components: { Pane, Splitter: () => null } } as unknown as ThemeState
    const { render, renderer, unmount } = createTestRoot({ width: 800, height: 400 })
    const moved: Move[] = []
    const edit = { resize: async () => {}, equalize() {}, move: (m: Move) => void moved.push(m) }
    render(
      <ThemeContext.Provider value={theme}>
        <Tiling tree={two} edit={edit} />
      </ThemeContext.Provider>,
    )
    renderer.flush()
    const drag = (to: { x: number; y: number }) => {
      renderer.nativeSimulateMouseDown(200, 200)
      renderer.nativeSimulateMouseMove(260, 200, 0)
      renderer.nativeSimulateMouseMove(to.x, to.y, 0)
      renderer.nativeSimulateMouseMove(to.x, to.y, 0)
      renderer.nativeSimulateMouseUp(to.x, to.y)
    }
    drag({ x: 760, y: 200 })
    drag({ x: 600, y: 380 })
    drag({ x: 590, y: 200 })
    // Onto itself: nothing.
    drag({ x: 100, y: 200 })
    expect(moved).toEqual([
      { kind: "tab.split", tab: "%1", pane: "%2", side: "right" },
      { kind: "tab.split", tab: "%1", pane: "%2", side: "down" },
      { kind: "tab.into", tab: "%1", pane: "%2" },
    ])
    unmount()
  })
})

describe.skipIf(!hasNativeTestRenderer)("useDraggable", () => {
  test("a button on a handle is clicked; a press on it that moves drags the handle", () => {
    const log: string[] = []
    // As the default theme's pane title bar: the handle, and a button on it with no background.
    function Bar() {
      const drag = useDraggable("bar", { onDragEnd: () => void log.push("dragged") })
      return (
        <div {...drag.props} style={{ display: "flex", width: 400, height: 30 }}>
          <div onClick={() => log.push("click")} style={{ width: 24, height: 24, hover: { backgroundColor: "#333333" } }} />
        </div>
      )
    }
    const { render, renderer, unmount } = createTestRoot({ width: 800, height: 400 })
    render(<Bar />)
    renderer.flush()
    // A click, with the jitter a press has.
    renderer.nativeSimulateMouseDown(10, 10)
    renderer.nativeSimulateMouseMove(11, 10, 0)
    renderer.nativeSimulateMouseUp(11, 10)
    expect(log).toEqual(["click"])
    // From the button, off the handle: a drag to the end, and no click.
    renderer.nativeSimulateMouseDown(10, 10)
    renderer.nativeSimulateMouseMove(60, 10, 0)
    renderer.nativeSimulateMouseMove(600, 300, 0)
    renderer.nativeSimulateMouseUp(600, 300)
    expect(log).toEqual(["click", "dragged"])
    unmount()
  })
})
