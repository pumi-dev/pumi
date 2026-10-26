// The addon; addon.d.ts types it. PUMI_ADDON names it; else it is beside this file (`bun run
// addon` put it there), or, in the compiled app (`bun run app`), beside the executable.
//
// In the app's worker (`app/ui-worker.ts`) it is not loaded: the classes are proxies of the main
// thread's (`bridge-client.ts`), which holds the addon, the windows and the frame loop.
import { dirname, join } from "node:path"
import { COMPILED } from "./platform.ts"

async function load() {
  if (globalThis.__pumiBridge) return (await import("./bridge-client.ts")).addon
  const name = `pumi.${process.platform}-${process.arch}.node`
  const path = process.env.PUMI_ADDON ?? join(COMPILED ? dirname(process.execPath) : import.meta.dir, name)
  const native = { exports: {} }
  process.dlopen(native, path)
  return native.exports
}

export const { Engine, GpuixRenderer, TestGpuixRenderer, hasTestGpuixRenderer, setCapturedKeys } = await load()
