import { describe, expect, test } from "bun:test"
import { BROWSER_OPEN, openLink } from "./links.ts"

describe("a terminal's link", () => {
  test("goes to the browser plugin while it is on", async () => {
    const calls: unknown[][] = []
    await openLink("https://a.dev", "%3", true, async (...args: unknown[]) => void calls.push(args))
    expect(calls).toEqual([[BROWSER_OPEN, { url: "https://a.dev", tab: "%3", link: true, shift: true }]])
  })

  test("goes to the system's browser while it is off", async () => {
    const calls: unknown[][] = []
    await openLink("https://a.dev", "%3", false, async (...args: unknown[]) => {
      calls.push(args)
      if (args[0] === BROWSER_OPEN) throw new Error("not_found")
    })
    expect(calls.map((c) => c[0])).toEqual([BROWSER_OPEN, "fs.open"])
    expect(calls[1]).toEqual(["fs.open", "https://a.dev"])
  })
})
