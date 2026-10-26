// What `bridge.test.ts` runs in the app's worker instead of the app: the proxies, used as the app
// uses them, each answer said back to the test (`{ t: "test" }`).
import { Engine, GpuixRenderer } from "../addon.js"
import { drawing } from "../blame.ts"

const bridge = (globalThis as any).__pumiBridge
const say = (what: string, value?: unknown) => bridge.post({ t: "test", what, value })
say("restart", bridge.init.restart ?? null)

const engine = Engine.start({ dataDir: "/nowhere" }, (event: any) => {
  if (event.kind === "spin") drawing(event.data, () => {
    for (;;) {}
  })
  say("event", event)
})
const renderer = new GpuixRenderer((_error: Error | null, event: any) => say("renderer.event", event))
renderer.init({ title: "test" })
renderer.applyBatch(JSON.stringify([["createElement", bridge.init.idBase + 1, "div"], ["setRoot", bridge.init.idBase + 1]]))
say("size", renderer.getWindowSize())
say("initialized", renderer.isInitialized())
say("tree", renderer.getAutomationTree().length)
say("stdpath", Engine.stdpath("state"))
try {
  renderer.getElementBounds(-1)
} catch (error: any) {
  say("thrown", error.message)
}
engine.call("echo", [1, 2]).then((value: unknown) => say("call", value))
bridge.ready()
