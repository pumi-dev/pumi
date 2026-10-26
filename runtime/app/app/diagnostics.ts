// What the app's JS measures of itself for Diagnostics (`src/diagnostics`): where this thread's
// time goes, a second at a time, sent to the native side while it watches (a `diagnostics` event
// says when). The counters are always on and cost an addition each; the React walk, the lag timer
// and the sampler run only while watched.
//
// This module must be imported before React's reconciler is: it installs the DevTools hook React
// reports each commit to (`onCommitFiberRoot`), which its production build calls too. That build
// keeps no render timings, so React is measured in what it re-rendered.
import { appliedMutationStats } from "../native/mutations.ts"
import { marks } from "./trace.ts"

/** One second, as `diagnostics::Js` reads it. Rates are per second; durations are ms. */
export interface Sample {
  lagMaxMs: number
  blockedMsPerS: number
  heapMb: number
  commitsPerS: number
  rendersPerS: number
  hot: Named[]
  batchesPerS: number
  opsPerS: number
  applyMsPerS: number
  events: Named[]
  callsPerS: number
  callMeanMs: number
  callMaxMs: number
  callSlowest?: string
}

export interface Named {
  name: string
  perS: number
  msPerS: number
}

/** Where a second goes: the engine's `reportDiagnostics`. */
export type Report = (report: { sample?: Sample; startup?: [string, number][] }) => void

const LAG_EVERY = 50
const HOT = 8
const EVENTS = 8

// --- Counters the app feeds.

const events = new Map<string, { count: number; ms: number }>()
let calls = { count: 0, ms: 0, max: 0, slowest: undefined as string | undefined }

/** An engine event of `kind` was handled, in `ms`. */
export function countEvent(kind: string, ms: number) {
  const entry = events.get(kind)
  if (entry) {
    entry.count++
    entry.ms += ms
  } else events.set(kind, { count: 1, ms })
}

/** A call to the server came back after `ms`. */
export function countCall(method: string, ms: number) {
  calls.count++
  calls.ms += ms
  if (ms > calls.max) {
    calls.max = ms
    calls.slowest = method
  }
}

// --- React, through the DevTools hook.

/** Fiber tags of components that render: function, class, forwardRef, simple memo. */
const COMPONENT_TAGS = new Set([0, 1, 11, 15])
/** `PerformedWork`: the fiber's render ran in this commit. React bubbles it into `subtreeFlags`. */
const PERFORMED = 1

let watching = false
let commits = 0
let renders = 0
const rendered = new Map<string, number>()

interface Fiber {
  tag: number
  type: any
  flags: number
  subtreeFlags: number
  child: Fiber | null
  sibling: Fiber | null
}

function nameOf(fiber: Fiber): string {
  const type = fiber.tag === 11 ? fiber.type?.render : fiber.type
  return type?.displayName || type?.name || "Anonymous"
}

/**
 * Counts what the commit re-rendered. A subtree is entered only when React marked work under it
 * in this render: a bailed-out subtree keeps last render's flags, and its parent's say so.
 */
export function walk(root: { current?: Fiber }) {
  const host = root.current
  if (!host) return
  commits++
  const stack: Fiber[] = [host]
  while (stack.length) {
    const fiber = stack.pop()!
    if (!(fiber.subtreeFlags & PERFORMED)) continue
    for (let child = fiber.child; child; child = child.sibling) {
      if (child.flags & PERFORMED && COMPONENT_TAGS.has(child.tag)) {
        const name = nameOf(child)
        renders++
        rendered.set(name, (rendered.get(name) ?? 0) + 1)
      }
      stack.push(child)
    }
  }
}

function installHook() {
  const global = globalThis as { __REACT_DEVTOOLS_GLOBAL_HOOK__?: any }
  let next = 0
  global.__REACT_DEVTOOLS_GLOBAL_HOOK__ ??= {
    supportsFiber: true,
    renderers: new Map(),
    inject(internals: unknown) {
      this.renderers.set(++next, internals)
      return next
    },
    onCommitFiberRoot() {},
  }
  const hook = global.__REACT_DEVTOOLS_GLOBAL_HOOK__
  const previous = hook.onCommitFiberRoot
  hook.onCommitFiberRoot = function (this: unknown, id: number, root: { current?: Fiber }, ...rest: unknown[]) {
    // React calls this in a try of its own: a walk that throws loses one commit's count.
    if (watching) walk(root)
    return previous?.call(this, id, root, ...rest)
  }
}
installHook()

// --- The sampler.

let report: Report = () => {}
let timers: ReturnType<typeof setInterval>[] = []
let lag = { at: 0, max: 0, total: 0 }
let last = { at: 0, batches: 0, ops: 0, ms: 0 }

const round = (n: number) => Math.round(n * 10) / 10

function reset() {
  const mutations = appliedMutationStats()
  last = { at: performance.now(), batches: mutations.batches, ops: mutations.ops, ms: mutations.ms }
  lag = { at: last.at, max: 0, total: 0 }
  commits = renders = 0
  rendered.clear()
  events.clear()
  calls = { count: 0, ms: 0, max: 0, slowest: undefined }
}

/** The second since the last, as rates; the counts start again. */
export function sample(): Sample {
  const now = performance.now()
  const seconds = Math.max(0.001, (now - last.at) / 1000)
  const per = (n: number) => round(n / seconds)
  const mutations = appliedMutationStats()
  const next: Sample = {
    lagMaxMs: round(lag.max),
    blockedMsPerS: per(lag.total),
    heapMb: Math.round(process.memoryUsage().heapUsed / 1048576),
    commitsPerS: per(commits),
    rendersPerS: per(renders),
    hot: [...rendered]
      .sort((a, b) => b[1] - a[1])
      .slice(0, HOT)
      .map(([name, count]) => ({ name, perS: per(count), msPerS: 0 })),
    batchesPerS: per(mutations.batches - last.batches),
    opsPerS: per(mutations.ops - last.ops),
    applyMsPerS: per(mutations.ms - last.ms),
    events: [...events]
      .sort((a, b) => b[1].ms - a[1].ms)
      .slice(0, EVENTS)
      .map(([name, e]) => ({ name, perS: per(e.count), msPerS: per(e.ms) })),
    callsPerS: per(calls.count),
    callMeanMs: round(calls.count ? calls.ms / calls.count : 0),
    callMaxMs: round(calls.max),
    ...(calls.slowest ? { callSlowest: calls.slowest } : {}),
  }
  reset()
  return next
}

function start() {
  reset()
  timers = [
    setInterval(() => {
      const at = performance.now()
      const late = Math.max(0, at - lag.at - LAG_EVERY)
      lag.at = at
      lag.max = Math.max(lag.max, late)
      lag.total += late
    }, LAG_EVERY),
    setInterval(() => send({ sample: sample() }), 1000),
  ]
  send({ startup: marks() })
}

function stop() {
  for (const timer of timers) clearInterval(timer)
  timers = []
}

function send(what: Parameters<Report>[0]) {
  try {
    report(what)
  } catch (err) {
    console.error("pumi: diagnostics:", String(err))
  }
}

/** Starts or stops measuring, as the native side's `diagnostics` event says. */
export function setWatching(on: boolean) {
  if (on === watching) return
  watching = on
  if (on) start()
  else stop()
}

/** Where the seconds go; set once the engine is up. */
export function connect(to: Report) {
  report = to
}
