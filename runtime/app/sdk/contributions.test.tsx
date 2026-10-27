// What the app draws a plugin's component and a theme's parts in: a compartment around one box,
// and a component drawn again only when what it is given changes.
import { describe, expect, mock, test } from "bun:test"
import { useState, type ReactNode } from "react"
import { flushSync } from "../react/reconciler/reconciler.ts"
import { createTestRoot, hasNativeTestRenderer } from "../react/testing.ts"

// The window's state, without starting the app's engine (which re-exports the wire ids' helpers).
const ids = await import("../app/ids.ts")
mock.module("../app/engine.ts", () => ({ ...ids, store: { get: () => ({ trees: {}, options: {} }), subscribe: () => () => {} }, call: () => Promise.resolve(null), on: () => () => {} }))
const { fitting, inCompartment, renderComponent } = await import("./contributions.ts")
type ComponentItem = import("./contributions.ts").ComponentItem

const gpu = hasNativeTestRenderer ? describe : describe.skip

describe("fitting", () => {
  const item = (plugin: string, match?: string[]): ComponentItem => ({ id: "file", plugin, match, Component: () => null })
  const editor = item("me/editor", ["*"])
  const image = item("me/image", ["*.png", "*.svg"])
  const markdown = item("me/markdown", ["*.md"])

  test("lists what fits a file for Open With, the best first", () => {
    const names = (path: string) => fitting([editor, image, markdown], path).map((r) => r.plugin)
    expect(names("/a/logo.png")).toEqual(["me/image", "me/editor"])
    expect(names("/a/main.ts")).toEqual(["me/editor"])
    expect(fitting([image, markdown], "/a/main.ts")).toEqual([])
  })
})

/** The compartments in the tree: the name each has, and the type of its one child. */
function compartments(t: ReturnType<typeof createTestRoot>) {
  return t.renderer
    .findByType("div")
    .filter((el) => el.customProps?.compartment !== undefined)
    .map((el) => ({ name: el.customProps!.compartment, child: el.children.map((id) => t.renderer.getElement(id)?.type) }))
}

gpu("inCompartment", () => {
  test("puts one around a component that draws one box, and keeps its hooks", () => {
    const t = createTestRoot({ width: 300, height: 200 })
    let bump = () => {}
    function Counter() {
      const [n, setN] = useState(0)
      bump = () => setN((v) => v + 1)
      return (
        <div style={{ display: "flex" }}>
          <text>{`count ${n}`}</text>
        </div>
      )
    }
    const C = inCompartment(Counter, "me/counter")
    expect(inCompartment(Counter, "me/counter")).toBe(C)
    t.render(<C />)
    expect(compartments(t)).toEqual([{ name: "me/counter", child: ["div"] }])
    flushSync(() => bump())
    t.renderer.flush()
    expect(t.renderer.getAllText()).toContain("count 1")
    t.unmount()
  })

  test("leaves what is not one box as it is", () => {
    const t = createTestRoot({ width: 300, height: 200 })
    const Fragment = inCompartment(() => (
      <>
        <text>a</text>
        <text>b</text>
      </>
    ), "me/fragment")
    const Nothing = inCompartment(() => null, "me/nothing")
    function Inner() {
      return <text>inner</text>
    }
    const Composite = inCompartment(() => <Inner />, "me/composite")
    t.render(
      <div style={{ display: "flex", flexDirection: "row" }}>
        <Fragment />
        <Nothing />
        <Composite />
      </div>,
    )
    expect(compartments(t)).toEqual([])
    expect(t.renderer.getAllText()).toEqual(expect.arrayContaining(["a", "b", "inner"]))
    t.unmount()
  })
})

gpu("renderComponent", () => {
  test("draws a component again when what it is given changes, not when the theme around it does", () => {
    const t = createTestRoot({ width: 300, height: 200 })
    let renders = 0
    // A component is given ids (`%1`), as the hooks give them.
    function Plugin({ tab }: { tab: string }) {
      renders++
      return <text>{`tab ${tab}`}</text>
    }
    const item: ComponentItem = { id: "component", plugin: "me/plugin", Component: Plugin }
    let bump = () => {}
    let move = () => {}
    function Theme() {
      const [n, setN] = useState(0)
      const [tab, setTab] = useState("%1")
      bump = () => setN((v) => v + 1)
      move = () => setTab("%2")
      const drawn: ReactNode = renderComponent(item, { tab })
      return (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <text>{`theme ${n}`}</text>
          {drawn}
        </div>
      )
    }
    t.render(<Theme />)
    expect(renders).toBe(1)
    flushSync(() => bump())
    flushSync(() => bump())
    expect(renders).toBe(1)
    flushSync(() => move())
    expect(renders).toBe(2)
    t.renderer.flush()
    expect(t.renderer.getAllText()).toContain("tab %2")
    t.unmount()
  })
})
