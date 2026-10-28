// The addon as `pumi/testing` loads it, in a plugin's tests outside this repository: in place of
// native/addon.js, which `bun run sdk` swaps this for. PUMI_ADDON names it; else it is the
// installed app's, where scripts/release/install.sh and install.ps1 put it. No addon is on npm.
//
// With none found, the test renderer is missing as it is from a release build: `hasNativeTestRenderer`
// is false, and a test that needs it skips.
import { existsSync } from "node:fs"
import { homedir } from "node:os"
import { join } from "node:path"

const name = `pumi.${process.platform}-${process.arch}.node`

/** Where an installed app's addon is, by platform: beside its executable. */
export function installedAddons(): string[] {
  if (process.platform === "darwin") {
    return ["/Applications", join(homedir(), "Applications")].map((dir) => join(dir, "pumi.app", "Contents", "MacOS", name))
  }
  if (process.platform === "win32") {
    const local = process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local")
    return [join(local, "Programs", "pumi", "pumi.app", "bin", name)]
  }
  return [join(homedir(), ".local", "pumi.app", "bin", name)]
}

/** The addon's path: PUMI_ADDON, else the first installed app's that exists. */
export const addonPath: string | null = process.env.PUMI_ADDON || installedAddons().find((path) => existsSync(path)) || null

type Addon = Pick<typeof import("../native/addon"), "TestGpuixRenderer" | "hasTestGpuixRenderer">

// Without an addon there is no test renderer class, as on Linux: native/testing.ts reads
// `TestGpuixRenderer` only when `hasTestGpuixRenderer()` is true.
function load(): Partial<Addon> & Pick<Addon, "hasTestGpuixRenderer"> {
  if (!addonPath) return { hasTestGpuixRenderer: () => false }
  const native = { exports: {} as Addon }
  process.dlopen(native, addonPath)
  return native.exports
}

const addon = load()
export const hasTestGpuixRenderer = addon.hasTestGpuixRenderer
export const TestGpuixRenderer = addon.TestGpuixRenderer!
