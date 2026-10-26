// What the app's JS is drawing now, for the main thread's watchdog (`bridge-host.ts`): when the
// app's worker stops answering, what it was drawing is what is left out when it starts again. A
// plugin's component and a theme's parts are drawn through `blamed` (`sdk/contributions.ts`). Outside
// the worker (the app run on the main thread, tests) it costs a branch and does nothing.
import type { Bridge } from "./bridge-client.ts"

const bridge = (globalThis as any).__pumiBridge as Bridge | undefined
const client = bridge ? await import("./bridge-client.ts") : undefined

const indices = new Map<string, number>()

/** `name`'s number in the watchdog's word, given to the main thread the first time. */
function indexOf(name: string): number {
  let index = indices.get(name)
  if (index === undefined) {
    index = indices.size + 1
    indices.set(name, index)
    bridge!.post({ t: "blame", index, name })
  }
  return index
}

/** Runs `draw` with `name` noted as what is drawing (a plugin, `owner/name`; a theme, `theme:<name>`). */
export function drawing<T>(name: string, draw: () => T): T {
  if (!client) return draw()
  const previous = client.setBlame(indexOf(name))
  try {
    return draw()
  } finally {
    client.setBlame(previous)
  }
}
