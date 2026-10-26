// A plugin's own channel from its `main.ts` to its `ui.tsx`, in process: the worker posts
// (`ui.post(topic, data)`), the app hands each message here, and the plugin's components hear it
// (`useChannel`). No server is between them, so a component can follow a stream token by token. Only
// the app that runs the plugin's worker (the plugin host) hears it; another app's windows ask
// `main.ts` through its exports (`<prefix>rpc`).
import { useCallback, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react"
import { usePlugin } from "./contributions.ts"

type Listener = (data: unknown) => void

const key = (plugin: string, topic: string) => `${plugin}\n${topic}`
/** The last message on each plugin's topic, for a component that mounts after it. */
const last = new Map<string, unknown>()
const listeners = new Map<string, Set<Listener>>()

/** A message from `plugin`'s worker on `topic`: kept as the topic's last, and heard by its listeners. */
export function deliver(plugin: string, topic: string, data: unknown) {
  const at = key(plugin, topic)
  last.set(at, data)
  for (const listener of [...(listeners.get(at) ?? [])]) {
    try {
      listener(data)
    } catch (err) {
      console.error(`pumi: a listener of ${plugin}'s ${topic} threw:`, err)
    }
  }
}

/** Hears each message `plugin` posts on `topic` from now on; returns what stops it. */
export function listen(plugin: string, topic: string, fn: Listener): () => void {
  const at = key(plugin, topic)
  const set = listeners.get(at) ?? new Set()
  set.add(fn)
  listeners.set(at, set)
  return () => {
    set.delete(fn)
    if (set.size === 0) listeners.delete(at)
  }
}

/** The last message `plugin` posted on `topic`, if any. */
export function latest(plugin: string, topic: string): { data: unknown } | undefined {
  const at = key(plugin, topic)
  return last.has(at) ? { data: last.get(at) } : undefined
}

/** Forgets what `plugin` posted: it stopped. */
export function forget(plugin: string) {
  for (const at of [...last.keys()]) if (at.startsWith(`${plugin}\n`)) last.delete(at)
}

function own(plugin: string | undefined): string {
  if (!plugin) throw new Error("pumi: useChannel is for a plugin's component (one its manifest contributes)")
  return plugin
}

/**
 * What the plugin's `main.ts` posts on `topic` (`ui.post(topic, data)`), in one of its components.
 *
 * Without `reduce`, the last message (`initial` until there is one; a component that mounts later
 * reads the last one too). With `reduce`, each message from when the component mounted folds into its state,
 * in order: `useChannel("delta", "", (text, d: string) => text + d)`. A message sent before it
 * mounted is not replayed: read where things stand from an export, then fold what follows.
 */
export function useChannel<T>(topic: string, initial: T, reduce?: (state: T, data: any) => T): T {
  const plugin = own(usePlugin())
  const folding = useRef(reduce)
  folding.current = reduce
  const [folded, setFolded] = useState(initial)
  useLayoutEffect(() => {
    if (!folding.current) return
    return listen(plugin, topic, (data) => setFolded((state) => folding.current!(state, data)))
  }, [plugin, topic, !!reduce])
  const folds = !!reduce
  const subscribe = useCallback((changed: () => void) => (folds ? () => {} : listen(plugin, topic, changed)), [plugin, topic, folds])
  const lastValue = useSyncExternalStore(subscribe, () => (folds ? undefined : last.get(key(plugin, topic))))
  if (reduce) return folded
  return last.has(key(plugin, topic)) ? (lastValue as T) : initial
}

/** Runs `fn` with each message the plugin's `main.ts` posts on `topic`, while the component is mounted. */
export function useChannelListener(topic: string, fn: (data: any) => void) {
  const plugin = own(usePlugin())
  const latestFn = useRef(fn)
  latestFn.current = fn
  useLayoutEffect(() => listen(plugin, topic, (data) => latestFn.current(data)), [plugin, topic])
}
