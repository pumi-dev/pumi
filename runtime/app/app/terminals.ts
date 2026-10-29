// Which tabs run on the core's terminal engine: a block declared with `engine: "terminal"`. The
// app learns it from the tab, not the block's name: the core sets `tab_pid` on every tab whose
// program it runs.

/** Whether `tab` (a snapshot's) is a terminal: a block on the terminal engine. */
export function runsTerminal(tab: { options?: Record<string, unknown> | null } | undefined): boolean {
  return typeof tab?.options?.tab_pid === "number"
}
