// The app's worker and the main thread (`bridge-host.ts`, `bridge-client.ts`), with a fake addon:
// calls, answers, events, and a worker that freezes started again without what froze it.
import { afterEach, describe, expect, test } from "bun:test"
import { fileURLToPath } from "node:url"
import { host } from "./bridge-host.ts"

/** A fake of the addon's classes, recording what reaches them. */
function fakeAddon() {
  const seen = { engines: 0, renderers: 0, batches: [] as string[], calls: [] as string[], refreshes: 0 }
  let emit: (event: unknown) => void = () => {}
  class Engine {
    static start(_options: unknown, onEvent: (event: unknown) => void) {
      seen.engines++
      emit = onEvent
      return new Engine()
    }
    static stdpath(kind: string) {
      return `/fake/${kind}`
    }
    static systemAppearance() {
      return "dark"
    }
    static logPath(id: string) {
      return `/fake/${id}.log`
    }
    call(method: string, params: unknown) {
      seen.calls.push(method)
      return Promise.resolve({ method, params })
    }
    refresh() {
      seen.refreshes++
    }
  }
  class GpuixRenderer {
    private initialized = false
    constructor(readonly callback: (error: Error | null, event: unknown) => void) {
      seen.renderers++
    }
    init() {
      this.initialized = true
    }
    isInitialized() {
      return this.initialized
    }
    applyBatch(json: string) {
      seen.batches.push(json)
      return json.includes("destroyElement") ? [1] : []
    }
    getWindowSize() {
      return { width: 800, height: 600 }
    }
    getAutomationTree() {
      return "x".repeat(3_000_000)
    }
    getElementBounds(id: number) {
      if (id < 0) throw new Error("Invalid element id: -1")
      return null
    }
    close() {}
  }
  return { addon: { Engine, GpuixRenderer, setCapturedKeys() {} } as any, seen, emit: (event: unknown) => emit(event) }
}

let stop = () => {}
afterEach(() => stop())

/** Starts the host on the fixture; resolves with what the worker says, as it says it. */
function start(fake: ReturnType<typeof fakeAddon>, stallMs = 400) {
  const heard: { what: string; value: unknown }[] = []
  const waiters: (() => void)[] = []
  const logs: string[] = []
  const running = host({
    addon: fake.addon,
    entry: fileURLToPath(new URL("./fixtures/bridge-app.ts", import.meta.url)),
    frameLoop: false,
    stallMs,
    log: (line) => logs.push(line),
    onMessage: (m) => {
      if (m.t !== "test") return
      heard.push(m)
      for (const wake of waiters.splice(0)) wake()
    },
  })
  stop = () => running.stop()
  /** The `n`th time the worker said `what`. */
  async function said(what: string, n = 1, ms = 5000): Promise<unknown> {
    const until = performance.now() + ms
    for (;;) {
      const all = heard.filter((h) => h.what === what)
      if (all.length >= n) return all[n - 1]!.value
      if (performance.now() > until) throw new Error(`the worker never said ${what} (${n}); it said ${heard.map((h) => h.what).join(", ")}; ${logs.join(" | ")}`)
      await new Promise<void>((resolve) => {
        waiters.push(resolve)
        setTimeout(resolve, 50)
      })
    }
  }
  return { running, said, logs }
}

describe("the app's worker", () => {
  test("calls the main thread's addon: messages, synchronous answers, big ones, errors and promises", async () => {
    const fake = fakeAddon()
    const { said } = start(fake)
    expect(await said("size")).toEqual({ width: 800, height: 600 })
    expect(await said("initialized")).toBe(true)
    expect(await said("tree")).toBe(3_000_000)
    expect(await said("stdpath")).toBe("/fake/state")
    expect(await said("thrown")).toContain("Invalid element id")
    expect(await said("call")).toEqual({ method: "echo", params: [1, 2] })
    expect(fake.seen.batches[0]).toContain("setRoot")
    // The engine's events reach it.
    fake.emit({ kind: "notice", data: "hello" })
    expect(await said("event")).toEqual({ kind: "notice", data: "hello" })
  })

  test("one that freezes is started again, in the same window, without what it was drawing", async () => {
    const fake = fakeAddon()
    const { said, running, logs } = start(fake)
    expect(await said("restart")).toBe(null)
    await said("size")
    fake.emit({ kind: "state", data: { snapshot: { layouts: [{ view_id: "@1" }] }, trees: { "@1": { tab: "%1" } }, options: { a: 1 } } })
    await said("event")
    // It spins while drawing a plugin's component: the watchdog ends it.
    fake.emit({ kind: "spin", data: "me/bad" })
    const restart: any = await said("restart", 2, 8000)
    expect(restart).toMatchObject({ count: 1, blamed: "me/bad", frozen: ["me/bad"], themeOff: false, safe: false })
    expect(logs.join("\n")).toContain("while drawing me/bad")
    // The new worker took the window over (no second renderer), and the engine stayed.
    await said("size", 2)
    expect(fake.seen.renderers).toBe(1)
    expect(fake.seen.engines).toBe(1)
    expect(fake.seen.refreshes).toBe(1)
    // Its first drawing replaced the old one, which was destroyed.
    const last = fake.seen.batches.slice(-2)
    expect(last[0]).toContain(`"setRoot",${2 ** 32 + 1}`)
    expect(last[1]).toBe(JSON.stringify([["destroyElement", 1]]))
    // It was given the server's whole state at once.
    expect(await said("event", 2)).toEqual({ kind: "state", data: { snapshot: { layouts: [{ view_id: "@1" }] }, trees: { "@1": { tab: "%1" } }, options: { a: 1 }, bindings: undefined, menus: undefined } })
    expect(running.restart?.count).toBe(1)
  }, 20_000)
})
