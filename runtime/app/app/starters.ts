// The plugins the start screen offers on the first run: pumi itself runs terminals, pages and
// agents' sessions, but every tab is a plugin's block (`pumi-dev/plugins`), so a fresh install has
// nothing to open until some are installed. Each is ticked to start with; the user unticks what
// they don't want. They are offered on the first run only: once shown, a later run doesn't offer
// them again, answered or not (`OFFERED`, kept in window.json: the window's own, not a setting).
// In the run that shows them, installing them puts them away.
//
// After installing, new tabs open the first of them that is installed, in this order: what a new
// tab holds is the `new_tab` option, which pumi leaves empty.
import { useCallback, useEffect, useState, useSyncExternalStore } from "react"
import { subscribeEvent } from "../sdk/events.ts"
import { call } from "./engine.ts"
import { getLocal, setLocal } from "./local.ts"
import { setOption } from "./settings/store.ts"

export interface Starter {
  /** Its folder's name, which an installed plugin's `spec.name` is. */
  name: string
  title: string
  detail: string
  icon: string
  /** What `plugin.add` is given. */
  source: string
  /** The block it declares that a new tab can hold (`new_tab`). */
  block: string
}

/** In the order they are offered, and the order `new_tab` takes the first installed of. */
export const STARTERS: Starter[] = [
  { name: "launcher", title: "Launcher", detail: "a list of what to open", icon: "plus", source: "pumi-dev/plugins/launcher", block: "pumi_dev.launcher.new" },
  { name: "terminal", title: "Terminal", detail: "a shell in a tab", icon: "terminal", source: "pumi-dev/plugins/terminal", block: "pumi_dev.terminal.shell" },
  { name: "agents", title: "Agents", detail: "Claude Code, Codex, pi…", icon: "sparkle", source: "pumi-dev/plugins/agents", block: "pumi_dev.agents.term" },
  { name: "browser", title: "Browser", detail: "a web page in a tab", icon: "globe", source: "pumi-dev/plugins/browser", block: "pumi_dev.browser.page" },
]

/** How Agents opens them, as its `prefer` option says: in their terminal, or in a chat tab (its GUI over the SDK, else ACP). */
export type AgentsMode = "terminal" | "sdk"
/** Agents' option that sets every agent at once. */
export const AGENTS_PREFER = "pumi_dev.agents.prefer"

/** window.json's key: whether a run's start screen has shown the starters. */
export const OFFERED = "start.offered"

/** Whether an earlier run showed them, read as this one starts: marking them shown now doesn't put them away. */
const offeredBefore = getLocal(OFFERED, false)

/** Installed in this run: every window's start screen stops offering them. */
let answered = false
const answeredListeners = new Set<() => void>()
function answer() {
  if (answered) return
  answered = true
  for (const l of answeredListeners) l()
}
function useAnswered(): boolean {
  return useSyncExternalStore(
    (l) => {
      answeredListeners.add(l)
      return () => answeredListeners.delete(l)
    },
    () => answered,
  )
}

type Installed = { spec?: { name?: unknown } }[]

const namesOf = (installed: Installed) => new Set(installed.map((p) => p?.spec?.name))

/** The starters no installed plugin (`registry.list("plugin")`'s) is named for. */
export function notInstalled(installed: Installed, starters = STARTERS): Starter[] {
  const names = namesOf(installed)
  return starters.filter((s) => !names.has(s.name))
}

/** The block new tabs open once these are installed: the first starter's that is, in order. */
export function newTabBlock(installed: Installed, starters = STARTERS): string | undefined {
  const names = namesOf(installed)
  return starters.find((s) => names.has(s.name))?.block
}

export interface Starters {
  /** What to offer: the starters not installed, on the first run until answered. `undefined` until read. */
  offered?: Starter[]
  /** The names ticked. */
  picked: Set<string>
  toggle(name: string): void
  /** The one being installed. */
  busy?: string
  error?: string
  /** Installs the ticked ones, in order, then points `new_tab` at the first installed. */
  install(): void
  /** How agents open, set as Agents installs: the chat until picked. */
  agents: AgentsMode
  setAgents(mode: AgentsMode): void
}

export function useStarters(): Starters {
  const done = useAnswered() || offeredBefore
  const [missing, setMissing] = useState<Starter[]>()
  const [unticked, setUnticked] = useState<Set<string>>(() => new Set())
  const [busy, setBusy] = useState<string>()
  const [error, setError] = useState<string>()
  const [agents, setAgents] = useState<AgentsMode>("sdk")
  const reload = useCallback(
    () =>
      call("registry.list", "plugin", {}).then(
        (list: unknown[]) => {
          const installed = (list ?? []) as Installed
          setMissing(notInstalled(installed))
          return installed
        },
        () => {
          setMissing([])
          return [] as Installed
        },
      ),
    [],
  )
  useEffect(() => {
    if (done) return
    reload()
    return subscribeEvent("registry.changed", undefined, () => void reload())
  }, [done, reload])
  const offered = done ? [] : missing
  // Shown once: from the next run on, the start screen doesn't offer them.
  const showing = !!offered?.length
  useEffect(() => {
    if (showing && !getLocal(OFFERED, false)) setLocal(OFFERED, true)
  }, [showing])
  const picked = new Set((offered ?? []).map((s) => s.name).filter((n) => !unticked.has(n)))
  const toggle = useCallback((name: string) => {
    setUnticked((was) => {
      const next = new Set(was)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  }, [])
  const install = useCallback(() => {
    if (busy || !offered) return
    const chosen = offered.filter((s) => picked.has(s.name))
    if (!chosen.length) return
    setError(undefined)
    ;(async () => {
      const failed: string[] = []
      for (const s of chosen) {
        setBusy(s.name)
        try {
          await call("plugin.add", s.source, {})
        } catch (err) {
          failed.push(`${s.title}: ${String((err as Error)?.message ?? err)}`)
          continue
        }
        // Its option is declared once it is installed. The terminal is its default already.
        if (s.name === "agents" && agents !== "terminal") await setOption(AGENTS_PREFER, agents).catch((err) => failed.push(`Open agents in: ${String(err?.message ?? err)}`))
      }
      const block = newTabBlock(await reload())
      if (block) await setOption("new_tab", block).catch((err) => failed.push(`New tabs: ${String(err?.message ?? err)}`))
      if (failed.length) setError(failed.join("\n"))
      else answer()
    })().finally(() => setBusy(undefined))
  }, [busy, offered, picked, reload, agents])
  return { offered, picked, toggle, busy, error, install, agents, setAgents }
}
