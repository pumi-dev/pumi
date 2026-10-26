// How the app was started: safe mode (`--safe`, `PUMI_SAFE_MODE=1`) draws the default theme and
// runs no plugin.
const argv = process.argv.slice(2)
/** What the main thread's watchdog left out, starting the app's worker again after it froze. */
export const restart = (globalThis as { __pumiBridge?: { init: import("../native/bridge-protocol.ts").Init } }).__pumiBridge?.init.restart
export const safe = argv.includes("--safe") || argv.includes("--safe-mode") || process.env.PUMI_SAFE_MODE === "1" || restart?.safe === true
/** The theme froze the app: the default draws until the app starts again. */
export const themeOff = restart?.themeOff === true
/** The plugins whose window side froze the app: left out until the app starts again. */
export const frozen: ReadonlySet<string> = new Set(restart?.frozen ?? [])
