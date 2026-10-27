import { afterAll, expect, test } from "bun:test"
import { cache } from "./model.ts"
import "./models.gen.ts"
import type { Tab } from "./models.gen.ts"
import type { Wire } from "./transport.ts"

// The cache fills a tab with `tab.get`, a tick later, as the wire would.
const tabs: Record<string, Record<string, unknown>> = { "%41": { block: "terminal", title: "make" } }
const wire = {
  call: () => Promise.resolve(null),
  on: () => Promise.resolve(-1),
  off: () => Promise.resolve(null),
} as unknown as Wire
// The cache is the process's: the other tests' wire comes back after these.
const had = { wire: cache.wire, fill: cache.fill }
afterAll(() => {
  cache.wire = had.wire
  cache.fill = had.fill
})
cache.connect(wire, (kind, id) => new Promise((resolve) => setTimeout(() => resolve(kind === "tab" ? (tabs[id] ?? null) : null), 5)))

test("a model's fields read once it is ready, and not before", async () => {
  const tab = cache.model<Tab>("tab", "%41")!
  expect(tab.block).toBeUndefined()
  expect(await tab.ready).toBe(tab)
  expect(tab.block).toBe("terminal")
  expect(tab.title).toBe("make")
})

test("a model of nothing is never ready", async () => {
  const tab = cache.model<Tab>("tab", "%42")!
  await expect(tab.ready).rejects.toThrow("no tab %42")
})
