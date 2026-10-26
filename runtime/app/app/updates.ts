// pumi's updates as functions every surface reaches (`pumi update check`, a plugin's
// `call("update.state")`), answered by this app, and an `update.changed` event each time the
// updater's state changes. Settings → General reads the same state (`Engine.updateState`).
import { call, engine, on } from "./engine.ts"

/** The functions this app answers itself, by name: what `request` events for them do. */
export const APP_FUNCTIONS = new Map<string, (args: unknown[]) => unknown>()

const UPDATES = [
  ["update.state", "read", "The updater's state: pumi's own (`app`: `status` idle, checking, downloading, ready, upToDate, failed or unavailable, the `version` found, `text` as Settings says it) and the plugins' (`plugins`: `checking`, the updates `found`).", () => engine.updateState()],
  ["update.check", "write", "Look for a newer pumi now and download it when there is one; `{plugins: true}` asks which installed plugins have a new version instead. The outcome is an `update.changed` event.", (args: unknown[]) => engine.checkForUpdates((args[0] as { plugins?: boolean } | undefined)?.plugins === true)],
  ["update.install", "write", "Quit, put the downloaded update in place, and open it; nothing while none is ready (`update.state`'s `app.status` is `ready`).", () => engine.restartToUpdate()],
] as const

/** Offers the update functions on the server, and tells it of each change. */
export function provideUpdates() {
  for (const [name, effect, desc, run] of UPDATES) {
    APP_FUNCTIONS.set(name, (args) => run(args as unknown[]) ?? null)
    engine.provide({
      name,
      since: 1,
      provider: "app",
      runs_on: "client",
      effect,
      agent: false,
      desc,
      parameters: name === "update.check" ? [["Dict", "opts"]] : [],
      params_schema: name === "update.check" ? { type: "object", properties: { opts: { type: "object", properties: { plugins: { type: "boolean" } } } } } : { type: "object" },
      returns: name === "update.state" ? "Object" : "nil",
      returns_schema: {},
      examples: [{ args: [] }],
      notes: [],
      source: "app",
    })
  }
  on("update", (data) => {
    call("event.emit", "update.changed", { data }).catch(() => {})
  })
}
