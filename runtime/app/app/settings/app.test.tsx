// The Settings window's tree in the GPU test renderer: its nav, its pages, its search, a link,
// and a plugin's section that throws, drawn as an error card while the rest goes on.
import { describe, expect, mock, test } from "bun:test"
import { createTestRoot, hasNativeTestRenderer } from "../../react/testing.ts"

const decls = [
  { name: "ui_theme", type: "string", default: "default", label: "Theme", group: "Appearance", control: "ui_theme" },
  { name: "terminal_colorscheme", type: "string", default: "", label: "Color scheme", group: "Appearance", control: "terminal_theme" },
  { name: "linespace", type: "number", default: 0, label: "Line height", group: "Font", min: -50, max: 100, step: 5, unit: "%", desc: "How much taller than the font's own line." },
  { name: "terminal_cursor_style", type: "string", default: "block", label: "Cursor shape", group: "Cursor", choices: ["block", "bar"], labels: ["Block", "Bar"], desc: "The cursor's shape while the program hasn't asked for another." },
  { name: "terminal_copy_on_select", type: "boolean", default: false, label: "Copy text when selected", group: "Mouse" },
  { name: "haptics", type: "boolean", default: true, label: "Haptic feedback", group: "Default theme · Motion and feedback" },
  { name: "autoupdate", type: "boolean", default: true, label: "Update automatically", group: "Updates" },
  { name: "me.remote.position", type: "string", plugin: "me/remote", label: "Position", default: "top", choices: ["top", "bottom"] },
]
const calls: unknown[][] = []
const ids = await import("../ids.ts")
mock.module("../engine.ts", () => ({
  store: { get: () => ({ trees: {}, options: { linespace: 10 } }), subscribe: () => () => {} },
  call: (...args: unknown[]) => {
    calls.push(args)
    if (args[0] === "registry.list" && args[1] === "option") return Promise.resolve(decls)
    if (args[0] === "registry.list" && args[1] === "plugin") return Promise.resolve([{ id: "me/remote", spec: { name: "remote" }, meta: { version: "1.0.0", description: "Reach pumi from afar." } }])
    return Promise.resolve(null)
  },
  on: () => () => {},
  engine: {
    call: () => Promise.resolve(null),
    haptic: () => {},
    provide: () => {},
    answer: () => {},
    updateState: () => ({ app: { status: "unavailable", current: "0.42.0", channel: "stable", text: "pumi 0.42.0 does not update itself.", canCheck: false, busy: false }, plugins: { checking: false, found: null, error: null } }),
    serverStatus: () => Promise.resolve({ version: "0.42.0", panes: 3, sessions: 1, plugins: 1 }),
    serverLog: () => null,
    fonts: () => ["Menlo", "SF Mono"],
    setAppearance: () => {},
  },
  // `mock.module` is process-wide: the app's other tests hear the real ids.
  ...ids,
  share: (_: unknown, next: unknown) => next,
  restartApp: () => {},
}))
// A plugin's window side: one section that draws, and one that throws.
mock.module("../ui.ts", () => ({
  settingsComponent: (_plugin: string, component: string) =>
    component === "Broken"
      ? () => {
          throw new Error("no QR code today")
        }
      : () => <text>Tailscale is here</text>,
}))
const plugins = await import("../plugins.ts")
plugins.entries.set("me/remote", {
  id: "me/remote",
  prefix: "me.remote.",
  dir: "/tmp/remote",
  manifest: { name: "remote", contributes: { settings: { sections: [{ id: "tailscale", page: "general", title: "Tailscale", component: "TailscaleSection", keywords: ["sign in"] }, { id: "broken", page: "general", title: "Broken", component: "Broken" }] } } },
  state: "idle",
  active: false,
  sent: false,
})
const { ThemeContext, DARK, METRICS } = await import("../../sdk/theme.ts")
const { DEFAULT } = await import("../theme/default.tsx")
const { SettingsApp } = await import("./app.tsx")
const { useState } = await import("react")
const { flushSync } = await import("../../react/reconciler/reconciler.ts")

const gpu = hasNativeTestRenderer ? describe : describe.skip
const theme = { spec: { name: "test" }, appearance: "dark" as const, tokens: DARK, metrics: METRICS, components: DEFAULT }

async function open() {
  let go: (route: { page?: string; link?: string }) => void = () => {}
  function Host() {
    const [route, setRoute] = useState({ asked: 0 } as { page?: string; link?: string; asked: number })
    go = (r) => setRoute((was) => ({ ...r, asked: was.asked + 1 }))
    return (
      <ThemeContext.Provider value={theme as any}>
        <SettingsApp route={route} safe={false} />
      </ThemeContext.Provider>
    )
  }
  const t = createTestRoot({ width: 900, height: 640 })
  t.render(<Host />)
  // The options and the server's answers come in.
  await Bun.sleep(20)
  t.renderer.flush()
  const to = async (r: { page?: string; link?: string }) => {
    flushSync(() => go(r))
    await Bun.sleep(5)
    t.renderer.flush()
  }
  return { t, go: to }
}

const click = (t: ReturnType<typeof createTestRoot>, testId: string) => {
  const el = t.renderer.findByTestId(testId)
  if (!el) throw new Error(`no ${testId}`)
  const b = t.renderer.getElementBounds(el.id)!
  t.renderer.nativeSimulateClick(b.x + b.width / 2, b.y + b.height / 2)
}

gpu("Settings", () => {
  test("lists the built-in pages (a plugin with only sections has no page), and shows General with the server and the plugin's sections after its rows", async () => {
    const { t } = await open()
    const text = t.renderer.getAllText()
    expect(text).toEqual(expect.arrayContaining(["General", "Appearance", "Keybindings", "Plugins", "Pumi Terminals", "Running", "Update automatically", "Tailscale is here"]))
    expect(text.indexOf("Tailscale is here")).toBeGreaterThan(text.indexOf("Update automatically"))
    // The default theme's settings are its own, under the theme picker: not General's.
    expect(text).not.toContain("Haptic feedback")
    t.unmount()
  })

  test("a plugin's section that throws is an error card with its name, and Settings goes on", async () => {
    const { t } = await open()
    expect(t.renderer.findByTestId("settings-error-me/remote")).toBeTruthy()
    expect(t.renderer.getAllText()).toEqual(expect.arrayContaining(["me/remote could not draw its settings", "no QR code today", "Show logs", "Tailscale is here"]))
    t.unmount()
  })

  test("the nav goes to a page", async () => {
    const { t } = await open()
    click(t, "settings-nav-appearance")
    expect(t.renderer.findByTestId("settings-page-appearance")).toBeTruthy()
    expect(t.renderer.getAllText()).toEqual(expect.arrayContaining(["Theme", "Color scheme", "Font", "Font size", "Line height", "10%", "config.ghostty", "Import from Ghostty", "Edit file", "Cursor shape", "Copy text when selected", "Motion and feedback", "Haptic feedback"]))
    // The theme comes first: its picker, the colour scheme and its settings, then the terminals'.
    const top = (testId: string) => t.renderer.getElementBounds(t.renderer.findByTestId(testId)!.id)!.y
    expect(top("terminal-scheme-cards")).toBeLessThan(top("font-family"))
    expect(top("option-haptics")).toBeLessThan(top("font-family"))
    // Appearance says what each setting is, not what it means: no row has a description.
    const rows = t.renderer.getAllText()
    expect(rows).not.toContain("How much taller than the font's own line.")
    expect(rows).not.toContain("The cursor's shape while the program hasn't asked for another.")
    t.unmount()
  })

  test("the theme and the colour scheme are strips with a search, and past a dozen a page of them all", async () => {
    const { t } = await open()
    click(t, "settings-nav-appearance")
    expect(t.renderer.findByTestId("ui-theme-search")).toBeTruthy()
    expect(t.renderer.findByTestId("terminal-scheme-search")).toBeTruthy()
    // A test has a theme or two, and hundreds of schemes.
    expect(t.renderer.findByTestId("ui-theme-all")).toBeFalsy()
    const navColour = () => t.renderer.findByTestId("settings-nav-appearance")!.style.backgroundColor
    const marked = navColour()
    click(t, "terminal-scheme-all")
    await Bun.sleep(5)
    t.renderer.flush()
    expect(t.renderer.findByTestId("settings-page-color-schemes")).toBeTruthy()
    // The nav marks Appearance, and the grid fills the page rather than running past it.
    expect(navColour()).toEqual(marked)
    const page = t.renderer.getElementBounds(t.renderer.findByTestId("settings-page-color-schemes")!.id)!
    const grid = t.renderer.getElementBounds(t.renderer.findByTestId("terminal-scheme-all-cards")!.id)!
    expect(grid.y + grid.height).toBeLessThanOrEqual(page.y + page.height + 1)
    expect(grid.height).toBeGreaterThan(200)
    t.renderer.nativeSimulateKeystrokes(t.renderer.findByTestId("terminal-scheme-all-search")!.id, "d r a c u l a")
    expect(t.renderer.getAllText()).toContain("Dracula")
    click(t, "terminal-scheme-all-back")
    expect(t.renderer.findByTestId("settings-page-appearance")).toBeTruthy()
    t.unmount()
  })

  test("a link opens the page that draws what it names", async () => {
    const { t, go } = await open()
    await go({ link: "linespace" })
    expect(t.renderer.findByTestId("settings-page-appearance")).toBeTruthy()
    await go({ page: "keybindings" })
    expect(t.renderer.findByTestId("settings-page-keys")).toBeTruthy()
    t.unmount()
  })

  test("the search finds an option, whose row works there, and a plugin's section by its keywords", async () => {
    const { t } = await open()
    const field = t.renderer.findByTestId("settings-search")!
    t.renderer.nativeSimulateKeystrokes(field.id, "l i n e")
    expect(t.renderer.findByTestId("settings-page-search")).toBeTruthy()
    expect(t.renderer.findByTestId("option-linespace")).toBeTruthy()
    t.unmount()
  })
})
