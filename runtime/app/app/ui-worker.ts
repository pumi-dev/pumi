// The app's worker: the app itself (app.tsx: React, the windows' trees, the theme, every plugin's
// `ui.tsx`, the keymap, the plugins' supervisor), off the main thread. The main thread holds the
// addon and pumps AppKit (`native/bridge-host.ts`); here `native/addon.js` gives proxies of it
// (`native/bridge-client.ts`). See `native/bridge-protocol.ts`.
//
// No top-level await here: Bun hands a worker its messages once its entry module has evaluated,
// so one that awaited its first message would wait forever.
import type { Bridge } from "../native/bridge-client.ts"
import { EXIT, type Init } from "../native/bridge-protocol.ts"

let bridge: Bridge | undefined
const early: unknown[] = []

self.onmessage = (e: MessageEvent) => {
  if (!bridge && e.data?.t === "init") return void run(e.data as Init)
  if (bridge?.receive) bridge.receive(e.data)
  else (bridge?.queue ?? early).push(e.data)
}

async function run(init: Init) {
  bridge = { init, post: (message) => postMessage(message), queue: early }
  ;(globalThis as any).__pumiBridge = bridge
  // The process's arguments, not this worker's: `--data-dir`, `--safe`, and what a restart runs.
  // (The environment is left alone: changing it under Bun crashes the next worker.) What a
  // restart leaves out is read from the bridge (`app/mode.ts`).
  process.argv = init.argv
  try {
    // The app's import is written out, for the compiled app's bundler to see.
    if (init.entry) await import(init.entry)
    else await import("./app.tsx")
  } catch (error: any) {
    // An app that cannot start is not started again: the process ends with it.
    console.error("pumi: the app did not start:", error?.stack ?? error)
    Atomics.store(new Int32Array(init.control), EXIT, 2)
    process.exit(1)
  }
}
