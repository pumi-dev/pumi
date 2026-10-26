// Dragging and dropping, as plain React over the renderer's events: what `kit.tsx` gives a theme,
// apart from the engine, so the default theme's tiles can use it without starting one.
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react"
import type { EventPayload } from "../native/addon"
import type { PublicInstance } from "../react/types/host.ts"
import { useGpuix } from "../react/hooks/use-gpuix.ts"

/** A drag in flight: what is carried, and where the mouse is (window px). */
export interface Drag<T = unknown> {
  data: T
  x: number
  y: number
  /** Where it started. */
  startX: number
  startY: number
  /** The drop target under the mouse, if one takes it. */
  over?: number
}

interface Target {
  accept(data: unknown): boolean
  drop(data: unknown, at: { x: number; y: number }): void
  priority: number
}

let current: Drag | null = null
const listeners = new Set<() => void>()
const targets = new Map<number, Target>()

function setDrag(next: Drag | null) {
  current = next
  for (const listener of listeners) listener()
}

function subscribeDrag(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** The drag in flight, or null, read when it is asked (an event's handler): it draws nothing again as the drag moves. */
export function dragging(): Drag | null {
  return current
}

/** The drag in flight, or null: for a theme's drag ghost (draw it with `pointerEvents: "none"`). */
export function useDragState<T = unknown>(): Drag<T> | null {
  return useSyncExternalStore(subscribeDrag, () => current) as Drag<T> | null
}

export interface DraggableOptions<T> {
  /** How far (px) the mouse moves with the button down before it is a drag. 4 by default. */
  threshold?: number
  disabled?: boolean
  onDragStart?(drag: Drag<T>): void
  /** `dropped` says whether a target took it. */
  onDragEnd?(drag: Drag<T>, dropped: boolean): void
}

/**
 * Makes an element draggable: spread `props` on it. While it drags, `useDragState()` says where;
 * a `useDroppable` target under the mouse that accepts `data` takes it on release.
 * `wasDragged()` is true in the click that ends a drag, so a click handler can ignore it.
 */
export function useDraggable<T>(data: T, options: DraggableOptions<T> = {}) {
  const { renderer } = useGpuix()
  const pressed = useRef<{ x: number; y: number } | null>(null)
  const dragging = useRef(false)
  const dragged = useRef(false)
  const latest = useRef({ data, options })
  latest.current = { data, options }

  // The target under the mouse that takes it: of the highest priority, then the topmost.
  const targetAt = (x: number, y: number): number | undefined => {
    const ids = renderer?.getElementsAt?.(x, y) ?? []
    let best: { id: number; priority: number } | undefined
    for (const id of ids) {
      const target = targets.get(id)
      if (!target?.accept(latest.current.data)) continue
      if (!best || target.priority > best.priority) best = { id, priority: target.priority }
    }
    return best?.id
  }

  const onMouseDown = useCallback((event: EventPayload) => {
    if (latest.current.options.disabled || (event.button ?? 0) !== 0) return
    pressed.current = { x: event.x ?? 0, y: event.y ?? 0 }
    dragged.current = false
  }, [])

  const onMouseMove = useCallback(
    (event: EventPayload) => {
      const start = pressed.current
      if (!start) return
      // No button down any more: the release was missed (it ended outside). A drag in flight
      // ends without a drop.
      if (event.pressedButton === undefined || event.pressedButton === null) {
        pressed.current = null
        if (dragging.current) {
          dragging.current = false
          const drag = current as Drag<T>
          setDrag(null)
          latest.current.options.onDragEnd?.(drag, false)
        }
        return
      }
      if (event.pressedButton !== 0) return
      const x = event.x ?? 0
      const y = event.y ?? 0
      if (!dragging.current) {
        const threshold = latest.current.options.threshold ?? 4
        if (Math.hypot(x - start.x, y - start.y) < threshold) return
        dragging.current = true
        dragged.current = true
        const drag: Drag<T> = { data: latest.current.data, x, y, startX: start.x, startY: start.y }
        setDrag(drag)
        latest.current.options.onDragStart?.(drag)
      }
      setDrag({ ...(current as Drag), x, y, over: targetAt(x, y) })
    },
    [renderer],
  )

  const onMouseUp = useCallback(
    (event: EventPayload) => {
      pressed.current = null
      if (!dragging.current) return
      dragging.current = false
      const drag = current as Drag<T>
      const x = event.x ?? drag.x
      const y = event.y ?? drag.y
      const over = targetAt(x, y)
      if (over !== undefined) targets.get(over)?.drop(drag.data, { x, y })
      setDrag(null)
      latest.current.options.onDragEnd?.({ ...drag, x, y, over }, over !== undefined)
    },
    [renderer],
  )

  const wasDragged = useCallback(() => dragged.current, [])
  const isDragging = useSyncExternalStore(subscribeDrag, () => current !== null && dragging.current)
  return { props: { onMouseDown, onMouseMove, onMouseUp }, dragging: isDragging, wasDragged }
}

export interface DroppableOptions<T> {
  /** Whether it takes this drag. Everything by default. */
  accept?(data: unknown): data is T
  onDrop(data: T, at: { x: number; y: number }): void
  /**
   * Of two targets under the mouse that both take a drag, the higher one gets it (0 by default);
   * of the same, the topmost. A target over a whole area whose parts take drops of their own
   * (a pane's split zone, over its tab strip) gives itself less.
   */
  priority?: number
}

/** A drop target: give it `ref`. `isOver` while a drag it accepts is over it. */
export function useDroppable<T = unknown>(options: DroppableOptions<T>) {
  const [id, setId] = useState<number>()
  const latest = useRef(options)
  latest.current = options
  const ref = useCallback((instance: PublicInstance | null) => setId(instance?.id), [])
  useEffect(() => {
    if (id === undefined) return
    targets.set(id, {
      accept: (data) => latest.current.accept?.(data) ?? true,
      drop: (data, at) => latest.current.onDrop(data as T, at),
      get priority() {
        return latest.current.priority ?? 0
      },
    })
    return () => {
      targets.delete(id)
    }
  }, [id])
  const drag = useDragState()
  return { ref, isOver: drag !== null && id !== undefined && drag.over === id, drag }
}
