// `bun run app [--target <triple>]`: the app as one executable, `target/pumi` (`target/pumi.exe`
// for Windows), minified, with the addon beside it (`target/pumi.<platform>-<arch>.node`, as
// `bun run addon` built it). Its workers are the same executable: worker.ts (a plugin's) and
// app/ui-worker.ts (the app's) are entry points.
import { copyFileSync } from "node:fs"
import { join } from "node:path"
import { addonName, appName, bunTarget, targetTriple } from "./target.ts"

const root = join(import.meta.dir, "..")
const target = join(root, "..", "..", "target")
const triple = targetTriple()
const addon = addonName(triple)
const executable = appName(triple)
// Bun builds for the machine it runs on unless told the target, and then fetches that target's
// own Bun to compile with.
const cross = bunTarget(triple)
const started = performance.now()
const built = Bun.spawnSync(
  [
    process.execPath,
    "build",
    "--compile",
    ...(cross ? [`--target=${cross}`] : []),
    // No `--bytecode`: it costs 10 MB of a 75 MB executable and starts no faster (measured:
    // 0.08 s either way on this app's ~1 MB of JS).
    "--minify",
    "--format",
    "esm",
    "--sourcemap",
    // A plugin's main.ts and ui.tsx are imported from disk: without package.json at runtime, a
    // compiled Bun resolves no bare specifier through node_modules (npm or `file:` dependencies).
    "--compile-autoload-package-json",
    "--define",
    'process.env.NODE_ENV="production"',
    join(root, "app", "main.ts"),
    join(root, "worker", "worker.ts"),
    join(root, "app", "ui-worker.ts"),
    "--outfile",
    join(target, executable),
  ],
  { cwd: root, stdio: ["inherit", "inherit", "inherit"] },
)
if (built.exitCode !== 0) process.exit(built.exitCode ?? 1)
copyFileSync(join(root, "native", addon), join(target, addon))
console.log(`target/${executable} and target/${addon} in ${((performance.now() - started) / 1000).toFixed(1)} s`)
