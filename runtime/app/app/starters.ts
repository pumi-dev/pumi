// The plugins the start screen offers to install: pumi itself runs terminals, pages and agents'
// sessions, but a shell in a tab, an agent's tab and a browser tab are plugins (`pumi-dev/plugins`).
// Each is offered until a plugin of its name is installed. The id is the plugin's id once
// installed (`pumi_dev/terminal`): `plugin.add` derives it from the repository.
import { useCallback, useEffect, useState } from "react"
import { subscribeEvent } from "../sdk/events.ts"
import { call } from "./engine.ts"

export interface Starter {
  /** Its folder's name, which an installed plugin's `spec.name` is. */
  name: string
  title: string
  detail: string
  icon: string
  /** What `plugin.add` is given. */
  source: string
}

export const STARTERS: Starter[] = [
  { name: "terminal", title: "Terminal", detail: "a shell in a tab", icon: "terminal", source: "pumi-dev/plugins/terminal" },
  { name: "agents", title: "Agents", detail: "Claude Code, Codex, pi…", icon: "sparkle", source: "pumi-dev/plugins/agents" },
  { name: "browser", title: "Browser", detail: "a web page in a tab", icon: "globe", source: "pumi-dev/plugins/browser" },
]

/** The starters no installed plugin (`registry.list("plugin")`'s) is named for. */
export function notInstalled(installed: { spec?: { name?: unknown } }[], starters = STARTERS): Starter[] {
  const names = new Set(installed.map((p) => p?.spec?.name))
  return starters.filter((s) => !names.has(s.name))
}

/**
 * The starters to offer, read again when a plugin is installed or removed, and `install(s)`.
 * `undefined` until the list is read, so nothing is offered that is there already.
 */
export function useStarters(): { offered?: Starter[]; busy?: string; error?: string; install(s: Starter): void } {
  const [offered, setOffered] = useState<Starter[]>()
  const [busy, setBusy] = useState<string>()
  const [error, setError] = useState<string>()
  const reload = useCallback(() => {
    call("registry.list", "plugin", {}).then(
      (list: unknown[]) => setOffered(notInstalled((list ?? []) as { spec?: { name?: unknown } }[])),
      () => setOffered([]),
    )
  }, [])
  useEffect(() => {
    reload()
    return subscribeEvent("registry.changed", undefined, reload)
  }, [reload])
  const install = useCallback(
    (s: Starter) => {
      if (busy) return
      setBusy(s.name)
      setError(undefined)
      call("plugin.add", s.source, {})
        .then(reload, (err: Error) => setError(`${s.title}: ${String(err?.message ?? err)}`))
        .finally(() => setBusy(undefined))
    },
    [busy, reload],
  )
  return { offered, busy, error, install }
}
