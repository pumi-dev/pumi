// How long startup takes, step by step, in ms since the process started: always kept (a push
// each), for Diagnostics; `PUMI_TRACE=1` prints them too.
const on = !!process.env.PUMI_TRACE
const kept: [string, number][] = []

export function mark(name: string) {
  kept.push([name, performance.now()])
}

/** The marks so far, in order. */
export function marks(): [string, number][] {
  return kept.slice()
}

/** The native side's lines (`pumi-trace NAME EPOCH_MS`) are read beside these. */
export function since(epochMs: number): number {
  return epochMs - performance.timeOrigin
}

export function report() {
  if (!on) return
  console.error(`pumi-trace origin ${performance.timeOrigin.toFixed(1)}`)
  for (const [name, at] of kept) console.error(`pumi-trace ${name.padEnd(16)} ${at.toFixed(1)} ms`)
}
