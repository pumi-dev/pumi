import type { MutationHost, MutationRenderer } from "./host.js"

export type MutationValue = number | string | boolean | object | null
export type MutationTuple = [string, ...MutationValue[]]

let appliedBatches = 0
let appliedOps = 0
let applyMs = 0

/**
 * How many batches of mutations have reached a renderer, across every queue.
 * It moves whenever the tree changes, which makes it the signal that an app is
 * still working: `collectGarbageWhenIdle` waits for it to hold still.
 */
export function appliedBatchCount(): number {
  return appliedBatches
}

/** Batches, mutations and ms spent in `applyBatch` so far, across every queue. */
export function appliedMutationStats(): { batches: number; ops: number; ms: number } {
  return { batches: appliedBatches, ops: appliedOps, ms: applyMs }
}

/** Atomic mutation queue shared by all framework adapters. */
export interface MutationQueue extends MutationRenderer {
  readonly pending: number
}

export function createMutationQueue(
  renderer: MutationHost,
  onDestroyed: (ids: readonly number[]) => void = () => {},
  scheduleFlush?: (flush: () => void) => void
): MutationQueue {
  let queue: MutationTuple[] = []
  let flushScheduled = false
  // A renderer on another thread (`bridge-client.ts`) applies a batch after `applyBatch` returns,
  // and says what it destroyed later.
  ;(renderer as { setDestroyedListener?: (listener: (ids: readonly number[]) => void) => void }).setDestroyedListener?.(onDestroyed)

  const enqueue = (mutation: MutationTuple) => {
    queue.push(mutation)
    if (!scheduleFlush || flushScheduled) return
    flushScheduled = true
    scheduleFlush(() => {
      if (!flushScheduled) return
      flushScheduled = false
      mutations.flushMutations()
    })
  }

  const mutations: MutationQueue = {
    get pending() {
      return queue.length
    },
    createElement(id, elementType) {
      enqueue(["createElement", id, elementType])
    },
    destroyElement(id) {
      enqueue(["destroyElement", id])
      return []
    },
    appendChild(parentId, childId) {
      enqueue(["appendChild", parentId, childId])
    },
    insertBefore(parentId, childId, beforeId) {
      enqueue(["insertBefore", parentId, childId, beforeId])
    },
    setStyle(id, style) {
      enqueue(["setStyle", id, style])
    },
    setText(id, content) {
      enqueue(["setText", id, content])
    },
    setEventListener(id, eventType, hasHandler) {
      enqueue(["setEventListener", id, eventType, hasHandler])
    },
    setRoot(id) {
      enqueue(["setRoot", id])
    },
    setKeyboardFocusDim(enabled) {
      enqueue(["setKeyboardFocusDim", enabled])
    },
    setCustomProp(id, key, value) {
      enqueue(["setCustomProp", id, key, value])
    },
    flushMutations() {
      flushScheduled = false
      if (queue.length === 0) return
      appliedBatches++
      appliedOps += queue.length
      const started = performance.now()
      const destroyed = renderer.applyBatch(JSON.stringify(queue))
      applyMs += performance.now() - started
      onDestroyed(destroyed)
      queue = []
    },
  }
  return mutations
}

export const EVENT_PROPS = [
  ["onToggleFile", "toggleFile"],
  ["onShowMore", "showMore"],
  ["onLineClick", "lineClick"],
  ["onLinkClick", "linkClick"],
  ["onVisibleRange", "visibleRange"],
  ["onHighlight", "highlight"],
  ["onMotionComplete", "motionComplete"],
  ["onMotionThrottled", "motionThrottled"],
  ["onChange", "change"],
  ["onSubmit", "submit"],
  ["onSelect", "select"],
  ["onClick", "click"],
  ["onDoubleClick", "doubleClick"],
  ["onContextMenu", "contextMenu"],
  ["onAuxClick", "auxClick"],
  ["onMouseDown", "mouseDown"],
  ["onMouseUp", "mouseUp"],
  ["onMouseEnter", "mouseEnter"],
  ["onMouseLeave", "mouseLeave"],
  ["onMouseMove", "mouseMove"],
  ["onMouseDownOutside", "mouseDownOutside"],
  ["onKeyDown", "keyDown"],
  ["onKeyUp", "keyUp"],
  ["onFocus", "focus"],
  ["onBlur", "blur"],
  ["onScroll", "scroll"],
  ["onLayout", "layout"],
  ["onScrollChange", "scrollChange"],
  ["onFileDrop", "fileDrop"],
] as const

export const EVENT_PROP_NAMES: ReadonlySet<string> = new Set(
  EVENT_PROPS.map(([name]) => name)
)

/**
 * Event type for a handler prop. Adapters walk an element's own props and look
 * each one up here, which beats testing every entry of `EVENT_PROPS` against an
 * element that has two or three props.
 */
export const EVENT_TYPE_BY_PROP: ReadonlyMap<string, string> = new Map(EVENT_PROPS)

export const RESERVED_PROPS: ReadonlySet<string> = new Set([
  "style",
  "className",
  "children",
  "key",
  "ref",
])

export const BUILT_IN_TYPES: ReadonlySet<string> = new Set(["div", "text"])

export const UNIVERSAL_PROPS: ReadonlySet<string> = new Set([
  "autoFocus",
  "tabIndex",
  "motion",
  "cache",
  "compartment",
  "testId",
  "title",
  "highlight",
  "textFlow",
  "role",
  "aria-label",
  "aria-description",
  "aria-id",
  "aria-expanded",
  "aria-selected",
  "aria-valuetext",
  "aria-level",
])

export function isReservedProp(name: string): boolean {
  return RESERVED_PROPS.has(name) || EVENT_PROP_NAMES.has(name)
}

/**
 * Structural equality for the JSON values a mutation carries: styles and
 * custom props. A key whose value is `undefined` counts as absent, because
 * `JSON.stringify` drops it, so `{ color: undefined }` and `{}` reach Rust as
 * the same payload and must compare equal here.
 *
 * Only plain objects, arrays and primitives are compared. A function or class
 * instance is equal to itself and nothing else.
 */
export function sameMutationValue(left: unknown, right: unknown): boolean {
  if (left === right) return true
  // NaN serializes to null on both sides, and -0 to 0, so neither is a change.
  if (typeof left === "number" && typeof right === "number") return left !== left && right !== right
  if (typeof left !== "object" || typeof right !== "object" || left === null || right === null) {
    return false
  }
  if (Array.isArray(left)) {
    if (!Array.isArray(right) || left.length !== right.length) return false
    for (let index = 0; index < left.length; index++) {
      if (!sameMutationValue(left[index], right[index])) return false
    }
    return true
  }
  if (Array.isArray(right)) return false
  const leftProto = Object.getPrototypeOf(left)
  if (leftProto !== Object.prototype && leftProto !== null) return false
  if (Object.getPrototypeOf(right) !== leftProto) return false
  const leftRecord = left as Record<string, unknown>
  const rightRecord = right as Record<string, unknown>
  let defined = 0
  for (const key in leftRecord) {
    const value = leftRecord[key]
    if (value === undefined) continue
    defined++
    if (!sameMutationValue(value, rightRecord[key])) return false
  }
  let rightDefined = 0
  for (const key in rightRecord) {
    if (rightRecord[key] !== undefined) rightDefined++
  }
  return defined === rightDefined
}

export function serializeCustomProp(
  value: object | string | number | boolean | null | undefined
): string | object | number | boolean | null {
  if (value === undefined || typeof value === "function") return null
  return value
}
