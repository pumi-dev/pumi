// What the window remembers of itself between runs that is no one else's: a side bar's width,
// which sections are collapsed. One JSON file in the state directory; `useLocal` is state that
// persists there.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { useCallback, useRef, useSyncExternalStore } from "react"
import { Engine } from "../native/addon.js"

const FILE = join(Engine.stdpath("state") ?? ".", "window.json")
let values: Record<string, unknown> = {}
try {
  if (existsSync(FILE)) values = JSON.parse(readFileSync(FILE, "utf8"))
} catch {}
const listeners = new Set<() => void>()
let saving: ReturnType<typeof setTimeout> | undefined

export function getLocal<T>(key: string, fallback: T): T {
  return key in values ? (values[key] as T) : fallback
}

export function setLocal(key: string, value: unknown) {
  values = { ...values, [key]: value }
  changed()
}

/** Forgets what is kept under `key`. */
export function removeLocal(key: string) {
  if (!(key in values)) return
  const { [key]: _gone, ...rest } = values
  values = rest
  changed()
}

function changed() {
  for (const listener of listeners) listener()
  clearTimeout(saving)
  saving = setTimeout(() => {
    try {
      mkdirSync(dirname(FILE), { recursive: true })
      writeFileSync(FILE, JSON.stringify(values))
    } catch {}
  }, 300)
}

/** State kept across runs under `key`. */
export function useLocal<T>(key: string, given: T): [T, (next: T | ((old: T) => T)) => void] {
  // The first fallback given: a literal (`[]`) is a new value each render, and the store's
  // snapshot must not change unless the store does.
  const fallback = useRef(given).current
  const value = useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => getLocal(key, fallback),
  )
  const set = useCallback(
    (next: T | ((old: T) => T)) => setLocal(key, typeof next === "function" ? (next as (old: T) => T)(getLocal(key, fallback)) : next),
    [key, fallback],
  )
  return [value, set]
}
