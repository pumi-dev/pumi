// Renaming a view or a session in place. A row that can show its name being edited calls
// `useRename(target, label)` and draws the field it gives; the app's Rename action (in the view's
// and the session's menus) starts it there, or asks with a prompt when nothing on screen shows
// that target's name.
import { useEffect, useRef, useState, useSyncExternalStore, type KeyboardEvent } from "react"
import { call, via } from "../app/engine.ts"
import { prompt } from "./dialogs.ts"

/** What can be renamed: a view, or a session. */
export type Renamable = { view: string } | { session: string }

const keyOf = (target: Renamable) => ("view" in target ? `view ${target.view}` : `session ${target.session}`)

/** How many rows show each target's name, so a rename knows whether one can take it in place. */
const shown = new Map<string, number>()
let editing: string | undefined
const listeners = new Set<() => void>()

function setEditing(key: string | undefined) {
  editing = key
  for (const l of listeners) l()
}

/** Gives `target` its new name on its server. */
export function rename(target: Renamable, name: string): Promise<void> {
  return "view" in target
    ? call(via(target.view, "view.update"), target.view, { name })
    : call(via(target.session, "session.update"), target.session, { name })
}

/**
 * Starts renaming `target`: in the row that shows its name, or, when none does, in a prompt over
 * the window. `label` is its name now.
 */
export function startRename(target: Renamable, label: string) {
  const key = keyOf(target)
  if ((shown.get(key) ?? 0) > 0) return setEditing(key)
  const kind = "view" in target ? "View" : "Session"
  prompt({ title: `Rename ${kind}`, value: label, confirm: "Rename" }).then((name) => {
    const given = name?.trim()
    if (given && given !== label) rename(target, given).catch((err) => console.error(String(err)))
  })
}

/** What a field editing a name is given: spread it on an `<input>` and style it as you like. */
export interface RenameInput {
  value: string
  autoFocus: true
  onChange(e: { value?: string }): void
  onSubmit(): void
  onBlur(): void
  onKeyDown(e: KeyboardEvent | any): void
  onClick(e: any): void
  onDoubleClick(e: any): void
}

/**
 * A row's name, editable in place: `start()` (or the app's Rename action, for this target) turns
 * `renaming` on, and `input` is what the row's `<input>` takes. Enter or a click away renames,
 * Escape gives up.
 */
export function useRename(target: Renamable, label: string): { renaming: boolean; start(): void; input: RenameInput | undefined } {
  const key = keyOf(target)
  useEffect(() => {
    shown.set(key, (shown.get(key) ?? 0) + 1)
    return () => {
      const n = (shown.get(key) ?? 1) - 1
      if (n > 0) shown.set(key, n)
      else shown.delete(key)
    }
  }, [key])
  const on = useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => {
        listeners.delete(l)
      }
    },
    () => editing === key,
  )
  const [value, setValue] = useState(label)
  // Enter ends the edit and the field's blur follows: only the first of them renames.
  const live = useRef(false)
  useEffect(() => {
    if (on) {
      live.current = true
      setValue(label)
    }
  }, [on])
  const stop = (e: any) => e?.stopPropagation?.()
  const end = (commit: boolean) => {
    if (!live.current) return
    live.current = false
    setEditing(undefined)
    const name = value.trim()
    if (commit && name && name !== label) rename(target, name).catch((err) => console.error(String(err)))
  }
  return {
    renaming: on,
    start: () => setEditing(key),
    input: on
      ? {
          value,
          autoFocus: true,
          onChange: (e) => setValue(e.value ?? ""),
          onSubmit: () => end(true),
          onBlur: () => end(true),
          onKeyDown: (e) => {
            if (e.key !== "escape") return
            e.preventDefault?.()
            e.stopPropagation?.()
            end(false)
          },
          onClick: stop,
          onDoubleClick: stop,
        }
      : undefined,
  }
}
