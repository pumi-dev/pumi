// The windows: each is a renderer and one React root, showing a session. Which windows there are
// and what each shows is the app's own (kept in `windows.json`, beside the server's state), so
// two windows can show two sessions; the window in front moves the server's current session.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { Profiler, useSyncExternalStore, type ReactNode } from "react"
import { createRoot, type Root } from "../react/reconciler/reconciler.js"
import { Engine, type GpuixRenderer } from "../native/addon.js"
import { createNativeRenderer, startFrameLoop } from "../native/runtime.ts"
import { HeldModifiers, WindowContext, type WindowInfo } from "./state.ts"
import * as commands from "./commands.ts"
import * as menus from "./menus.ts"
import { mark } from "./trace.ts"
import { call, on, store, via } from "./engine.ts"

const FILE = join(Engine.stdpath("state") ?? ".", "windows.json")

interface Saved {
  session?: string
  width?: number
  height?: number
}

/** Where the window's traffic lights are (macOS draws three 12 px buttons 8 px apart). */
const TRAFFIC_LIGHTS = { x: 14, y: 12, width: 52, height: 12 }

/** macOS draws its title bar under the traffic lights; every other OS keeps its own. */
const MACOS = process.platform === "darwin"

export class AppWindow {
  readonly renderer: GpuixRenderer
  root!: Root
  session?: string
  active = false
  fullscreened = false
  /** The modifiers held down in it (holding ⌘ shows the keys). */
  readonly held = new HeldModifiers()
  private listeners = new Set<() => void>()

  constructor(
    readonly manager: WindowManager,
    readonly id: number,
    saved: Saved,
    content: (window: AppWindow) => ReactNode,
  ) {
    this.session = saved.session
    this.renderer = createNativeRenderer({
      onError: (err) => console.error("pumi: window", err),
      onWindow: (event) => {
        if (event.eventType === "windowClosed") return manager.closed(this)
        if (event.eventType === "windowActivated") {
          this.active = event.hovered === true
          if (this.active) manager.front(this)
          // Their release goes to the window in front now (⌘-Tab away): none are held here.
          else this.held.clear()
          manager.activated()
          this.changed()
        }
        if (event.eventType === "windowFullscreen") {
          this.fullscreened = event.hovered === true
          this.changed()
        }
      },
    })
    mark("w:renderer")
    this.renderer.init({
      title: "pumi",
      appName: "pumi",
      width: saved.width ?? 1100,
      height: saved.height ?? 720,
      titlebarTransparent: MACOS,
      ...(MACOS ? { trafficLightX: TRAFFIC_LIGHTS.x, trafficLightY: TRAFFIC_LIGHTS.y } : {}),
      minWidth: 480,
      minHeight: 320,
      appId: "dev.pumi.app",
    })
    mark("w:init")
    this.root = createRoot(this.renderer, {
      onKeyDown: (event) => commands.key(event, this.context()),
      onModifiersChanged: (modifiers) => this.held.set(modifiers),
      onUncaughtError: (error) => console.error("pumi: the window threw", error),
    } as any)
    mark("w:root")
    this.render(content)
    mark("w:render")
  }

  render(content: (window: AppWindow) => ReactNode) {
    this.root.render(<Provide window={this}>{content(this)}</Provide>)
  }

  /** What a command run in this window acts on. */
  context(): commands.Context {
    const snapshot = store.get().snapshot
    const session = snapshot?.workspaces.find((w) => w.workspace_id === this.session)
    const view = session?.active_view_id ?? undefined
    const tab = snapshot?.layouts.find((l) => l.view_id === view)?.focused_tab_id ?? undefined
    return { window: this, session: this.session, view, tab }
  }

  show(session: string) {
    this.session = session
    this.changed()
    this.manager.save()
    if (this.active) call(via(session, "session.focus"), session).catch(() => {})
  }

  close() {
    this.renderer.close()
    this.manager.closed(this)
  }

  subscribe(listener: () => void) {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  changed() {
    for (const listener of this.listeners) listener()
  }

  private shown?: { key: string; info: WindowInfo }

  /** What the window's tree reads of it (`useWindow`), the same object until it changes. */
  info(): WindowInfo {
    const key = `${this.session}|${this.active}|${this.fullscreened}`
    if (this.shown?.key === key) return this.shown.info
    const info: WindowInfo = {
      id: this.id,
      session: this.session,
      active: this.active,
      show: (s) => this.show(s),
      close: () => this.close(),
      minimize: () => this.renderer.minimizeWindow(),
      zoom: () => this.renderer.zoomWindow(),
      fullscreen: () => this.renderer.toggleFullscreen(),
      fullscreened: this.fullscreened,
      trafficLights: MACOS && !this.fullscreened ? TRAFFIC_LIGHTS : undefined,
      setBackground: (mode) => this.renderer.setWindowBackground(mode),
      setBlur: (radius) => this.renderer.setWindowBlur(radius),
      setGlass: (opacity) => this.renderer.setWindowGlass(opacity),
      held: this.held,
    }
    this.shown = { key, info }
    return info
  }
}

function Provide({ window, children }: { window: AppWindow; children: ReactNode }) {
  const info = useSyncExternalStore(
    (l) => window.subscribe(l),
    () => window.info(),
  )
  const tree = <WindowContext.Provider value={info}>{children}</WindowContext.Provider>
  return renders ? (
    <Profiler id="window" onRender={rendered}>
      {tree}
    </Profiler>
  ) : (
    tree
  )
}

/** `PUMI_TRACE`: how many commits the windows made and how long React spent on them. */
const renders = process.env.PUMI_TRACE ? { commits: 0, ms: 0, since: performance.now() } : undefined
function rendered(_id: string, _phase: string, ms: number) {
  if (!renders) return
  renders.commits++
  renders.ms += ms
  if (performance.now() - renders.since > 10_000) {
    console.error(`pumi-trace react: ${renders.commits} commits in 10 s, ${(renders.ms / renders.commits).toFixed(2)} ms each, ${renders.ms.toFixed(0)} ms in all`)
    Object.assign(renders, { commits: 0, ms: 0, since: performance.now() })
  }
}

export class WindowManager {
  readonly windows: AppWindow[] = []
  private next = 1
  private loop?: { stop(): void }

  /** Whether the server was last told one of the app's windows has the focus. */
  private reported?: boolean
  private reporting?: ReturnType<typeof setTimeout>

  constructor(private content: (window: AppWindow) => ReactNode) {
    on("app", (data: { reopen?: boolean }) => {
      if (data?.reopen) this.reopen()
    })
    menus.watch(this)
    // A new connection is a new client, which the server knows nothing of yet.
    let connected = false
    store.subscribe(() => {
      const now = !!store.get().snapshot
      if (now && !connected) {
        this.reported = undefined
        this.activated()
      }
      connected = now
    })
  }

  /**
   * Tells the server whether one of the app's windows has the focus (`client_active`, which fires
   * `app.changed`), once it settles: moving between two windows blurs one before the other is in front.
   */
  activated() {
    clearTimeout(this.reporting)
    this.reporting = setTimeout(() => {
      const active = this.windows.some((w) => w.active)
      if (active === this.reported) return
      this.reported = active
      call("core.setOptionValue", "client_active", active, { scope: "client" }).catch(() => {
        if (this.reported === active) this.reported = undefined
      })
    }, 50)
  }

  /** A menu item was clicked: run its command in the window in front. */
  menu(command: string) {
    const window = this.windows.find((w) => w.active) ?? this.windows[0]
    if (window) commands.run(command, window.context())
  }

  /** Opens the windows the app had, or one. */
  restore() {
    let saved: Saved[] = []
    try {
      if (existsSync(FILE)) saved = JSON.parse(readFileSync(FILE, "utf8"))
    } catch {}
    if (!saved.length) saved = [{}]
    for (const s of saved) this.add(s)
  }

  open(session?: string) {
    this.add({ session })
    this.save()
  }

  private add(saved: Saved) {
    const window = new AppWindow(this, this.next++, saved, this.content)
    this.windows.push(window)
    this.lastHides()
    mark("w:new")
    if (!this.loop) this.loop = startFrameLoop(window.renderer, { onTerminated: () => process.exit(0) })
    mark("w:loop")
    // A window that shows nothing yet shows the session the server has in front.
    if (!window.session) {
      const pick = () => {
        const s = store.get().snapshot
        if (!s || window.session) return
        const id = s.focused_workspace_id ?? s.workspaces[0]?.workspace_id
        if (id) window.show(id)
      }
      pick()
      const stop = store.subscribe(() => (window.session ? stop() : pick()))
    }
    return window
  }

  /** The window in front moved: the server's current session follows it. */
  front(window: AppWindow) {
    if (window.session) call(via(window.session, "session.focus"), window.session).catch(() => {})
  }

  closed(window: AppWindow) {
    const at = this.windows.indexOf(window)
    if (at < 0) return
    // Without a Dock icon to bring a hidden app back, closing the last window quits the app,
    // which opens it again next time; the server keeps the terminals running.
    if (!MACOS && this.windows.length === 1) return this.quit()
    this.windows.splice(at, 1)
    window.root.unmount()
    this.lastHides()
    this.save()
  }

  /** Closing the last window hides the app on macOS (its tree stays, and comes back at once from
   *  the Dock); elsewhere it closes, and the app quits. */
  private lastHides() {
    for (const window of this.windows) window.renderer.setHideOnClose(MACOS && this.windows.length === 1)
  }

  /** The app opened again (its Dock icon): a window, if it has none. */
  reopen() {
    if (this.windows.length === 0) this.open()
    else this.windows[0]!.renderer.activateWindow()
  }

  /** A session the server closed leaves the windows that showed it. */
  follow() {
    const sessions = store.get().snapshot?.workspaces ?? []
    const listed = (id: string) => sessions.some((s) => s.workspace_id === id)
    for (const window of this.windows) {
      if (window.session && !listed(window.session)) {
        window.session = sessions[0]?.workspace_id
        window.changed()
      }
    }
  }

  save() {
    const saved: Saved[] = this.windows.map((w) => ({ session: w.session }))
    try {
      mkdirSync(dirname(FILE), { recursive: true })
      writeFileSync(FILE, JSON.stringify(saved))
    } catch {}
  }

  render() {
    for (const window of this.windows) window.render(this.content)
  }

  quit() {
    this.save()
    process.exit(0)
  }
}
