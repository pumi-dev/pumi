// The addon, as the app's worker has it: proxies of `Engine` and `GpuixRenderer` that reach the
// real ones on the main thread (`bridge-host.ts`). `native/addon.js` gives these in the worker.
// See `bridge-protocol.ts`.
import {
  BLAME,
  ENGINE_ASYNC,
  ENGINE_VOID,
  EXIT,
  HEARTBEAT,
  HEARTBEAT_MS,
  RENDERER_ASYNC,
  RENDERER_LOCAL,
  RENDERER_VOID,
  SYNC_CHUNK,
  SYNC_OFFSET,
  SYNC_STATE,
  SYNC_TOTAL,
  type Init,
} from "./bridge-protocol.ts"
import { setElementIdBase } from "./renderer-state.ts"

/** What `app/ui-worker.ts` leaves for this module: the main thread's init, and the channel. */
export interface Bridge {
  init: Init
  post(message: unknown): void
  /** Set here: what each message from the main thread goes to. */
  receive?: (message: any) => void
  /** Messages that came before `receive` was set. */
  queue: any[]
  /** Set here: the app's windows are open (the main thread closes the ones it kept, unused). */
  ready?: () => void
}

const bridge = (globalThis as any).__pumiBridge as Bridge
const { init } = bridge
const control = new Int32Array(init.control)
const data = new Uint8Array(init.data)
const decoder = new TextDecoder()
const post = (message: unknown) => bridge.post(message)

setElementIdBase(init.idBase)

// The main thread's watchdog: a worker that stops beating has stopped answering.
setInterval(() => Atomics.add(control, HEARTBEAT, 1), HEARTBEAT_MS)

// `process.exit` ends this worker, not the process: the main thread is told it was meant.
const exit = process.exit.bind(process)
process.exit = ((code?: number) => {
  Atomics.store(control, EXIT, (Number(code) || 0) + 1)
  return exit(code)
}) as typeof process.exit

/** Asks the main thread and waits for its answer (it never waits for this worker). */
function sync(message: Record<string, unknown>): any {
  Atomics.store(control, SYNC_STATE, 0)
  post({ t: "sync", ...message })
  let bytes: Uint8Array | undefined
  for (;;) {
    if (Atomics.wait(control, SYNC_STATE, 0, 20_000) === "timed-out") {
      throw new Error(`pumi: the main thread did not answer ${String(message.method)}`)
    }
    const total = Atomics.load(control, SYNC_TOTAL)
    const chunk = Atomics.load(control, SYNC_CHUNK)
    const offset = Atomics.load(control, SYNC_OFFSET)
    bytes ??= new Uint8Array(total)
    bytes.set(data.subarray(0, chunk), offset)
    if (offset + chunk >= total) break
    Atomics.store(control, SYNC_STATE, 0)
    post({ t: "sync-more" })
  }
  const failed = Atomics.load(control, SYNC_STATE) === 2
  const text = decoder.decode(bytes)
  if (failed) throw new Error(text)
  return text === "" ? undefined : JSON.parse(text)
}

let nextRequest = 0
const requests = new Map<number, { resolve(value: unknown): void; reject(error: Error): void }>()

/** Asks the main thread for something that answers later (a promise). */
function request(message: Record<string, unknown>): Promise<any> {
  const id = ++nextRequest
  return new Promise((resolve, reject) => {
    requests.set(id, { resolve, reject })
    post({ ...message, id })
  })
}

// --- Engine

let onEngineEvent: ((event: unknown) => void) | undefined

/** The engine's static answers, which do not change while the app runs: asked once each. */
const kept = new Map<string, unknown>()
function once(method: string, args: unknown[]): any {
  const key = JSON.stringify([method, args])
  if (!kept.has(key)) kept.set(key, sync({ target: "Engine", method, args }))
  return kept.get(key)
}

class EngineProxy {
  static start(options: unknown, onEvent: (event: unknown) => void): EngineProxy {
    onEngineEvent = onEvent
    post({ t: "engine.start", options })
    return engine
  }
  static systemAppearance(): "dark" | "light" {
    return once("systemAppearance", [])
  }
  static stdpath(kind: string): string | null {
    return once("stdpath", [kind])
  }
  static logPath(id: string): string {
    return once("logPath", [id])
  }
  static cli(): number {
    throw new Error("pumi: the command line runs on the main thread")
  }
}

for (const method of init.methods.engine) {
  ;(EngineProxy.prototype as any)[method] = ENGINE_VOID.has(method)
    ? (...args: unknown[]) => post({ t: "engine", method, args })
    : ENGINE_ASYNC.has(method)
      ? (...args: unknown[]) => request({ t: "engine", method, args })
      : (...args: unknown[]) => sync({ target: "engine", method, args })
}
const engine = new EngineProxy()

// --- Renderers

type EventCallback = (error: Error | null, event: any) => void
let nextRenderer = 0
const renderers = new Map<number, RendererProxy>()

class RendererProxy {
  readonly rid = ++nextRenderer
  private initialized = false
  /** `createMutationQueue`'s: the ids a batch destroyed, which the main thread says after it. */
  private destroyed?: (ids: readonly number[]) => void

  constructor(readonly callback?: EventCallback | null) {
    renderers.set(this.rid, this)
    post({ t: "renderer.new", rid: this.rid })
  }

  // The frame loop is the main thread's.
  requiresTick(): boolean {
    return false
  }
  tick(): boolean {
    return true
  }
  setFrameWakeCallback(): boolean {
    return false
  }
  isInitialized(): boolean {
    return this.initialized
  }

  /** Sent, not waited for: what it destroyed comes back to `setDestroyedListener`'s listener. */
  applyBatch(json: string): number[] {
    post({ t: "renderer", rid: this.rid, method: "applyBatch", args: [json] })
    return []
  }
  setDestroyedListener(listener: (ids: readonly number[]) => void) {
    this.destroyed = listener
  }

  /** The main thread's messages for this renderer. */
  receive(message: any) {
    if (message.t === "destroyed") this.destroyed?.(message.ids)
    else if (message.t === "renderer.event") this.callback?.(message.error ? new Error(message.error) : null, message.event)
  }

  markInitialized() {
    this.initialized = true
  }
}

for (const method of init.methods.renderer) {
  if (RENDERER_LOCAL.has(method) || method === "constructor") continue
  ;(RendererProxy.prototype as any)[method] = RENDERER_VOID.has(method)
    ? function (this: RendererProxy, ...args: unknown[]) {
        if (method === "init") this.markInitialized()
        post({ t: "renderer", rid: this.rid, method, args })
      }
    : RENDERER_ASYNC.has(method)
      ? function (this: RendererProxy, ...args: unknown[]) {
          return request({ t: "renderer", rid: this.rid, method, args })
        }
      : function (this: RendererProxy, ...args: unknown[]) {
          return sync({ target: "renderer", rid: this.rid, method, args })
        }
}

// --- What the main thread sends.

bridge.receive = (message: any) => {
  switch (message.t) {
    case "engine.event":
      return onEngineEvent?.(message.event)
    case "renderer.event":
    case "destroyed":
      return renderers.get(message.rid)?.receive(message)
    case "reply": {
      const pending = requests.get(message.id)
      if (!pending) return
      requests.delete(message.id)
      return message.ok ? pending.resolve(message.value) : pending.reject(new Error(message.error))
    }
  }
}
for (const message of bridge.queue.splice(0)) bridge.receive(message)
bridge.ready = () => post({ t: "ready" })

/** Notes what the worker is drawing now (`native/blame.ts`); the watchdog reads it on a freeze. */
export function setBlame(index: number): number {
  const previous = Atomics.load(control, BLAME)
  Atomics.store(control, BLAME, index)
  return previous
}

export const addon = {
  Engine: EngineProxy,
  GpuixRenderer: RendererProxy,
  TestGpuixRenderer: undefined,
  hasTestGpuixRenderer: () => false,
  setCapturedKeys: (...args: unknown[]) => post({ t: "keys", args }),
}
