// The settings kit's controls, drawn in the GPU test renderer: what a press, a drag or a key does
// to each, and what `OptionRow` draws for an option and sets.
import { describe, expect, mock, test } from "bun:test"
import { useState, type ReactNode } from "react"
import { createTestRoot, hasNativeTestRenderer } from "../../react/testing.ts"

// The app's engine, without starting it: what the kit imports, and the modules it pulls in.
const haptics: string[] = []
const ids = await import("../../app/ids.ts")
mock.module("../../app/engine.ts", () => ({
  store: { get: () => ({ trees: {}, options: {} }), subscribe: () => () => {} },
  call: () => Promise.resolve(null),
  on: () => () => {},
  engine: { call: () => Promise.resolve(null), haptic: (p: string) => haptics.push(p), provide: () => {}, answer: () => {} },
  // `mock.module` is process-wide: the app's other tests hear the real ids.
  ...ids,
  share: (_: unknown, next: unknown) => next,
  restartApp: () => {},
}))
const { ThemeContext, DARK, METRICS } = await import("../theme.ts")
const { DEFAULT } = await import("../../app/theme/default.tsx")
const kit = await import("./index.ts")
const { SettingsContext } = await import("./context.ts")
const { formatNumber, parse, snap, titleOf, choicesOf } = await import("./options.ts")

const gpu = hasNativeTestRenderer ? describe : describe.skip
const theme = { spec: { name: "test" }, appearance: "dark" as const, tokens: DARK, metrics: METRICS, components: DEFAULT }

function draw(node: ReactNode, size = { width: 600, height: 400 }) {
  const t = createTestRoot(size)
  t.render(<ThemeContext.Provider value={theme as any}>{node}</ThemeContext.Provider>)
  return t
}

/** The centre of the element with `testId`, in window px. */
function centre(t: ReturnType<typeof createTestRoot>, testId: string) {
  const el = t.renderer.findByTestId(testId)
  if (!el) throw new Error(`no ${testId}`)
  const box = t.renderer.getElementBounds(el.id)!
  return { x: box.x + box.width / 2, y: box.y + box.height / 2, box }
}

describe("options", () => {
  test("a number shows its step's decimals and its unit", () => {
    expect(formatNumber(100, 10, "%")).toBe("100%")
    expect(formatNumber(0.3, 0.05)).toBe("0.30")
    expect(formatNumber(20, 1, "px")).toBe("20 px")
  })
  test("a value snaps to its step within its range", () => {
    expect(snap(13.4, 10, 16, 1)).toBe(13)
    expect(snap(1.33, 1, 2, 0.05)).toBe(1.35)
    expect(snap(99, 0, 80, 5)).toBe(80)
    expect(snap(-3, 0, 80, 5)).toBe(0)
  })
  test("text typed becomes the option's type", () => {
    expect(parse({ name: "n", type: "number" }, " 3 ")).toEqual({ value: 3 })
    expect("error" in parse({ name: "n", type: "number" }, "x")).toBe(true)
    expect(parse({ name: "t", type: "table", default: [] }, "")).toEqual({ value: [] })
    expect(parse({ name: "t", type: "table" }, '{"a":1}')).toEqual({ value: { a: 1 } })
  })
  test("an option without a label is titled from its name, and its choices take their labels", () => {
    expect(titleOf({ name: "files.hide_hidden", type: "boolean" })).toBe("Hide hidden")
    expect(choicesOf({ name: "x", type: "string", choices: ["a", "b"], labels: ["Ay"] })).toEqual([
      { value: "a", label: "Ay" },
      { value: "b", label: "b" },
    ])
  })
  test("a colour typed as hex is read as #rrggbb", () => {
    expect(kit.parseHex("#FFAA00")).toBe("#ffaa00")
    expect(kit.parseHex(" ffaa00 ")).toBe("#ffaa00")
    expect(kit.parseHex("#fa0")).toBe("#ffaa00")
    expect(kit.parseHex("")).toBe("")
    expect(kit.parseHex("#ggg")).toBeUndefined()
    expect(kit.parseHex("#12345")).toBeUndefined()
  })
  test("a strip builds only the cards in view and a few either side", () => {
    expect(kit.visibleCards(0, 600, 900, 180)).toEqual([0, 4 + 3])
    const [from, to] = kit.visibleCards(100 * 180, 600, 900, 180)
    expect(from).toBe(97)
    expect(to - from).toBeLessThan(12)
    expect(kit.visibleCards(1e9, 600, 900, 180)).toEqual([900, 900])
    expect(kit.visibleCards(0, 600, 2, 180)).toEqual([0, 2])
  })
  test("a grid fits as many cards as their narrowest allows, and widens them to fill the row", () => {
    expect(kit.gridColumns(632, 200)).toEqual({ columns: 3, width: 202 })
    expect(kit.gridColumns(100, 200)).toEqual({ columns: 1, width: 200 })
    expect(kit.gridColumns(1000, 200).columns).toBe(4)
  })
})

gpu("the kit", () => {
  test("a switch turns on a press and on Space, and taps the trackpad", () => {
    haptics.length = 0
    function On() {
      const [on, setOn] = useState(false)
      return (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <kit.Switch checked={on} onChange={setOn} testId="sw" />
          <text>{on ? "on" : "off"}</text>
        </div>
      )
    }
    const t = draw(<On />)
    const { x, y } = centre(t, "sw")
    t.renderer.nativeSimulateClick(x, y)
    expect(t.renderer.getAllText()).toContain("on")
    t.renderer.nativeSimulateKeystrokes(t.renderer.findByTestId("sw")!.id, "space")
    expect(t.renderer.getAllText()).toContain("off")
    expect(haptics).toEqual(["generic", "generic"])
    t.unmount()
  })

  test("a slider follows a drag in steps, taps each step and harder at an end, and commits where it stops", () => {
    haptics.length = 0
    const changes: number[] = []
    const commits: number[] = []
    const t = draw(
      <div style={{ display: "flex", padding: 20 }}>
        <kit.Slider testId="sl" value={0} min={0} max={10} step={1} width={200} onChange={(v) => changes.push(v)} onCommit={(v) => commits.push(v)} />
      </div>,
    )
    const { box, y } = centre(t, "sl")
    t.renderer.nativeSimulateMouseDown(box.x + box.width * 0.5, y)
    t.renderer.nativeSimulateMouseMove(box.x + box.width * 0.7, y, 0)
    t.renderer.nativeSimulateMouseMove(box.x + box.width + 40, y, 0)
    t.renderer.nativeSimulateMouseUp(box.x + box.width + 40, y)
    expect(changes).toEqual([5, 7, 10])
    expect(commits).toEqual([10])
    expect(haptics).toEqual(["levelChange", "levelChange", "alignment"])
    t.unmount()
  })

  test("a stepper steps within its range", () => {
    function N() {
      const [n, setN] = useState(2)
      return <kit.Stepper testId="st" value={n} min={1} max={3} onChange={setN} unit="pt" />
    }
    const t = draw(<N />)
    const up = centre(t, "st-up")
    t.renderer.nativeSimulateClick(up.x, up.y)
    expect(t.renderer.getAllText()).toContain("3 pt")
    t.renderer.nativeSimulateClick(up.x, up.y)
    expect(t.renderer.getAllText()).toContain("3 pt")
    const down = centre(t, "st-down")
    t.renderer.nativeSimulateClick(down.x, down.y)
    t.renderer.nativeSimulateClick(down.x, down.y)
    expect(t.renderer.getAllText()).toContain("1 pt")
    t.unmount()
  })

  test("a choice grid lays its choices out in rows and picks one", () => {
    const picked: string[] = []
    const choices = ["top-left", "top", "top-right", "bottom-left", "bottom", "bottom-right"].map((value) => ({ value }))
    const t = draw(<kit.ChoiceGrid testId="pos" columns={3} choices={choices} value="top" onChange={(v) => picked.push(v)} />)
    const a = centre(t, "pos-top-left")
    const b = centre(t, "pos-bottom-right")
    expect(b.y).toBeGreaterThan(a.y)
    expect(b.x).toBeGreaterThan(a.x)
    expect(Math.abs(centre(t, "pos-bottom-left").x - a.x)).toBeLessThan(1)
    t.renderer.nativeSimulateClick(b.x, b.y)
    expect(picked).toEqual(["bottom-right"])
    t.unmount()
  })

  test("image cards pick the one pressed and mark the picked one", () => {
    const picked: string[] = []
    const cards = [
      { value: "a", title: "Card A" },
      { value: "b", title: "Card B" },
    ]
    const t = draw(<kit.ImageCards testId="cards" cards={cards} value="a" onChange={(v) => picked.push(v)} />)
    expect(t.renderer.getAllText()).toEqual(expect.arrayContaining(["Card A", "Card B"]))
    const b = centre(t, "cards-b")
    t.renderer.nativeSimulateClick(b.x, b.y)
    expect(picked).toEqual(["b"])
    t.unmount()
  })

  test("a strip opens on the picked card, however far along it is, and shows it", async () => {
    const cards = Array.from({ length: 300 }, (_, i) => ({ value: `s${i}`, title: `Scheme ${i}` }))
    const t = draw(<kit.ImageCards strip testId="strip" cards={cards} value="s150" onChange={() => {}} />)
    for (let i = 0; i < 3; i++) {
      t.renderer.flush()
      t.renderer.dispatchNativeEvents()
      await Bun.sleep(5)
    }
    const strip = centre(t, "strip").box
    const picked = centre(t, "strip-s150")
    expect(picked.x).toBeGreaterThan(strip.x)
    expect(picked.x).toBeLessThan(strip.x + strip.width)
    t.unmount()
  })

  test("a grid fills its space in rows, builds only the rows in view, and opens on the picked card", async () => {
    const cards = Array.from({ length: 300 }, (_, i) => ({ value: `s${i}`, title: `Scheme ${i}` }))
    const t = draw(
      <div style={{ display: "flex", flexDirection: "column", width: 600, height: 400 }}>
        <kit.ImageCards grid testId="grid" cards={cards} value="s200" width={180} height={80} onChange={() => {}} />
      </div>,
    )
    for (let i = 0; i < 3; i++) {
      t.renderer.flush()
      t.renderer.dispatchNativeEvents()
      await Bun.sleep(5)
    }
    const grid = centre(t, "grid").box
    expect(grid.height).toBeLessThanOrEqual(400)
    const picked = centre(t, "grid-s200")
    expect(picked.y).toBeGreaterThan(grid.y)
    expect(picked.y).toBeLessThan(grid.y + grid.height)
    // A row of three, side by side, and only a few rows of the hundred built.
    expect(centre(t, "grid-s199").box.y).toBe(picked.box.y)
    expect(centre(t, "grid-s199").x).toBeLessThan(picked.x)
    expect(t.renderer.findByTestId("grid-s0")).toBeFalsy()
    t.unmount()
  })

  test("a colour picker picks a swatch, its empty one as empty, and takes a hex typed in its custom field", () => {
    const picked: string[] = []
    const invalid: (string | undefined)[] = []
    function C() {
      const [v, setV] = useState("")
      return (
        <kit.ColorPicker
          testId="cp"
          value={v}
          empty="#3474f0"
          onChange={(next) => {
            picked.push(next)
            setV(next)
          }}
          onInvalid={(m) => invalid.push(m)}
        />
      )
    }
    const t = draw(<C />)
    expect(t.renderer.findByTestId("cp-3474f0")).toBeTruthy()
    expect(t.renderer.findByTestId("cp-hex")).toBeFalsy()
    const pink = centre(t, "cp-e5508f")
    t.renderer.nativeSimulateClick(pink.x, pink.y)
    const blue = centre(t, "cp-3474f0")
    t.renderer.nativeSimulateClick(blue.x, blue.y)
    expect(picked).toEqual(["#e5508f", ""])
    const custom = centre(t, "cp-custom")
    t.renderer.nativeSimulateClick(custom.x, custom.y)
    expect(t.renderer.findByTestId("cp-hex")).toBeTruthy()
    t.unmount()
  })

  test("a colour option draws a picker with its hex in the field while it is none of the swatches", () => {
    const decls = [{ name: "ui_accent", type: "string", default: "", placeholder: "#3474f0", control: "color" }]
    const settings = {
      decls,
      decl: (n: string) => decls.find((d) => d.name === n),
      value: () => "#ffffff",
      isSet: () => true,
      set: () => {},
      open: () => {},
      report: () => {},
    }
    const t = draw(
      <SettingsContext.Provider value={settings as any}>
        <kit.OptionRow name="ui_accent" />
      </SettingsContext.Provider>,
      { width: 900, height: 200 },
    )
    expect(t.renderer.findByTestId("color-ui_accent")).toBeTruthy()
    expect(t.renderer.findByTestId("color-ui_accent-hex")).toBeTruthy()
    t.unmount()
  })

  test("a key recorder records a chord and a sequence, and Escape cancels", async () => {
    const got: string[] = []
    let cancelled = 0
    const t = draw(<kit.KeyRecorder testId="rec" onRecord={(k) => got.push(k)} onCancel={() => cancelled++} />)
    const id = t.renderer.findByTestId("rec")!.id
    t.renderer.nativeSimulateKeystrokes(id, "cmd-k")
    t.renderer.nativeSimulateKeystrokes(id, "cmd-j")
    expect(t.renderer.getAllText().join(" ")).toContain("⌘ K")
    await Bun.sleep(kit.RECORD_PAUSE + 100)
    expect(got).toEqual(["cmd+k cmd+j"])
    t.renderer.nativeSimulateKeystrokes(id, "escape")
    expect(cancelled).toBeGreaterThan(0)
    t.unmount()
  })

  test("a section draws a heading over its rows, and a row its title, what it means and its control", () => {
    const t = draw(
      <kit.Page title="Remote">
        <kit.Section title="Tailscale">
          <kit.Row icon="link" title="Sign in to Tailscale" description="Access pumi from your other devices." control={<kit.Button primary onClick={() => {}}>Sign In</kit.Button>} />
          {false}
          <kit.Row title="Second" />
        </kit.Section>
      </kit.Page>,
    )
    expect(t.renderer.getAllText()).toEqual(expect.arrayContaining(["Remote", "Tailscale", "Sign in to Tailscale", "Access pumi from your other devices.", "Sign In", "Second"]))
    t.unmount()
  })

  test("an option row draws the control its type calls for, sets through Settings, and resets", () => {
    const set: [string, unknown][] = []
    const decls = [
      { name: "me.x.on", type: "boolean", default: false, label: "On", desc: "Turns it on." },
      { name: "me.x.size", type: "number", default: 12, min: 10, max: 16, step: 1, unit: "pt" },
      { name: "me.x.pos", type: "string", default: "top", choices: ["top", "bottom"], labels: ["Top", "Bottom"] },
    ]
    const values: Record<string, unknown> = { "me.x.size": 14 }
    const settings = {
      decls,
      decl: (n: string) => decls.find((d) => d.name === n),
      value: (n: string) => (values[n] ?? decls.find((d) => d.name === n)?.default ?? null) as any,
      isSet: (n: string) => n in values,
      set: (n: string, v: unknown) => set.push([n, v]),
      open: () => {},
      report: () => {},
    }
    const t = draw(
      <SettingsContext.Provider value={settings as any}>
        <kit.Section>
          <kit.OptionRow name="me.x.on" />
          <kit.OptionRow name="me.x.size" />
          <kit.OptionRow name="me.x.pos" />
          <kit.OptionRow name="me.x.missing" />
        </kit.Section>
      </SettingsContext.Provider>,
      { width: 700, height: 400 },
    )
    const text = t.renderer.getAllText()
    expect(text).toEqual(expect.arrayContaining(["On", "Turns it on.", "Size", "14 pt", "Pos", "Top"]))
    const sw = centre(t, "switch-me.x.on")
    t.renderer.nativeSimulateClick(sw.x, sw.y)
    const up = centre(t, "stepper-me.x.size-up")
    t.renderer.nativeSimulateClick(up.x, up.y)
    const reset = centre(t, "reset-me.x.size")
    t.renderer.nativeSimulateClick(reset.x, reset.y)
    expect(set).toEqual([
      ["me.x.on", true],
      ["me.x.size", 15],
      ["me.x.size", null],
    ])
    t.unmount()
  })
})
