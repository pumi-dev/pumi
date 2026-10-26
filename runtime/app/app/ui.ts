// Plugins' window side: each plugin's `src/ui.tsx`, loaded on the main thread when one of its
// contributions is first drawn, and its themes. What the manifest says it contributes is
// registered at once (so the window knows its slots before any code runs): actions as data,
// components that load `ui.tsx` when they mount.
//
// In `ui.tsx` (and a theme), `import … from "pumi"` is the window's SDK, `pumi/theme` the kit a
// theme draws the window's parts with, `react` the app's own,
// and an import of the plugin's `main.ts` is not that file: each of its exports becomes a call to
// the plugin's worker (an `async` function, its arguments structured-cloned).
import * as React from "react"
import { lazy, type ComponentType } from "react"
import { realpathSync } from "node:fs"
import { basename, dirname, join, resolve, sep } from "node:path"
import * as jsx from "../jsx-dev-runtime.ts"
import * as sdk from "../sdk/index.ts"
import * as themeKit from "../sdk/theme-kit.ts"
import { addAction, addComponent, addRenderer, inCompartment, removePlugin } from "../sdk/contributions.ts"
import { frozen } from "./mode.ts"
import * as plugins from "./plugins.ts"
import { setThemeLoader, setThemes, type ThemeEntry } from "./themes.ts"
import { samePath } from "../native/platform.ts"

const transpiler = new Bun.Transpiler({ loader: "ts" })
/** A plugin's `main.ts`, by path: which plugin's it is. */
const mains = new Map<string, string>()
/** A plugin's `ui.tsx` and theme files' directories: which plugin's they are. */
const owners = new Map<string, string>()
const plain = (path: string) => path.replace(/\?.*$/, "")
/** `path` as Bun names a module: a linked plugin's files by where they really are, spelled so
 *  that these paths compare equal (`samePath`). */
const real = (path: string) => {
  try {
    return samePath(realpathSync(path))
  } catch {
    return samePath(path)
  }
}

let installed = false
function install() {
  if (installed) return
  installed = true
  Bun.plugin({
    name: "pumi-ui",
    setup(build) {
      build.module("pumi", () => ({ exports: { ...sdk }, loader: "object" }))
      // A UI theme's kit: the window's parts and the raw state the app's own theme draws with.
      build.module("pumi/theme", () => ({ exports: { ...themeKit }, loader: "object" }))
      build.module("react", () => ({ exports: { ...React, default: React }, loader: "object" }))
      for (const name of ["pumi/jsx-runtime", "pumi/jsx-dev-runtime", "react/jsx-runtime", "react/jsx-dev-runtime"]) {
        build.module(name, () => ({ exports: { ...jsx }, loader: "object" }))
      }
      build.onResolve({ filter: /^\.\.?\// }, (args) => {
        const importer = samePath(plain(args.importer ?? ""))
        const plugin = [...owners].find(([dir]) => importer.startsWith(dir))?.[1]
        if (!plugin) return undefined
        for (const path of [resolve(dirname(importer), args.path), resolve(dirname(importer), `${args.path}.ts`)]) {
          if (mains.get(samePath(path)) === plugin) return { path: samePath(path), namespace: "pumi-main" }
        }
        return undefined
      })
      build.onLoad({ filter: /.*/, namespace: "pumi-main" }, async (args) => {
        const plugin = mains.get(args.path)!
        const names = transpiler.scan(await Bun.file(args.path).text()).exports
        const exports = Object.fromEntries(names.map((name) => [name, (...a: unknown[]) => plugins.invoke(plugin, name, a)]))
        return { exports, loader: "object" }
      })
    },
  })
}

/** Each plugin's `ui.tsx` module once loaded, by the version it was loaded at. */
const loaded = new Map<string, { version: number; module: Promise<Record<string, unknown>> }>()
const versions = new Map<string, number>()

function loadUi(entry: plugins.Entry): Promise<Record<string, unknown>> {
  install()
  const version = versions.get(entry.id) ?? 0
  const had = loaded.get(entry.id)
  if (had && had.version === version) return had.module
  const module = import(`${real(entry.ui!)}?v=${version}`) as Promise<Record<string, unknown>>
  loaded.set(entry.id, { version, module })
  return module
}

/** A component that loads `name` from `entry`'s `ui.tsx` when it first draws. */
function lazyExport(entry: plugins.Entry, name: string): ComponentType<any> {
  return lazy(async () => {
    const module = await loadUi(entry)
    const found = module[name]
    if (typeof found !== "function") throw new Error(`src/ui.tsx has no export ${name}`)
    // What it draws is in a compartment of its own: whatever it draws, it costs the window's
    // frames only when it changes, and only its share of them when it is slow to draw.
    return { default: inCompartment(found as ComponentType<any>, entry.id) }
  })
}

/** Each Settings component made (`settingsComponent`), by plugin, version and export: one component each. */
const settingsComponents = new Map<string, ComponentType<any>>()

/**
 * The export `name` of plugin `id`'s `ui.tsx`, as a component that loads it when Settings first
 * shows it (`contributes.settings`); nothing when the plugin has no window side, or is left out.
 */
export function settingsComponent(id: string, name: string): ComponentType<any> | undefined {
  const entry = plugins.entries.get(id)
  if (!entry?.ui || frozen.has(id) || (entry.state === "disabled" && entry.error)) return undefined
  const key = `${id}@${versions.get(id) ?? 0}/${name}`
  let component = settingsComponents.get(key)
  if (!component) settingsComponents.set(key, (component = lazyExport(entry, name)))
  return component
}

const pascal = (id: string) => id.replace(/(^|[_-])([a-z])/g, (_, __, c: string) => c.toUpperCase())

/** The frozen plugins already said to be left out. */
const told = new Set<string>()

/** What each plugin registered, by a signature of what it was registered from. */
const registered = new Map<string, string>()

function register(entry: plugins.Entry) {
  const c = entry.manifest.contributes
  for (const [slot, given] of Object.entries(c?.slots ?? {})) {
    for (const action of given.actions ?? []) {
      addAction(slot, {
        id: action.id,
        title: action.title,
        icon: action.icon,
        plugin: entry.id,
        run: (args) => {
          plugins.command(entry.id, action.command, args).catch((err) => console.error(`pumi: ${entry.id}: ${action.title}:`, err?.message ?? err))
        },
      })
    }
    if (given.component && entry.ui) {
      addComponent(slot, { id: given.component, plugin: entry.id, title: given.title, icon: given.icon, Component: lazyExport(entry, given.component) })
    }
  }
  if (!entry.ui) return
  // Its own blocks, each drawn by the export named for it (`diagram` → `Diagram`), and the
  // blocks it draws but did not define.
  for (const block of c?.blocks ?? []) {
    addRenderer(`${entry.prefix}${block.id}`, { id: block.id, plugin: entry.id, title: block.title, icon: block.icon, Component: lazyExport(entry, pascal(block.id)) })
  }
  for (const [block, given] of Object.entries(c?.renderers ?? {})) {
    const name = typeof given === "string" ? given : given.export
    const match = typeof given === "string" ? undefined : given.match
    addRenderer(block, { id: name, plugin: entry.id, match, Component: lazyExport(entry, name) })
  }
}

/** Follows the plugins: what each contributes, and its themes. */
function sync() {
  const seen = new Set<string>()
  const themes: ThemeEntry[] = []
  for (const entry of plugins.entries.values()) {
    for (const path of entry.manifest.themes ?? []) {
      const dir = resolve(entry.dir, String(path))
      themes.push({ id: basename(dir), plugin: entry.id, dir, version: versions.get(entry.id) ?? 0 })
      owners.set(`${real(dir)}${sep}`, entry.id)
    }
    if (entry.state === "disabled" && entry.error) continue
    // Its window side froze the app's JS: left out until the app starts again (its logic runs).
    if (frozen.has(entry.id)) {
      if (!told.has(entry.id)) {
        told.add(entry.id)
        const line = "pumi: its window side (src/ui.tsx) froze the app's windows, and is left out until pumi starts again"
        console.error(`${entry.id}: ${line}`)
        plugins.note(entry.id, line)
      }
      continue
    }
    seen.add(entry.id)
    if (entry.main) mains.set(real(entry.main), entry.id)
    if (entry.ui) owners.set(`${real(dirname(entry.ui))}${sep}`, entry.id)
    const signature = JSON.stringify([entry.manifest.contributes, entry.ui, versions.get(entry.id) ?? 0])
    if (registered.get(entry.id) === signature) continue
    removePlugin(entry.id)
    register(entry)
    registered.set(entry.id, signature)
  }
  for (const id of [...registered.keys()]) {
    if (seen.has(id)) continue
    removePlugin(id)
    registered.delete(id)
  }
  // Settings → Appearance lists them (`useThemes`), each with its preview.
  setThemes(themes)
}

/** Loads a theme's module (`theme.tsx`, or `theme.ts`): its default export is its spec. */
export async function loadTheme(theme: ThemeEntry): Promise<sdk.ThemeSpec> {
  install()
  for (const file of ["theme.tsx", "theme.ts"]) {
    const path = join(real(theme.dir), file)
    if (!(await Bun.file(path).exists())) continue
    const module = await import(`${path}?v=${theme.version}`)
    if (!module.default?.name) throw new Error(`${path}: its default export is not a defineTheme({...})`)
    return module.default as sdk.ThemeSpec
  }
  throw new Error(`${theme.dir} has no theme.tsx`)
}

/** A plugin reloaded (`pumi plugin dev`): its window side loads again when next drawn. */
export function reloaded(id: string) {
  versions.set(id, (versions.get(id) ?? 0) + 1)
  sync()
}

plugins.subscribe(sync)
plugins.onReload(reloaded)
setThemeLoader(loadTheme)
sync()
