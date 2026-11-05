// What the app says when a new tab can't open: pumi has no block of its own for one, so while no
// plugin installed starts one (Launchpad, which ships with pumi, is off and nothing else is
// installed), `tab.open(null, …)` is refused (`pumi-host`'s `NO_NEW_TAB`). The window explains it
// once at a time and offers Settings.
import { confirm } from "../sdk/dialogs.ts"
import { openSettings } from "./settings/open.ts"

/** The words `tab.open`'s refusal starts with (`crates/pumi-host/src/api/tabs.rs`). */
export const NO_NEW_TAB = "no plugin opens new tabs"

/** Whether `err` is that refusal. */
export function isNoNewTab(err: unknown): boolean {
  const message = typeof err === "string" ? err : String((err as { message?: unknown })?.message ?? err ?? "")
  return message.includes(NO_NEW_TAB)
}

let asking = false

/** Tells the user, when `err` is that refusal, and answers whether it was. */
export function explainNoNewTab(err: unknown): boolean {
  if (!isNoNewTab(err)) return false
  if (asking) return true
  asking = true
  confirm({
    title: "Nothing opens new tabs",
    detail: "Launchpad ships with pumi: turn it back on in Settings ▸ Plugins, or install Terminal, Agents or Browser. Settings ▸ General picks what new tabs open.",
    confirm: "Open Settings",
    cancel: "Close",
  })
    .then((go) => go && openSettings("general", "new_tab"))
    .finally(() => (asking = false))
  return true
}
