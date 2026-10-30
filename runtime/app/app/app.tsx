// The app (`app/main.ts` runs it). `pumi COMMAND …` is the command line, as
// in every tab; with no command it opens the windows.
import { mark, report } from "./trace.ts"
// Before React's reconciler: it installs the hook React reports its commits to.
import * as diagnostics from "./diagnostics.ts"
mark("bun")
import { Engine } from "../native/addon.js"
mark("addon")

import { isCli } from "./cli.ts"

const argv = process.argv.slice(2)
if (isCli(argv)) process.exit(Engine.cli(argv))

const { safe } = await import("./mode.ts")
const at = argv.indexOf("--data-dir")
if (at >= 0 && argv[at + 1]) process.env.PUMI_DATA_DIR = argv[at + 1]

mark("args")
const { installRuntimeErrorHandlers } = await import("../react/reconciler/renderer.js")
installRuntimeErrorHandlers()
mark("react")
const { engine, store, on } = await import("./engine.ts")
mark("engine")
diagnostics.connect((report) => engine.reportDiagnostics(report))
on("diagnostics", (data: { watching?: boolean }) => diagnostics.setWatching(!!data?.watching))
const plugins = await import("./plugins.ts")
const { WindowManager } = await import("./windows.tsx")
const { Root } = await import("./root.tsx")
const { setContextResolver } = await import("../sdk/context.ts")
const opened = await import("./opened.ts")
const links = await import("./links.ts")
const { provideUpdates } = await import("./updates.ts")
const ghostty = await import("./ghostty.ts")
const { SETTINGS_FILE } = await import("./settings/store.ts")
const layout = await import("./layout.ts")
await import("./ui.ts")
mark("imports")

const manager = new WindowManager(() => <Root safe={safe} />)
// ⌘, and `openSettings`: the Settings window, React as the windows are.
;(await import("./settings/window.tsx")).installSettings(manager, safe)
setContextResolver((id) => manager.windows.find((w) => w.id === id)?.context())
manager.restore()
mark("restore")
// In the app's worker: its windows are open (the main thread closes any of the last worker's it
// did not take over).
;(globalThis as { __pumiBridge?: { ready?: () => void } }).__pumiBridge?.ready?.()

// Once the server's state is here: it may be already (the app's worker started again is given it
// at once, before this runs).
let begun = false
const begin = () => {
  if (begun || !store.get().snapshot) return
  begun = true
  stop()
  mark("state")
  setTimeout(report, 1500)
  // Files and folders dropped on the Dock icon, the app's start's included.
  opened.start(manager)
  // A terminal's cmd+clicked links: to the browser plugin, else the system's browser.
  links.start()
  plugins.start(safe)
  provideUpdates()
  // The default theme's settings this Mac kept before they were settings.
  layout.moveLocalPrefs()
  // The terminals' settings: config.ghostty, followed as it changes.
  ghostty.follow(
    (name, value) => engine.call("option.set", [name, value, { scope: "global", source: ghostty.GHOSTTY_SOURCE }]),
    () => store.get().options,
    SETTINGS_FILE,
  )
}
const stop = store.subscribe(begin)
begin()
// Web pages get their inspector as `webview_devtools` says.
let devtools: unknown
store.subscribe(() => {
  const on = store.get().options.webview_devtools ?? true
  if (on !== devtools) engine.setWebDevtools((devtools = on) !== false)
})
