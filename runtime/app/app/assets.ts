// What the app ships beside its code: the icons (assets/icons) and the default theme's preview
// (assets/themes).
import { existsSync } from "node:fs"
import { dirname, join } from "node:path"
import { COMPILED } from "../native/platform.ts"

// The checkout's assets; the compiled app's are in the bundle's Resources, or beside `target/`.
const exe = dirname(process.execPath)
const ROOT = COMPILED
  ? [join(exe, "..", "Resources", "assets"), join(exe, "..", "assets")].find((d) => existsSync(d)) ?? join(exe, "assets")
  : join(import.meta.dir, "..", "..", "..", "assets")

/** The icon names that are another's file: what plugins call an icon, old names. */
const ALIASES: Record<string, string> = {
  agent: "person",
  branch: "git_branch",
  dot: "status_dot",
  search: "magnifying_glass",
  sliders: "settings",
}

export const assets = {
  /** An icon's SVG file (`assets/icons/<name>.svg`), by name or by one of its other names. */
  icon(name: string): string | undefined {
    const path = join(ROOT, "icons", `${ALIASES[name] ?? name}.svg`)
    return existsSync(path) ? path : undefined
  },
  /** A built-in theme's preview for Settings → Appearance (`assets/themes/<name>.png`, or `.svg`). */
  themePreview(name: string): string | undefined {
    return ["png", "svg"].map((ext) => join(ROOT, "themes", `${name}.${ext}`)).find((path) => existsSync(path))
  },
}
