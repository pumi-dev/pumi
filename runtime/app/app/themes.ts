// The UI themes there are: each a directory a plugin's manifest lists under `themes`, named as
// that directory is (`ui_theme = "<name>"`). A theme's module loads when a window first uses it.
import { useEffect, useState, useSyncExternalStore } from "react"
import type { ThemeSpec } from "../sdk/theme.ts"

export interface ThemeEntry {
  /** Its name: the directory's. */
  id: string
  plugin: string
  dir: string
  /** Its plugin's reload count: a reload loads it again. */
  version: number
}

let themes: ThemeEntry[] = []
const listeners = new Set<() => void>()
const specs = new Map<string, Promise<ThemeSpec>>()
let load: (theme: ThemeEntry) => Promise<ThemeSpec> = () => Promise.reject(new Error("no theme loader"))

export function setThemes(next: ThemeEntry[]) {
  themes = next
  for (const listener of listeners) listener()
}

/** The themes there are now, outside a window's tree. */
export function getThemes(): ThemeEntry[] {
  return themes
}

export function setThemeLoader(loader: (theme: ThemeEntry) => Promise<ThemeSpec>) {
  load = loader
}

export function useThemes(): ThemeEntry[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => themes,
  )
}

/** The theme named `name` once it has loaded (undefined while it loads, or if it fails). */
export function useThemeSpec(name: string | undefined): { spec?: ThemeSpec; error?: string } {
  const theme = useThemes().find((t) => t.id === name)
  const [state, setState] = useState<{ key?: string; spec?: ThemeSpec; error?: string }>({})
  const key = theme ? `${theme.dir}@${theme.version}` : undefined
  useEffect(() => {
    if (!theme || !key) return
    let promise = specs.get(key)
    if (!promise) specs.set(key, (promise = load(theme)))
    let live = true
    promise.then(
      (spec) => live && setState({ key, spec }),
      (err) => live && setState({ key, error: String(err?.message ?? err) }),
    )
    return () => {
      live = false
    }
  }, [key])
  return state.key === key ? state : {}
}
