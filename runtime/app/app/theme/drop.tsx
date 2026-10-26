// A drop target that knows its own box: what the default theme's drop zones are made of, so a drop
// can say which part of its target it landed on (a side of a pane, the top of a row).
import { useCallback, useRef } from "react"
import { useGpuix } from "../../react/hooks/use-gpuix.ts"
import { useDroppable } from "../../sdk/drag.ts"

export type Bounds = { x: number; y: number; width: number; height: number }

/** Where a point is in a box, in shares of its width and height (0 to 1, from the left and top). */
export function within(at: { x: number; y: number }, box: Bounds | undefined): { fx: number; fy: number } {
  if (!box || box.width <= 0 || box.height <= 0) return { fx: 0.5, fy: 0.5 }
  const clamp = (n: number) => Math.min(1, Math.max(0, n))
  return { fx: clamp((at.x - box.x) / box.width), fy: clamp((at.y - box.y) / box.height) }
}

/**
 * A drop target (give it `ref`) that takes what `accept` says, and is told where it landed and
 * its own box. `isOver` while an accepted drag is over it; `drag` is the drag in flight.
 * `priority`: as `useDroppable`'s, against the other targets under the mouse.
 */
export function useDropBox<T>(accept: (data: unknown) => data is T, onDrop: (it: T, at: { x: number; y: number }, box?: Bounds) => void, priority?: number) {
  const { renderer } = useGpuix()
  const id = useRef<number | undefined>(undefined)
  const box = (): Bounds | undefined => (id.current === undefined ? undefined : (renderer?.getElementBounds?.(id.current) ?? undefined))
  const drop = useDroppable<T>({ accept, onDrop: (it, at) => onDrop(it, at, box()), priority })
  const dropRef = drop.ref
  const ref = useCallback(
    (instance: { id: number } | null) => {
      id.current = instance?.id
      dropRef(instance as never)
    },
    [dropRef],
  )
  return { ref, isOver: drop.isOver, drag: drop.drag, box }
}
