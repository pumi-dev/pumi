// The plugins the start screen offers on the first run: pumi itself runs terminals, pages and
// agents' sessions, but every tab is a plugin's block (`pumi-dev/plugins`), so a fresh install has
// nothing to open until some are installed. Each is ticked to start with; the user unticks what
// they don't want. Once they install or say "not now", the start screen doesn't offer them again
// (`OFFERED`, kept in window.json: the window's own, not a setting).
//
// After installing, new tabs open the first of them that is installed, in this order: what a new
// tab holds is the `new_tab` option, which pumi leaves empty.
import { useCallback, useEffect, useState } from "react"
import { subscribeEvent } from "../sdk/events.ts"
import { call } from "./engine.ts"
import { getLocal, setLocal, useLocal } from "./local.ts"
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

/** window.json's key: whether the start screen has offered the starters and been answered. */
export const OFFERED = "start.offered"

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
  /** What to offer: the starters not installed, while the first run hasn't been answered. `undefined` until read. */
  offered?: Starter[]
  /** The names ticked. */
  picked: Set<string>
  toggle(name: string): void
  /** The one being installed. */
  busy?: string
  error?: string
  /** Installs the ticked ones, in order, then points `new_tab` at the first installed. */
  install(): void
  /** Not now: the start screen stops offering them. */
  dismiss(): void
}

export function useStarters(): Starters {
  const [answered] = useLocal(OFFERED, false)
  const [missing, setMissing] = useState<Starter[]>()
  const [unticked, setUnticked] = useState<Set<string>>(() => new Set())
  const [busy, setBusy] = useState<string>()
  const [error, setError] = useState<string>()
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
    if (answered) return
    reload()
    return subscribeEvent("registry.changed", undefined, () => void reload())
  }, [answered, reload])
  const offered = answered ? [] : missing
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
        }
      }
      const block = newTabBlock(await reload())
      if (block) await setOption("new_tab", block).catch((err) => failed.push(`New tabs: ${String(err?.message ?? err)}`))
      if (failed.length) setError(failed.join("\n"))
      else if (!getLocal(OFFERED, false)) setLocal(OFFERED, true)
    })().finally(() => setBusy(undefined))
  }, [busy, offered, picked, reload])
  const dismiss = useCallback(() => setLocal(OFFERED, true), [])
  return { offered, picked, toggle, busy, error, install, dismiss }
}
