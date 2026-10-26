// The server's events, in the app: `listen` subscribes the app's channel (`event.on`), and the
// engine hands each event back with its subscription's handle (`event`). `useEvent(name, fn)`
// runs `fn` with each one while the component is mounted; every listener in the window for one
// event (and match) shares one subscription, dropped with the last listener.
import { useEffect, useRef } from "react"
import { call, engine, on } from "../app/engine.ts"

/** An event as the app hears it: what fired, what it matched, what it is about, its data. */
export interface WindowEvent<D = any> {
  event: string
  match: string
  session?: string | null
  view?: string | null
  /** The tab it is about (`%3`, `devbox/%3`). */
  tab?: string | null
  data: D
  seq: number
}

export interface UseEventOptions {
  /** Globs tested against the event's `match` (a tab's type, a request's id, …). */
  match?: string | string[]
}

/** What `event.on` takes besides the events. */
export interface ListenOptions {
  match?: string | string[]
  once?: boolean
  desc?: string
}

/** Hears one subscription's events: the event, and the subscription's handle. */
export type Heard = (e: WindowEvent, handle: number) => void

/** Each subscription's listener, by handle. */
const listening = new Map<number, { fn: Heard; once: boolean }>()
/**
 * Events of a subscription whose handle `event.on` has not answered yet: the engine's events and
 * its answers come back on separate paths. Kept for a few handles at most.
 */
const early = new Map<number, WindowEvent[]>()
const EARLY_LIMIT = 64

function heard(handle: number, e: WindowEvent) {
  const held = listening.get(handle)
  if (!held) {
    const list = early.get(handle) ?? []
    list.push(e)
    early.set(handle, list)
    while (early.size > EARLY_LIMIT) early.delete(early.keys().next().value!)
    return
  }
  if (held.once) listening.delete(handle)
  try {
    held.fn(e, handle)
  } catch (err) {
    console.error(`pumi: a listener of ${e?.event} threw:`, err)
  }
}

on("event", ({ subscription, event }: { subscription: number; event: WindowEvent }) => heard(subscription, event))

/**
 * Subscribes the app's channel to `events` (names or globs) with `event.on`, and runs `fn` with
 * each; `caller` is who the subscription is made for (a plugin). Answers the handle.
 */
export async function listen(events: string | string[], opts: ListenOptions, fn: Heard, caller?: unknown): Promise<number> {
  const params = [events, { match: opts.match, once: opts.once, desc: opts.desc }]
  const handle: unknown = await (caller === undefined ? call("event.on", ...params) : engine.call("event.on", params, caller))
  if (typeof handle !== "number") throw new Error(`event.on answered ${JSON.stringify(handle)}`)
  listening.set(handle, { fn, once: !!opts.once })
  const missed = early.get(handle) ?? []
  early.delete(handle)
  for (const e of missed) heard(handle, e)
  return handle
}

/** Ends a subscription `listen` made. */
export function unlisten(handle: number): Promise<unknown> {
  listening.delete(handle)
  early.delete(handle)
  return call("event.off", handle)
}

type Listener = (e: WindowEvent) => void

interface Subscription {
  listeners: Set<Listener>
  /** The server's handle for it, once `event.on` answered (`undefined` if it refused). */
  handle: Promise<number | undefined>
}

const subscriptions = new Map<string, Subscription>()

/** Hears `event` (a name, or a glob such as `agent.*`) from the server; returns what stops it. */
export function subscribeEvent(event: string, match: string | string[] | undefined, fn: Listener): () => void {
  const at = JSON.stringify([event, match ?? null])
  let sub = subscriptions.get(at)
  if (!sub) {
    const listeners = new Set<Listener>()
    const handle = listen(event, { match, desc: "a window's useEvent" }, (e) => {
      for (const listener of [...listeners]) listener(e)
    }).catch((err: any) => {
      console.error(`pumi: useEvent(${event}):`, err?.message ?? err)
      return undefined
    })
    sub = { listeners, handle }
    subscriptions.set(at, sub)
  }
  const held = sub
  held.listeners.add(fn)
  return () => {
    held.listeners.delete(fn)
    if (held.listeners.size > 0 || subscriptions.get(at) !== held) return
    subscriptions.delete(at)
    held.handle.then((handle) => handle !== undefined && unlisten(handle).catch(() => {}))
  }
}

/**
 * Runs `fn` with each server event `event` while the component is mounted: an event pumi fires
 * (`tab.changed`, `option.changed`, …) or a plugin's own (`owner.name.done`). `fn` may change
 * between renders; the subscription stays.
 */
export function useEvent<D = any>(event: string, fn: (e: WindowEvent<D>) => void, opts: UseEventOptions = {}) {
  const latest = useRef(fn)
  latest.current = fn
  const match = opts.match === undefined ? undefined : JSON.stringify(opts.match)
  useEffect(() => subscribeEvent(event, match === undefined ? undefined : JSON.parse(match), (e) => latest.current(e)), [event, match])
}
