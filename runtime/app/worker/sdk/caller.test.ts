import { describe, expect, test } from "bun:test"
import { call, self, workspace } from "./core.ts"
import { connection, deliver } from "./runtime.ts"
import "./models.gen.ts"

/** A fake app: answers each call with `null`, and keeps what the plugin posted. */
function fakeApp() {
  const posted: any[] = []
  connection.install(
    {
      post(m: any) {
        posted.push(m)
        if (m.t === "call") queueMicrotask(() => deliver.reply({ id: m.id, ok: null }))
      },
    },
    { pkg: "me/office", prefix: "me.office.", dir: "/nonexistent", manifest: { name: "office", api: 1 } },
  )
  return posted
}

/** Serves `name` as the app would, for `caller`, and answers what the function answered. */
function serve(posted: any[], name: string, caller: unknown): Promise<any> {
  const id = posted.length + 1000
  deliver.serve({ id, name, args: [], caller })
  return (async () => {
    for (;;) {
      const reply = posted.find((m) => m.t === "reply" && m.id === id)
      if (reply) return reply.ok ?? reply.error
      await Bun.sleep(1)
    }
  })()
}

describe("a function the plugin serves", () => {
  test("knows its caller, and the calls it makes are about the caller's tab", async () => {
    const posted = fakeApp()
    connection.provided.set("me.office.sit", async () => {
      const before = self.caller
      await call("tab.focus")
      // Still the caller's after an await.
      return { before, after: self.caller, tab: self.tab, current: workspace.currentTab?.id ?? null }
    })
    const answer = await serve(posted, "me.office.sit", { chan: 4, name: "pumi-app", tab: 7, client: 4 })
    const caller = { client: 4, tab: "%7", source: "pumi-app" }
    expect(answer).toEqual({ before: caller, after: caller, tab: "%7", current: "%7" })
    expect(posted.find((m) => m.t === "call" && m.name === "tab.focus")).toMatchObject({ caller: { tab: 7, client: 4 } })
  })

  test("outside a call there is no caller, and calls carry none", async () => {
    const posted = fakeApp()
    expect(self.caller).toBeNull()
    await call("tab.focus")
    expect(posted.find((m) => m.t === "call")).not.toHaveProperty("caller")
  })

  test("a caller with no tab or client is only its source", async () => {
    const posted = fakeApp()
    connection.provided.set("me.office.who", () => self.caller)
    expect(await serve(posted, "me.office.who", { name: "me/office" })).toEqual({ client: null, tab: null, source: "me/office" })
  })
})
