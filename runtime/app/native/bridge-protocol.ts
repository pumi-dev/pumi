// What the main thread and the app's worker say to each other (`bridge-host.ts`, `bridge-client.ts`).
//
// The main thread holds the addon: the engine, each window's renderer and the frame loop that
// pumps AppKit. The app (React, the theme, every plugin's `ui.tsx`, the keymap) runs in a worker,
// where `native/addon.js` gives proxies of the same classes. A call that returns nothing is a
// message; one whose answer is used waits for it on shared memory (`Atomics.wait`), which only
// the worker ever does: the main thread never waits for the worker, so nothing the app's JS does
// can stop the windows drawing, the terminals printing or the keys reaching them.

/** The control words, an `Int32Array` over `Init.control`. */
export const HEARTBEAT = 0
/** The index (`blame` names) of what the worker is drawing now: a plugin's component, a theme's part. */
export const BLAME = 1
/** A synchronous call: 0 while asked, 1 answered, 2 failed. */
export const SYNC_STATE = 2
/** Its answer's length in bytes, and the bytes of it in `Init.data` now, from `SYNC_OFFSET`. */
export const SYNC_TOTAL = 3
export const SYNC_CHUNK = 4
export const SYNC_OFFSET = 5
/** The worker is exiting on purpose (`process.exit`): its code, plus one. */
export const EXIT = 6
export const CONTROL_WORDS = 8
export const DATA_BYTES = 1 << 20

/** How often the worker beats. */
export const HEARTBEAT_MS = 100

/** What the main thread starts a worker with. */
export interface Init {
  t: "init"
  control: SharedArrayBuffer
  data: SharedArrayBuffer
  /** The process's arguments, as the main thread has them (a worker's are its own). */
  argv: string[]
  /** Each class's methods, as the addon has them. */
  methods: { engine: string[]; renderer: string[] }
  /** Where this worker's element ids start: a worker started again keeps clear of the last's. */
  idBase: number
  /** Set when this worker replaces one that stopped answering. */
  restart?: Restart
  /** For tests: what the worker runs instead of the app. */
  entry?: string
}

export interface Restart {
  /** How many times the app's worker was started again. */
  count: number
  /** What it was drawing when it stopped answering, if anything was. */
  blamed?: string
  /** The plugins whose window side is left out: each one blamed for a freeze. */
  frozen: string[]
  /** The theme is left out (it was blamed): the default draws. */
  themeOff: boolean
  /** Too many freezes: safe mode (no plugins, the default theme). */
  safe: boolean
}

/** Methods whose answer is not used: sent, not waited for. */
export const RENDERER_VOID = new Set([
  "init",
  "close",
  "setWindowBackground",
  "setWindowBlur",
  "setWindowGlass",
  "setTooltipStyle",
  "resetDebugFrameOverlayStats",
  "activateWindow",
  "setHideOnClose",
  "minimizeWindow",
  "zoomWindow",
  "toggleFullscreen",
  "setWindowTitle",
  "focusElement",
  "focusNext",
  "focusPrevious",
  "focusNextWithin",
  "focusPreviousWithin",
  "setWindowKeyEvents",
  "blur",
  "setWindowSelectionChange",
  "clearSelection",
  "scrollTo",
  "scrollToItem",
  "scrollIntoView",
  "setImagePixels",
  "setImage",
  "registerFont",
  "captureScreenshot",
])
/** Methods that answer with a promise. */
export const RENDERER_ASYNC = new Set(["promptForPaths"])
/** The worker's own: the frame loop is the main thread's. */
export const RENDERER_LOCAL = new Set(["tick", "requiresTick", "setFrameWakeCallback", "isInitialized", "applyBatch"])

export const ENGINE_VOID = new Set([
  "openDiagnostics",
  "reportDiagnostics",
  "setMenus",
  "showAbout",
  "restartToUpdate",
  "setWebDevtools",
  "setAppearance",
  "checkForUpdates",
  "pluginUpdatesApplied",
  "haptic",
  "provide",
  "answer",
  "refresh",
  "setTerminalPalette",
])
export const ENGINE_ASYNC = new Set(["call", "serverStatus", "serverRestart", "fonts"])
