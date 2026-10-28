// Settings: a window of its own (one per app, brought to the front when opened again), a
// renderer and a React root as the main windows are, drawn with the UI theme in use and the
// settings kit. ⌘, and `openSettings(page, link)` reach it (`settings/open.ts`).
import { type ReactNode } from "react"
import { useSyncExternalStore } from "react"
import { createRoot, type Root } from "../../react/reconciler/reconciler.js"
import type { GpuixRenderer } from "../../native/addon.js"
import { createNativeRenderer } from "../../native/runtime.ts"
import { ThemeContext } from "../../sdk/theme.ts"
import { chordOf } from "../keymap.ts"
import { AppLayers } from "../layers.tsx"
import { OwnPaletteContext, useResolvedTheme } from "../root.tsx"
import { WindowContext, type WindowInfo } from "../state.ts"
import type { WindowManager } from "../windows.tsx"
import { SettingsApp, type Route } from "./app.tsx"
import { setOpener } from "./open.ts"

const WIDTH = 900
const HEIGHT = 640
const TRAFFIC_LIGHTS = { x: 12, y: 12, width: 52, height: 12 }

export class SettingsWindow {
  readonly renderer: GpuixRenderer
  readonly root: Root
  active = false
  route: Route = { asked: 0 }
  private listeners = new Set<() => void>()
  private shown?: { key: string; info: WindowInfo }

  constructor(
    readonly manager: WindowManager,
    readonly safe: boolean,
    private onClosed: () => void,
  ) {
    this.renderer = createNativeRenderer({
      onError: (err) => console.error("pumi: settings", err),
      onWindow: (event) => {
        if (event.eventType === "windowClosed") return this.closed()
        if (event.eventType === "windowActivated") {
          this.active = event.hovered === true
          this.changed()
        }
      },
    })
    this.renderer.init({ title: "Settings", appName: "pumi", width: WIDTH, height: HEIGHT, titlebarTransparent: true, trafficLightX: TRAFFIC_LIGHTS.x, trafficLightY: TRAFFIC_LIGHTS.y, minWidth: 560, minHeight: 360, appId: "dev.pumi.app" })
    this.root = createRoot(this.renderer, {
      onKeyDown: (event: { key?: string | null; modifiers?: any }) => this.key(event),
      onUncaughtError: (error: unknown) => console.error("pumi: Settings threw", error),
    } as any)
    this.root.render(<SettingsTree window={this} />)
  }

  /** Goes to `page`, and to what `link` names on it, in front. */
  show(page?: string, link?: string) {
    this.route = { page, link, asked: this.route.asked + 1 }
    this.changed()
    this.renderer.activateWindow()
  }

  /** The keys the page left: ⌘W closes it, ⌘Q quits, ⌘M minimizes. */
  private key(event: { key?: string | null; modifiers?: any }) {
    const chord = chordOf(event)
    if (chord === "cmd+w") this.close()
    else if (chord === "cmd+m") this.renderer.minimizeWindow()
    else if (chord === "cmd+q") this.manager.quit()
  }

  close() {
    this.renderer.close()
    this.closed()
  }

  private done = false
  private closed() {
    if (this.done) return
    this.done = true
    this.root.unmount()
    this.onClosed()
  }

  subscribe(listener: () => void) {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  private changed() {
    for (const listener of this.listeners) listener()
  }

  /** What a plugin's component drawn here reads of its window (`useWindow`): this one, which shows no session. */
  info(): WindowInfo {
    const key = `${this.active}`
    if (this.shown?.key === key) return this.shown.info
    const info: WindowInfo = {
      id: 0,
      session: undefined,
      active: this.active,
      show: () => {},
      close: () => this.close(),
      minimize: () => this.renderer.minimizeWindow(),
      zoom: () => this.renderer.zoomWindow(),
      fullscreen: () => this.renderer.toggleFullscreen(),
      fullscreened: false,
      trafficLights: TRAFFIC_LIGHTS,
      setBackground: (mode) => this.renderer.setWindowBackground(mode),
      setBlur: (radius) => this.renderer.setWindowBlur(radius),
      setGlass: (opacity) => this.renderer.setWindowGlass(opacity),
    }
    this.shown = { key, info }
    return info
  }
}

function SettingsTree({ window }: { window: SettingsWindow }) {
  const info = useSyncExternalStore(
    (l) => window.subscribe(l),
    () => window.info(),
  )
  const route = useSyncExternalStore(
    (l) => window.subscribe(l),
    () => window.route,
  )
  const { state, own } = useResolvedTheme(window.safe)
  const tree: ReactNode = (
    <div style={{ width: "100%", height: "100%", position: "relative" }}>
      <SettingsApp route={route} safe={window.safe} />
      <AppLayers />
    </div>
  )
  return (
    <ThemeContext.Provider value={state}>
      <OwnPaletteContext.Provider value={own}>
        <WindowContext.Provider value={info}>{tree}</WindowContext.Provider>
      </OwnPaletteContext.Provider>
    </ThemeContext.Provider>
  )
}

let current: SettingsWindow | undefined

/** Makes `openSettings` open the window (one per app), as the app starts. */
export function installSettings(manager: WindowManager, safe: boolean) {
  setOpener((page, link) => {
    current ??= new SettingsWindow(manager, safe, () => (current = undefined))
    current.show(page, link)
  })
}
