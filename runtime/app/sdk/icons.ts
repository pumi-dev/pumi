// A tab's icon, as every theme shows it: the tab's own (`@icon`, which its block, its program or
// anyone may set), else its block's (`blocks[].icon` in the manifest), else the core block's. The value is an icon of the app's by name, a `data:image/…` URI, or
// `plugin:<path>`, a file in the plugin whose block the tab holds; as `@icon` it may also be
// `{src, tint}`. The server checks each (`pumi_api::icon`); what still doesn't resolve is skipped.
import { existsSync } from "node:fs"
import { isAbsolute, relative, resolve } from "node:path"
import { useMemo } from "react"
import { assets } from "../app/assets.ts"
import { NEW_TAB } from "../app/names.ts"
import { useApp } from "../app/state.ts"
import { useBlockOwner } from "./contributions.ts"

/** The tab option holding a tab's own icon. */
export const ICON = "@icon"
/** The most an icon's source may be, as the server has it (`pumi_api::icon::MAX_ICON`). */
const MAX_ICON = 16 * 1024

/** An icon to draw: a file or a `data:` URI, and whether it takes the theme's colour (an SVG does, unless it says not to). */
export interface IconSource {
  src: string
  tint: boolean
}

/** Where a tab's icon came from: its `@icon`, its block's manifest, or the core's. */
export type IconFrom = "tab" | "block" | "core"

export interface TabIconInfo extends IconSource {
  from: IconFrom
}

/** The core blocks' icons, and the one for a block without any. */
const CORE: Record<string, string> = { file: "file", browser: "tool_web", files: "folder", image: "image", [NEW_TAB]: "plus" }
const FALLBACK = "box"

const IMAGE = /^data:image\/(svg\+xml|png|jpeg|webp|gif)[;,]/i
const NAME = /^[a-z0-9_]{1,64}$/
const FILE = /\.(svg|png|jpe?g|webp|gif)$/i

const isSvg = (src: string) => /^data:image\/svg/i.test(src) || /\.svg$/i.test(src)

/**
 * `value` (`@icon`, or a block's `icon`) as an image to draw, `dir` the plugin a `plugin:` path
 * is in: `undefined` when it is none of an icon's forms, names no icon the app has, or names a
 * file outside the plugin or not there.
 */
export function iconSource(value: unknown, dir?: string): IconSource | undefined {
  let src = value
  let tint: boolean | undefined
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const v = value as { src?: unknown; tint?: unknown }
    src = v.src
    if (typeof v.tint === "boolean") tint = v.tint
  }
  if (typeof src !== "string" || !src || src.length > MAX_ICON) return undefined
  if (src.startsWith("data:")) return IMAGE.test(src) ? { src, tint: tint ?? isSvg(src) } : undefined
  if (src.startsWith("plugin:")) {
    const path = src.slice("plugin:".length)
    if (!dir || !FILE.test(path) || isAbsolute(path) || path.includes("\\")) return undefined
    const file = resolve(dir, path)
    const rel = relative(dir, file)
    if (!rel || rel.startsWith("..") || isAbsolute(rel) || !existsSync(file)) return undefined
    return { src: file, tint: tint ?? isSvg(file) }
  }
  if (!NAME.test(src)) return undefined
  const file = assets.icon(src)
  return file ? { src: file, tint: tint ?? true } : undefined
}

/**
 * What a tab with these shows, the first that resolves: its own icon, its block's icon (`owner`:
 * the plugin that defines the block, `dir` where it is), the core block's, then a box.
 */
export function tabIcon(tab: { block: string; options?: Record<string, unknown> | null } | undefined, owner?: { icon?: string; dir?: string }): TabIconInfo {
  const own = iconSource(tab?.options?.[ICON], owner?.dir)
  if (own) return { ...own, from: "tab" }
  const block = owner?.icon ? iconSource(owner.icon, owner.dir) : undefined
  if (block) return { ...block, from: "block" }
  const core = assets.icon(CORE[tab?.block ?? ""] ?? FALLBACK) ?? assets.icon(FALLBACK) ?? ""
  return { src: core, tint: true, from: "core" }
}

/** `tab`'s icon (`tabIcon`), drawn again when its `@icon` or its block's plugin changes. */
export function useTabIcon(tab: string | undefined): TabIconInfo {
  const block = useApp((s) => s.snapshot?.tabs.find((p) => p.tab_id === tab)?.block) ?? ""
  const own = useApp((s) => s.snapshot?.tabs.find((p) => p.tab_id === tab)?.options?.[ICON])
  const owner = useBlockOwner(block)
  const key = typeof own === "object" && own ? JSON.stringify(own) : own
  return useMemo(() => tabIcon({ block, options: { [ICON]: own } }, owner), [block, key, owner?.icon, owner?.dir])
}

/**
 * The status a tab shows (`@status`): what its block set, or what its program reported (OSC
 * 7501, or a fallback: OSC 9;4, a spinning title, a notification). `undefined` for a tab with
 * none.
 */
export function useTabStatus(tab: string | undefined): string | undefined {
  return useApp((s) => s.snapshot?.tabs.find((p) => p.tab_id === tab)?.status) || undefined
}

/** A status that says something happened (working, done, blocked, error): what a tab's icon gives way to. */
export const loudStatus = (status: string | undefined): status is string => !!status && status !== "idle" && status !== "unknown"

/**
 * Whether a tab shows its `status` in place of its `icon`, as the app's own `TabIcon` part does:
 * while it says something happened, and whenever its icon is only the core block's.
 */
export const showsStatus = (status: string | undefined, icon: Pick<TabIconInfo, "from">): status is string => !!status && (loudStatus(status) || icon.from === "core")
