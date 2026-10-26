// The app's entry: `bun app/main.ts` (after `bun run addon`).
//
// - React runs its production build unless `PUMI_DEV=1` asks for the development one (its
//   warnings, its profiler). This has to be set before anything imports React, so the app itself
//   is imported after it.
// - Run from this checkout (beside a built `target/debug/pumid`), it is a debug build's app: its
//   own server and files (`pumi-dev`), and that pumid, never the installed pumi's. Launched from
//   one of the installed pumi's tabs, it runs itself again without that pumi's `$PUMI` and the
//   rest: the environment can't be changed in place (Bun keeps its own copy, and changing the
//   process's under it crashes the next worker).
import { existsSync } from "node:fs"
import { join } from "node:path"

// The compiled app runs its isolated plugins as itself: `pumi <worker.ts> --isolate`.
if (process.argv.includes("--isolate")) {
  await import("../worker/worker.ts")
  await new Promise(() => {})
}

process.env.NODE_ENV ??= process.env.PUMI_DEV ? "development" : "production"
const debug = join(import.meta.dir, "..", "..", "..", "target", "debug", process.platform === "win32" ? "pumid.exe" : "pumid")
if (existsSync(debug) && (process.env.PUMI_APPNAME !== "pumi-dev" || !process.env.PUMID_BIN)) {
  const env: Record<string, string | undefined> = { ...process.env, PUMI_APPNAME: "pumi-dev", PUMID_BIN: process.env.PUMID_BIN || debug }
  if (process.env.PUMI_APPNAME !== "pumi-dev") {
    for (const name of ["PUMI", "PUMI_TAB", "PUMI_VIEW", "PUMI_SESSION", "PUMI_VERSION"]) delete env[name]
  }
  const child = Bun.spawn([process.execPath, ...process.argv.slice(1)], { env, stdio: ["inherit", "inherit", "inherit"] })
  for (const signal of ["SIGINT", "SIGTERM"] as const) process.on(signal, () => child.kill(signal))
  process.exit(await child.exited)
}
// The app runs in a worker of its own, and this thread holds the addon and pumps AppKit
// (`native/bridge-host.ts`): whatever a theme or a plugin's window side does, the windows keep
// drawing and the terminals keep taking keys. `PUMI_UI_THREAD=main` runs the app on this thread;
// the command line (`pumi COMMAND …`) always does.
const { isCli } = await import("./cli.ts")
if (process.env.PUMI_UI_THREAD === "main" || isCli(process.argv.slice(2))) await import("./app.tsx")
else (await import("../native/bridge-host.ts")).host()
export {}
