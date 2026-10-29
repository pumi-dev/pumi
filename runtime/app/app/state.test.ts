import { describe, expect, mock, test } from "bun:test"

// `mock.module` is process-wide: the app's other tests hear this stub too, so it carries what
// they import from the engine (`call`, `on`) beside what `state.ts` needs.
mock.module("./engine.ts", () => ({
  store: { get: () => ({}), subscribe: () => () => {} },
  call: () => Promise.resolve(null),
  on: () => () => {},
}))
const { HeldModifiers } = await import("./state.ts")

describe("HeldModifiers", () => {
  const mods = (held: Partial<Record<"ctrl" | "alt" | "shift" | "cmd", boolean>>) => ({ ctrl: false, alt: false, shift: false, cmd: false, ...held })

  test("what is held, as a chord writes it before its key, and a word only when it changes", () => {
    const held = new HeldModifiers()
    let heard = 0
    held.subscribe(() => heard++)
    held.set(mods({ cmd: true }))
    expect(held.get()).toBe("cmd")
    held.set(mods({ cmd: true }))
    expect(heard).toBe(1)
    held.set(mods({ cmd: true, shift: true, ctrl: true }))
    expect(held.get()).toBe("ctrl+shift+cmd")
    held.clear()
    expect(held.get()).toBe("")
    expect(heard).toBe(3)
  })
})
