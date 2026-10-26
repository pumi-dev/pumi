import { describe, expect, test } from "bun:test"
import { agent, ui } from "./core.ts"
import { connection, deliver } from "./runtime.ts"

/** A fake app: answers each call with `answer`, and keeps what the plugin posted. */
function fakeApp(answer: (name: string, args: any[]) => unknown) {
  const posted: any[] = []
  connection.install(
    {
      post(m: any) {
        posted.push(m)
        if (m.t !== "call") return
        queueMicrotask(() => deliver.reply({ id: m.id, ok: answer(m.name, m.args) }))
      },
    },
    { pkg: "pumi_dev/acp", prefix: "pumi_dev.acp.", dir: "/nonexistent", manifest: { name: "acp", api: 1 } },
  )
  return posted
}

describe("ui.post", () => {
  test("a message for the plugin's own window side goes to the host as it is", () => {
    const posted = fakeApp(() => null)
    ui.post("delta", { text: "he" })
    expect(posted.at(-1)).toEqual({ t: "ui", topic: "delta", data: { text: "he" } })
  })
})

describe("agent.onSend in a plugin", () => {
  test("the handler's text is the plugin's answer to the turn", async () => {
    let sending: number | undefined
    const contributed: any[] = []
    fakeApp((name, args) => {
      if (name === "event.on") return (sending = 3)
      if (name === "agent.contribute") {
        contributed.push(args)
        return contributed.length === 1
      }
      return null
    })
    const errors: string[] = []
    const error = console.error
    console.error = (line: string) => errors.push(line)
    try {
      const off = agent.onSend(async (turn) => (turn.event === "start" ? "the guide" : null))
      await off.ready
      deliver.event(sending!, { event: "agent.sending", tab: "%2", data: { id: 7, tab: "%2", event: "start" } })
      await Bun.sleep(5)
      expect(contributed).toEqual([[7, "the guide"]])
      deliver.event(sending!, { event: "agent.sending", tab: "%2", data: { id: 8, tab: "%2", event: "prompt" } })
      await Bun.sleep(5)
      expect(contributed[1]).toEqual([8, null])
      expect(errors.some((e) => e.includes("after its deadline"))).toBe(true)
    } finally {
      console.error = error
    }
  })

  test("a handler that throws answers null and logs why", async () => {
    let sending: number | undefined
    const contributed: any[] = []
    fakeApp((name, args) => {
      if (name === "event.on") return (sending = 3)
      if (name === "agent.contribute") return (contributed.push(args), true)
      return null
    })
    const errors: string[] = []
    const error = console.error
    console.error = (line: string) => errors.push(line)
    try {
      const off = agent.onSend(() => {
        throw new Error("no map")
      })
      await off.ready
      deliver.event(sending!, { event: "agent.sending", tab: "%2", data: { id: 9, tab: "%2", event: "prompt" } })
      await Bun.sleep(5)
      expect(contributed).toEqual([[9, null]])
      expect(errors.some((e) => e.includes("no map"))).toBe(true)
    } finally {
      console.error = error
    }
  })
})

/** The events that carry a tab's change, which `agent.take` waits on. */
const TAB_CHANGED = "tab.changed"

describe("agent.take in a plugin", () => {
  test("waiting, it asks again when an item is queued for its tab", async () => {
    const refs = new Map<string, number>()
    let items: any[] = []
    fakeApp((name, args) => {
      if (name === "event.on") {
        const key = [args[0]].flat().join(",") + (args[1].match ? `:${args[1].match}` : "")
        refs.set(key, refs.size + 1)
        return refs.size
      }
      if (name === "agent.take") {
        expect(args).toEqual(["%4", { session: "s-1" }])
        return items.shift() ?? null
      }
      return null
    })
    const taken = agent.take("%4", { session: "s-1", wait: true })
    await Bun.sleep(5)
    items = [{ id: 1, tab: "%4", session: "s-1", text: "wake up", from: "cli", created: "2026-01-01T00:00:00Z" }]
    deliver.event(refs.get(TAB_CHANGED)!, { event: "tab.changed", tab: "%9", data: { tab: "%9", keys: ["traits.agent"], patch: {} } })
    await Bun.sleep(5)
    deliver.event(refs.get(TAB_CHANGED)!, { event: "tab.changed", tab: "%4", data: { tab: "%4", keys: ["traits.agent"], patch: {} } })
    expect((await taken)?.text).toBe("wake up")
  })

  test("waiting, it gives up when the session changes or the signal aborts", async () => {
    const refs = new Map<string, number>()
    fakeApp((name, args) => {
      if (name === "event.on") {
        refs.set([args[0]].flat().join(",") + (args[1].match ? `:${args[1].match}` : ""), refs.size + 1)
        return refs.size
      }
      return null
    })
    const taken = agent.take("%4", { session: "s-1", wait: true })
    await Bun.sleep(5)
    deliver.event(refs.get(TAB_CHANGED)!, { event: "tab.changed", tab: "%4", data: { tab: "%4", keys: ["options.@agent.session"], patch: { "options.@agent.session": "s-2" } } })
    expect(await taken).toBeNull()
    const stop = new AbortController()
    const stopped = agent.take("%4", { session: "s-1", wait: true, signal: stop.signal })
    await Bun.sleep(5)
    stop.abort()
    expect(await stopped).toBeNull()
  })
})
