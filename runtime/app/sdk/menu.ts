// A menu of a slot's actions where the mouse is, drawn by the theme's `Menu` part: `useMenu`
// gives what opens it (a right press, or any press) and the menu to draw while it is open.
import { createElement, useEffect, useMemo, useState } from "react"
import { useSlot } from "./contributions.ts"
import { useComponents } from "./theme.ts"

/**
 * A menu of `slots`' actions and components, for `args` (`{ tab }`): the first slot's items, then
 * the next's, a rule between. Put `onAuxClick={open}` on what a right press opens it on (or call
 * `show(e)` from a button), and draw `menu` beside it; `shown` is whether it is open.
 *
 * ```tsx
 * const { open, menu } = useMenu("tab.menu", { tab })
 * return <div onAuxClick={open}>…{menu}</div>
 * ```
 */
export function useMenu(slots: string | string[], args: Record<string, unknown>) {
  const C = useComponents()
  const [first, ...rest] = typeof slots === "string" ? [slots] : slots
  const [at, setAt] = useState<{ x: number; y: number } | undefined>()
  // The actions are made only while it is open: a list of rows with a menu each costs nothing
  // until one opens. A fixed number of hooks: at most three slots in one menu.
  const given = at ? args : undefined
  const a = useSlot(first ?? "", given)
  const b = useSlot(rest[0] ?? "", rest[0] ? given : undefined)
  const c = useSlot(rest[1] ?? "", rest[1] ? given : undefined)
  const used = [a, rest[0] ? b : undefined, rest[1] ? c : undefined].filter((s) => !!s)
  // Each slot's items a group of their own, after the groups they name themselves.
  const actions = useMemo(() => used.flatMap((s, i) => s.actions.map((x) => (i === 0 ? x : { ...x, group: `${i}:${x.group ?? ""}` }))), [a.actions, b.actions, c.actions])
  const components = useMemo(() => used.flatMap((s) => s.components), [a.components, b.components, c.components])
  const show = (e?: { x?: number; y?: number }) => setAt({ x: e?.x ?? 0, y: e?.y ?? 0 })
  const open = (e?: { x?: number; y?: number; isRightClick?: boolean }) => e?.isRightClick && show(e)
  const close = () => setAt(undefined)
  const empty = !actions.length && !components.length
  // Opened on something with nothing to offer: closed again, never drawn.
  useEffect(() => {
    if (at && empty) setAt(undefined)
  }, [at, empty])
  const menu = at && !empty ? createElement(C.Menu, { at, actions, components, args, close }) : null
  return { open, show, close, menu, shown: !!at }
}
