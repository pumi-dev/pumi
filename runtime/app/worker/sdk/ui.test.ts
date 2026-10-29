import { describe, expect, test } from "bun:test"
import { ui } from "./core.ts"
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
