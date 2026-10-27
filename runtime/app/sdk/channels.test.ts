import { describe, expect, mock, test } from "bun:test"

// The window's events go through the engine: a fake one that records `event.on`/`event.off` and
// hands `event`s back as the engine would.
const calls: unknown[][] = []
const handlers = new Map<string, (data: any) => void>()
let nextId = 40
// The engine re-exports the wire ids' helpers (`via`, `num`, …), which what is under test imports.
const ids = await import("../app/ids.ts")
mock.module("../app/engine.ts", () => ({
  ...ids,
  store: { get: () => ({}), subscribe: () => () => {} },
  call: (...args: unknown[]) => {
    calls.push(args)
    return Promise.resolve(args[0] === "event.on" ? ++nextId : null)
  },
  engine: { call: () => Promise.resolve(null) },
  on: (kind: string, fn: (data: any) => void) => {
    handlers.set(kind, fn)
    return () => {}
  },
}))
const channels = await import("./channels.ts")
// A copy of its own (the query): another test file may have loaded events.ts already, bound to
// another engine, and this one has to hear the fake's `event`s.
const fresh = "./events.ts?channels-test"
const { subscribeEvent } = (await import(fresh)) as typeof import("./events.ts")

describe("a plugin's channel", () => {
  test("a message reaches the plugin's listeners of its topic, and is kept as the last", () => {
    const heard: unknown[] = []
    const stop = channels.listen("pumi_dev/acp", "delta", (d) => heard.push(d))
    channels.listen("pumi_dev/other", "delta", () => heard.push("wrong plugin"))
    channels.deliver("pumi_dev/acp", "delta", "he")
    channels.deliver("pumi_dev/acp", "delta", "llo")
    channels.deliver("pumi_dev/acp", "other", "wrong topic")
    expect(heard).toEqual(["he", "llo"])
    expect(channels.latest("pumi_dev/acp", "delta")).toEqual({ data: "llo" })
    stop()
    channels.deliver("pumi_dev/acp", "delta", "!")
    expect(heard).toEqual(["he", "llo"])
  })

  test("a plugin that stops is forgotten", () => {
    channels.deliver("pumi_dev/gone", "state", { n: 1 })
    channels.forget("pumi_dev/gone")
    expect(channels.latest("pumi_dev/gone", "state")).toBeUndefined()
  })
})

describe("useEvent's subscriptions", () => {
  test("listeners of one event share one subscription, dropped with the last", async () => {
    calls.length = 0
    const heard: string[] = []
    const a = subscribeEvent("request.opened", undefined, (e) => heard.push(`a:${e.data.request}`))
    const b = subscribeEvent("request.opened", undefined, (e) => heard.push(`b:${e.data.request}`))
    expect(calls.filter((c) => c[0] === "event.on")).toHaveLength(1)
    const [, events] = calls[0] as [string, string]
    expect(events).toEqual("request.opened")
    await new Promise((r) => setTimeout(r, 0))
    handlers.get("event")!({ subscription: nextId, event: { event: "request.opened", data: { request: "+7" } } })
    handlers.get("event")!({ subscription: 999, event: { data: { request: "+8" } } })
    expect(heard).toEqual(["a:+7", "b:+7"])
    a()
    await Promise.resolve()
    expect(calls.some((c) => c[0] === "event.off")).toBe(false)
    b()
    await new Promise((r) => setTimeout(r, 0))
    expect(calls.filter((c) => c[0] === "event.off")).toEqual([["event.off", nextId]])
  })

  test("another match is another subscription", () => {
    calls.length = 0
    const stop = [
      subscribeEvent("request.closed", "+3", () => {}),
      subscribeEvent("request.closed", "+4", () => {}),
    ]
    expect(calls.filter((c) => c[0] === "event.on").map((c) => (c[2] as any).match)).toEqual(["+3", "+4"])
    stop.forEach((s) => s())
  })
})
