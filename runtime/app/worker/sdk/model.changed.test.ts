// The cache's merge of `*.changed` events: a key into a field (`options.title`) is merged into
// it, and a model asked for again is held as it was until the answer comes.
import { afterAll, describe, expect, test } from "bun:test"
import { cache, type Fields } from "./model.ts"

/** A fill the test answers by hand, one ask at a time. */
function manualFill() {
  const asks: { kind: string; id: string; answer(fields: Fields | null): void }[] = []
  cache.connect({} as any, (kind, id) => new Promise((resolve) => asks.push({ kind, id, answer: resolve })))
  return asks
}

// The cache is the process's: the other tests' wire comes back after these.
const had = { wire: cache.wire, fill: cache.fill }
afterAll(() => {
  cache.wire = had.wire
  cache.fill = had.fill
})

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe("cache.changed", () => {
  test("a key into a field is merged into it, the rest of it kept", () => {
    manualFill()
    cache.prime("tab", "%901", { options: { "@agent": "claude", title: "a" } })
    cache.changed({ kind: "tab", id: "%901", keys: ["options.title"], patch: { options: { title: "b" } } })
    expect(cache.get("tab", "%901")?.options).toEqual({ "@agent": "claude", title: "b" })
  })

  test("a key into a field set to null takes it out", () => {
    manualFill()
    cache.prime("tab", "%902", { options: { "@agent": "claude", "@x": 1 } })
    cache.changed({ kind: "tab", id: "%902", keys: ["options.@x"], patch: { options: { "@x": null } } })
    expect(cache.get("tab", "%902")?.options).toEqual({ "@agent": "claude" })
  })

  test("a key the patch does not carry: the model is held as it was until it is read again", async () => {
    const asks = manualFill()
    cache.prime("tab", "%903", { options: { "@agent": "claude" }, title: "a" })
    cache.changed({ kind: "tab", id: "%903", keys: ["traits"], patch: {} })
    expect(cache.get("tab", "%903")?.options).toEqual({ "@agent": "claude" })
    expect(asks.length).toBe(1)
    asks[0]!.answer({ options: { "@agent": "codex" }, title: "b" })
    await flush()
    expect(cache.get("tab", "%903")).toMatchObject({ options: { "@agent": "codex" }, title: "b" })
  })

  test("of two asks again, the older answer is left for the newer one's", async () => {
    const asks = manualFill()
    cache.prime("tab", "%904", { title: "a" })
    cache.changed({ kind: "tab", id: "%904", keys: ["traits"], patch: {} })
    cache.changed({ kind: "tab", id: "%904", keys: ["traits"], patch: {} })
    asks[1]!.answer({ title: "new" })
    await flush()
    asks[0]!.answer({ title: "old" })
    await flush()
    expect(cache.get("tab", "%904")?.title).toBe("new")
  })
})
