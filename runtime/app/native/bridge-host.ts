// The main thread, when the app runs in its worker (`app/main.ts`): it holds the addon (the
// engine, each window's renderer), pumps AppKit, and does what the worker asks
// (`bridge-protocol.ts`). Nothing the app's JS does (a theme's loop, a plugin drawing a hundred
// thousand rows) runs here, so the windows keep drawing, the terminals keep printing and taking
// keys, and a window can be moved and resized, whatever the worker is doing.
//
// A watchdog hears the worker beat. One that stops answering for `STALL_MS` is ended and started
// again: the new worker draws into the same windows (the old drawing stays up until its first),
// is given the server's whole state at once, and leaves out what the old one was drawing when it
// froze (a plugin's window side, or the theme). After `SAFE_AFTER` freezes in a minute it starts
// in safe mode. A worker that crashes is started again the same way; one that exits means it.
import { fileURLToPath } from "node:url"
import { Engine, GpuixRenderer, setCapturedKeys } from "./addon.js"
import {
  BLAME,
  CONTROL_WORDS,
  DATA_BYTES,
  EXIT,
  HEARTBEAT,
  SYNC_CHUNK,
  SYNC_OFFSET,
  SYNC_STATE,
  SYNC_TOTAL,
  type Init,
  type Restart,
} from "./bridge-protocol.ts"
import { startFrameLoop } from "./runtime.ts"
import { COMPILED, embedded } from "./platform.ts"

/** Not answering for this long, once its windows are up, is frozen. */
const STALL_MS = 2500
/** While it starts (imports, the first render). */
const STARTING_STALL_MS = 10_000
/** This many freezes within `SAFE_WINDOW_MS` start it in safe mode. */
const SAFE_AFTER = 3
const SAFE_WINDOW_MS = 60_000
/** Each worker's element ids start this far past the last's. */
const ID_SPAN = 2 ** 32

// In the compiled app (`bun run app`) app/ui-worker.ts is an entry point of the executable,
// embedded in it (`/$bunfs/root/app/ui-worker.js`, `B:\~BUN\root\app\ui-worker.js`).
const WORKER = COMPILED ? embedded("app", "ui-worker.js") : fileURLToPath(new URL("../app/ui-worker.ts", import.meta.url))

/** The methods a class has, as the worker's proxies are made with them. */
function methodsOf(prototype: Record<string, unknown>): string[] {
  return Object.getOwnPropertyNames(prototype).filter((name) => name !== "constructor" && typeof prototype[name] === "function")
}

/** A window's renderer, and which worker's (`rid`) it is now. */
interface Window {
  real: GpuixRenderer
  rid: number
  closed: boolean
  /** The root the last batch set, and, for a window a new worker took over, the old drawing's. */
  root?: number
  replaces?: number
}

/** The server's state, merged as `app/engine.ts` merges it. */
interface FullState {
  snapshot: { layouts?: { view_id: string }[] }
  trees: Record<string, unknown>
  options?: unknown
  bindings?: unknown
  menus?: unknown
}

export interface HostOptions {
  /** For tests: what the worker runs instead of the app, and the addon. */
  entry?: string
  addon?: { Engine: typeof Engine; GpuixRenderer: typeof GpuixRenderer; setCapturedKeys: typeof setCapturedKeys }
  /** For tests: what a frozen worker is (default `STALL_MS`), and whether to pump AppKit. */
  stallMs?: number
  frameLoop?: boolean
  log?: (line: string) => void
  /** For tests: what the worker says that is not the bridge's. */
  onMessage?: (message: any) => void
}

export function host(options: HostOptions = {}) {
  const addon = options.addon ?? { Engine, GpuixRenderer, setCapturedKeys }
  const log = options.log ?? ((line: string) => console.error(line))
  const stallMs = options.stallMs ?? STALL_MS
  const control = new SharedArrayBuffer(CONTROL_WORDS * 4)
  const words = new Int32Array(control)
  const data = new SharedArrayBuffer(DATA_BYTES)
  const bytes = new Uint8Array(data)
  const encoder = new TextEncoder()
  const methods = { engine: methodsOf(addon.Engine.prototype as never), renderer: methodsOf(addon.GpuixRenderer.prototype as never) }

  let worker: Worker | undefined
  let closed = true
  let generation = 0
  let ready = false
  let lastBeat = -1
  let lastBeatAt = performance.now()
  const freezes: number[] = []
  let restart: Restart | undefined
  /** What each worker said its blame indices are. */
  let blameNames = new Map<number, string>()

  // --- The engine: one for the process, its events to whichever worker is current.
  let engine: Engine | undefined
  /** The worker asked for the engine's events (`Engine.start`); before that they wait. */
  let listening = false
  let waiting: unknown[] = []
  /** The server's state as a worker would have merged it: what a new worker is given at once. */
  let full: FullState | undefined

  function engineEvent(event: any) {
    if (event?.kind === "state" && event.data && !event.data.error && event.data.snapshot) {
      const d = event.data
      const trees = { ...(full?.trees ?? {}), ...(d.trees ?? {}) }
      for (const view of Object.keys(trees)) if (!d.snapshot.layouts?.some((l: any) => l.view_id === view)) delete trees[view]
      full = { snapshot: d.snapshot, trees, options: d.options ?? full?.options, bindings: d.bindings ?? full?.bindings, menus: d.menus ?? full?.menus }
    }
    if (listening) post({ t: "engine.event", event })
    else waiting.push(event)
  }

  // --- The windows.
  const windows = new Map<number, Window>()
  /** The last worker's windows, for the next one to take over in the order it opens its own. */
  let orphans: Window[] = []
  let loop: { stop(): void } | undefined

  function renderer(rid: number): Window {
    const adopted = orphans.shift()
    if (adopted) {
      adopted.rid = rid
      adopted.replaces = adopted.root
      windows.set(rid, adopted)
      return adopted
    }
    const window: Window = { rid, closed: false } as Window
    window.real = new addon.GpuixRenderer((error: Error | null, event: any) => {
      if (event?.eventType === "windowClosed") window.closed = true
      post({ t: "renderer.event", rid: window.rid, error: error ? String(error.message ?? error) : undefined, event })
    })
    windows.set(rid, window)
    return window
  }

  function applyBatch(window: Window, json: string) {
    const destroyed = window.real.applyBatch(json)
    if (json.includes('"setRoot"')) {
      const roots = [...json.matchAll(/\["setRoot",(\d+)\]/g)]
      if (roots.length) window.root = Number(roots[roots.length - 1]![1])
      // The new worker's first drawing is up: the old one goes.
      if (window.replaces !== undefined && window.replaces !== window.root) {
        window.real.applyBatch(JSON.stringify([["destroyElement", window.replaces]]))
        window.replaces = undefined
      }
    }
    if (destroyed.length) post({ t: "destroyed", rid: window.rid, ids: destroyed })
  }

  function call(target: any, method: string, args: unknown[]) {
    // A buffer crosses as a plain Uint8Array; the addon takes Buffers.
    const given = args.map((a) => (a instanceof Uint8Array && !Buffer.isBuffer(a) ? Buffer.from(a.buffer, a.byteOffset, a.byteLength) : a))
    const fn = target?.[method]
    if (typeof fn !== "function") throw new Error(`pumi: no ${method}`)
    return fn.apply(target, given)
  }

  function targetOf(message: any): any {
    if (message.target === "Engine") return addon.Engine
    if (message.target === "engine") return engine
    return windows.get(message.rid)?.real
  }

  // --- Synchronous answers, through shared memory.
  let pending: { bytes: Uint8Array; sent: number; failed: boolean } | undefined

  function answer(value: unknown, failed: boolean) {
    const text = failed ? String(value) : value === undefined ? "" : JSON.stringify(value)
    pending = { bytes: encoder.encode(text), sent: 0, failed }
    sendChunk()
  }

  function sendChunk() {
    if (!pending) return
    const chunk = Math.min(DATA_BYTES, pending.bytes.length - pending.sent)
    bytes.set(pending.bytes.subarray(pending.sent, pending.sent + chunk))
    Atomics.store(words, SYNC_TOTAL, pending.bytes.length)
    Atomics.store(words, SYNC_CHUNK, chunk)
    Atomics.store(words, SYNC_OFFSET, pending.sent)
    pending.sent += chunk
    const state = pending.failed ? 2 : 1
    if (pending.sent >= pending.bytes.length) pending = undefined
    Atomics.store(words, SYNC_STATE, state)
    Atomics.notify(words, SYNC_STATE)
  }

  function reply(id: number, promise: Promise<unknown> | unknown) {
    Promise.resolve(promise).then(
      (value) => post({ t: "reply", id, ok: true, value }),
      (error) => post({ t: "reply", id, ok: false, error: String(error?.message ?? error) }),
    )
  }

  function post(message: unknown) {
    if (!closed) worker!.postMessage(message)
  }

  const debug = !!process.env.PUMI_BRIDGE_DEBUG
  function receive(message: any) {
    if (debug && message.t !== "renderer") log(`bridge: ${message.t} ${message.method ?? ""}`)
    switch (message.t) {
      case "engine.start": {
        engine ??= addon.Engine.start(message.options, engineEvent)
        // A worker after the first: the server's state at once, then what came meanwhile.
        if (generation > 1 && full) post({ t: "engine.event", event: { kind: "state", data: full } })
        listening = true
        for (const event of waiting.splice(0)) post({ t: "engine.event", event })
        if (generation > 1) engine.refresh()
        return
      }
      case "engine": {
        if (!engine) return
        const result = call(engine, message.method, message.args)
        if (message.id !== undefined) reply(message.id, result)
        return
      }
      case "renderer.new":
        renderer(message.rid)
        return
      case "renderer": {
        const window = windows.get(message.rid)
        if (!window) return
        if (message.method === "applyBatch") return applyBatch(window, message.args[0])
        // A window taken over is open already.
        if (message.method === "init" && window.real.isInitialized()) return
        if (message.method === "close") window.closed = true
        const result = call(window.real, message.method, message.args)
        if (message.method === "init" && !loop && options.frameLoop !== false) {
          loop = startFrameLoop(window.real, { onTerminated: () => process.exit(0) })
        }
        if (message.id !== undefined) reply(message.id, result)
        return
      }
      case "sync": {
        let value: unknown
        try {
          value = call(targetOf(message), message.method, message.args ?? [])
        } catch (error: any) {
          return answer(error?.message ?? error, true)
        }
        return answer(value, false)
      }
      case "sync-more":
        return sendChunk()
      case "keys":
        return call(addon, "setCapturedKeys", message.args)
      case "blame":
        blameNames.set(message.index, message.name)
        return
      default:
        options.onMessage?.(message)
        return
      case "ready": {
        ready = true
        // The windows the last worker had and this one did not open again.
        for (const window of orphans.splice(0)) {
          window.closed = true
          window.real.close()
        }
        return
      }
    }
  }

  function start() {
    generation++
    for (const word of [HEARTBEAT, BLAME, SYNC_STATE, EXIT]) Atomics.store(words, word, 0)
    blameNames = new Map()
    listening = false
    ready = false
    lastBeat = -1
    lastBeatAt = performance.now()
    const next = new Worker(WORKER)
    worker = next
    closed = false
    next.onmessage = (e: MessageEvent) => {
      if (worker !== next) return
      try {
        receive(e.data)
      } catch (error: any) {
        log(`pumi: the main thread: ${e.data?.t} ${e.data?.method ?? ""}: ${error?.stack ?? error}`)
      }
    }
    next.addEventListener("error", (e: ErrorEvent) => log(`pumi: the app's worker: ${e.message}`))
    next.addEventListener("close", (e: any) => {
      if (worker !== next) return
      closed = true
      const meant = Atomics.load(words, EXIT)
      if (meant) process.exit(meant - 1)
      log(`pumi: the app's worker ended (code ${e.code}); starting it again`)
      again(undefined)
    })
    const init: Init = { t: "init", control, data, argv: process.argv, methods, idBase: (generation - 1) * ID_SPAN, restart, entry: options.entry }
    next.postMessage(init)
  }

  /** Ends the worker (if it still runs) and starts another in its place. */
  function again(blamed: string | undefined) {
    const now = performance.now()
    freezes.push(now)
    while (freezes.length && now - freezes[0]! > SAFE_WINDOW_MS) freezes.shift()
    const frozen = new Set(restart?.frozen ?? [])
    if (blamed && !blamed.startsWith("theme:")) frozen.add(blamed)
    restart = {
      count: (restart?.count ?? 0) + 1,
      blamed,
      frozen: [...frozen],
      themeOff: restart?.themeOff || !!blamed?.startsWith("theme:"),
      safe: freezes.length >= SAFE_AFTER,
    }
    const old = worker
    if (old && !closed) {
      worker = undefined
      closed = true
      // Terminating a worker that has already closed takes Bun down (1.3.14): it has not.
      old.terminate()
    }
    // Its windows stay up, drawn as they were, for the next worker to take over.
    orphans = [...windows.values()].filter((w) => !w.closed)
    windows.clear()
    pending = undefined
    start()
  }

  // --- The watchdog.
  const watchdog = setInterval(() => {
    if (closed || !worker) return
    const beat = Atomics.load(words, HEARTBEAT)
    const now = performance.now()
    if (beat !== lastBeat) {
      lastBeat = beat
      lastBeatAt = now
      return
    }
    const limit = ready ? stallMs : Math.max(stallMs, STARTING_STALL_MS)
    if (now - lastBeatAt < limit) return
    const index = Atomics.load(words, BLAME)
    const blamed = index ? blameNames.get(index) : undefined
    log(`pumi: the app's JS stopped answering for ${Math.round(now - lastBeatAt)} ms${blamed ? ` while drawing ${blamed}` : ""}; starting it again${blamed ? " without it" : ""}`)
    again(blamed)
  }, 250)

  start()
  return {
    /** For tests: the worker now, its restarts, and stopping it all. */
    get worker() {
      return worker
    },
    get restart() {
      return restart
    },
    stop() {
      clearInterval(watchdog)
      loop?.stop()
      if (worker && !closed) {
        closed = true
        worker.terminate()
      }
    },
  }
}
