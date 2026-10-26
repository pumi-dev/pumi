// What the app asks of the OS it runs on that is a line of TypeScript rather than a `pumi_sys`
// capability: whether it is the one file `bun run app` builds, how it spells a path, and where a
// file embedded in that build is.
import { join } from "node:path"

/** The app is the compiled executable: its modules live in Bun's virtual file system then,
 *  `/$bunfs/root` on Unix and `B:\~BUN\root` on Windows. */
export const COMPILED = import.meta.url.includes("$bunfs") || import.meta.url.includes("~BUN")

/**
 * An entry point of the compiled app, by the path it has in `runtime/app`: `embedded("app",
 * "ui-worker.js")`, `embedded("worker", "worker.js")`.
 *
 * Bun embeds every entry point under its virtual root, `/$bunfs/root` on Unix and
 * `B:\~BUN\root` on Windows, at the path it has in `runtime/app` — wherever the module that asks
 * was bundled. `node:fs` cannot stat inside that virtual file system, so the root is not probed:
 * it is the constant Bun documents, and the entry point's path under it is the one built.
 */
export function embedded(...path: string[]): string {
  return join(process.platform === "win32" ? "B:\\~BUN\\root" : "/$bunfs/root", ...path)
}

/**
 * `path` as paths are compared by it: on Windows `\` throughout and a lower-case drive letter,
 * where what realpath answers (`C:\…`) and what Bun names a module are spelled differently.
 */
export function samePath(path: string): string {
  return process.platform === "win32" ? path.replace(/\//g, "\\").replace(/^([A-Za-z]):/, (_, letter: string) => `${letter.toLowerCase()}:`) : path
}
