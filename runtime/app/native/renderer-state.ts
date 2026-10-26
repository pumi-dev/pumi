import type { EventPayload } from "./addon"
import type {
  ElementIdAllocator,
  EventHandlerMap,
  HostEventHandler,
  KeyEvent,
  NativeRenderer,
  PressEvent,
  WindowEventHandler,
  WindowKeyEventHandler,
  WindowKeyEventHandlers,
} from "./host.js"

export interface RendererRootHandlers {
  eventHandlers?: EventHandlerMap
  onWindowKeyDown?: WindowKeyEventHandler
  onWindowKeyUp?: WindowKeyEventHandler
  onSelectionChange?: WindowEventHandler
  onModifiersChanged?: WindowKeyEventHandlers["onModifiersChanged"]
  /** See `WindowKeyEventHandlers.tabNavigation`. Defaults to true. */
  tabNavigation?: boolean
  onEvent?: (event: EventPayload) => void
  schedule?: (dispatch: () => void) => void
}

export interface RendererRootBinding {
  readonly ids: ElementIdAllocator
  readonly eventHandlers: EventHandlerMap
  readonly windowKeyEventId: number
  readonly windowSelectionEventId: number
  detach(): boolean
}

/**
 * One open overlay: a dialog, menu, popover or tooltip. Only the top layer
 * closes on Escape. A layer is above its parent and above every layer that
 * opened before it.
 */
export interface DismissLayer {
  /** The layer this one is nested in, such as the Dialog around a Select. */
  readonly parent?: DismissLayer
  /** Escape reached the window and no handler prevented it. */
  onEscapeKeyDown(event: KeyEvent): void
  /**
   * Element to focus when the layer opens, or null to leave focus. Called
   * only when the layer lands on top, so the innermost of several layers that
   * open together takes focus.
   */
  initialFocus?(): number | null
  /**
   * Element to focus when the layer closes, or null to leave focus.
   * `previous` is the focus from before the layer opened. Not called when the
   * parent layer closed first, because the parent restores for both.
   */
  finalFocus?(previous: number | null): number | null
}

export interface PushLayerOptions {
  /**
   * The focused element from before this layer's elements were committed.
   * Read it before the commit: once committed, an `autoFocus` inside the
   * layer may already have taken focus.
   */
  previousFocus?: number | null
}

interface LayerEntry {
  layer: DismissLayer
  previous: number | null
}

function isAncestor(ancestor: DismissLayer, layer: DismissLayer): boolean {
  for (let current = layer.parent; current; current = current.parent) {
    if (current === ancestor) return true
  }
  return false
}

export interface RendererState {
  readonly ids: ElementIdAllocator
  attach(handlers?: RendererRootHandlers): RendererRootBinding
  dispatch(event: EventPayload): boolean
  current(): RendererRootBinding | undefined
  /** Open a layer. Call the returned function when it closes. */
  pushLayer(layer: DismissLayer, options?: PushLayerOptions): () => void
}

type ActiveRoot = RendererRootBinding & { handlers: RendererRootHandlers }

/** Shared by every event of one GPUI key dispatch. */
interface KeystrokeFlags {
  id: number | undefined
  prevented: boolean
  stopped: boolean
}

const STATE_KEY = Symbol.for("@gpuix/native/renderer-states")

/** The events one mouse press or release reaches each element under it with (`PressEvent`). */
const PRESS_TYPES: ReadonlySet<string> = new Set(["click", "doubleClick", "auxClick", "contextMenu", "mouseDown", "mouseUp"])

/** Register an open overlay on this renderer's layer stack. */
export function pushDismissLayer(
  renderer: NativeRenderer,
  layer: DismissLayer,
  options?: PushLayerOptions
): () => void {
  return createRendererState(renderer).pushLayer(layer, options)
}

function allStates(): WeakMap<NativeRenderer, RendererState> {
  const existing = Reflect.get(globalThis, STATE_KEY) as
    | WeakMap<NativeRenderer, RendererState>
    | undefined
  if (existing) return existing
  const states = new WeakMap<NativeRenderer, RendererState>()
  Reflect.set(globalThis, STATE_KEY, states)
  return states
}

function toKeyEvent(event: EventPayload, flags: KeystrokeFlags): KeyEvent {
  return {
    ...event,
    get defaultPrevented() {
      return flags.prevented
    },
    preventDefault() {
      flags.prevented = true
    },
    stopPropagation() {
      flags.stopped = true
    },
  }
}

function hasCommandModifier(event: KeyEvent): boolean {
  const modifiers = event.modifiers
  return Boolean(modifiers?.ctrl || modifiers?.alt || modifiers?.cmd)
}

// One id space for every window: the addon keys scroll, list and bounds state by element id
// across windows.
const ids = { nextElementId: 0 }

/**
 * Where element ids start. A worker that replaces one that stopped answering draws into the
 * windows the last one left (`native/bridge-host.ts`): its ids start past the last one's.
 */
export function setElementIdBase(base: number) {
  ids.nextElementId = Math.max(ids.nextElementId, base)
}

export function createRendererState(renderer: NativeRenderer): RendererState {
  const existing = allStates().get(renderer)
  if (existing) return existing

  let generation = 0
  let active: ActiveRoot | undefined
  const layers: LayerEntry[] = []
  // Native emits one keystroke's events back to back, in GPUI bubble order,
  // and the window event last. So only the latest keystroke needs flags.
  let keystroke: KeystrokeFlags = { id: undefined, prevented: false, stopped: false }
  // Native emits one press's element events back to back, innermost first, so
  // only the latest press needs to know which of its events were stopped.
  let press: { id: number | undefined; stopped: Set<string> } = { id: undefined, stopped: new Set() }
  const stoppedFor = (pressId: number): Set<string> => {
    if (press.id !== pressId) press = { id: pressId, stopped: new Set() }
    return press.stopped
  }
  const flagsFor = (event: EventPayload): KeystrokeFlags => {
    // A synthetic event without an id never shares flags with another event.
    if (event.keystrokeId == null) return { id: undefined, prevented: false, stopped: false }
    if (keystroke.id !== event.keystrokeId) {
      keystroke = { id: event.keystrokeId, prevented: false, stopped: false }
    }
    return keystroke
  }

  const state: RendererState = {
    ids,
    attach(handlers = {}) {
      if (active) {
        throw new Error(
          "This renderer already drives a mounted GPUIX root. One renderer owns one window, one native root id, and one event map, so a second root would silently take both over. Unmount the first root first."
        )
      }
      generation += 1
      const eventHandlers = handlers.eventHandlers ?? new Map()
      const binding: ActiveRoot = {
        ids,
        handlers,
        eventHandlers,
        windowKeyEventId: generation,
        windowSelectionEventId: generation,
        detach() {
          if (active !== binding) return false
          active = undefined
          return true
        },
      }
      active = binding
      return binding
    },
    dispatch(event) {
      const root = active
      if (!root) return false
      const schedule = (invoke: () => void) => {
        const run = () => {
          invoke()
          root.handlers.onEvent?.(event)
        }
        if (root.handlers.schedule) root.handlers.schedule(run)
        else run()
      }

      if (event.eventType === "windowKeyDown" || event.eventType === "windowKeyUp") {
        if (event.elementId !== root.windowKeyEventId) return false
        const keyDown = event.eventType === "windowKeyDown"
        const handler = keyDown ? root.handlers.onWindowKeyDown : root.handlers.onWindowKeyUp
        const flags = flagsFor(event)
        const keyEvent = toKeyEvent(
          { ...event, elementId: 0, eventType: keyDown ? "keyDown" : "keyUp" },
          flags
        )
        // Default actions run last, after every element handler and the window
        // handler of this keystroke had the chance to call preventDefault().
        const runDefault = () => {
          if (!keyDown || flags.prevented || hasCommandModifier(keyEvent)) return
          if (keyEvent.key === "escape") {
            layers.at(-1)?.layer.onEscapeKeyDown(keyEvent)
          } else if (keyEvent.key === "tab" && root.handlers.tabNavigation !== false) {
            if (keyEvent.modifiers?.shift) renderer.focusPrevious?.()
            else renderer.focusNext?.()
          }
        }
        schedule(() => {
          if (handler && !flags.stopped) handler(keyEvent, renderer)
          runDefault()
        })
        return true
      }
      if (event.eventType === "windowModifiersChanged") {
        if (event.elementId !== root.windowKeyEventId) return false
        const handler = root.handlers.onModifiersChanged
        if (!handler) return false
        const modifiers = event.modifiers ?? { ctrl: false, alt: false, shift: false, cmd: false }
        schedule(() => handler(modifiers))
        return true
      }
      if (event.eventType === "selectionChange") {
        if (event.elementId !== root.windowSelectionEventId) return false
        const handler = root.handlers.onSelectionChange
        if (!handler) return false
        schedule(() => handler({ ...event, elementId: 0 }, renderer))
        return true
      }
      const handler: HostEventHandler | undefined =
        root.eventHandlers.get(event.elementId)?.get(event.eventType)
      if (!handler) return false
      if (event.eventType === "keyDown" || event.eventType === "keyUp") {
        const flags = flagsFor(event)
        const keyEvent = toKeyEvent(event, flags)
        schedule(() => {
          if (!flags.stopped) handler(keyEvent)
        })
        return true
      }
      if (PRESS_TYPES.has(event.eventType)) {
        // A click no mouse made (an a11y press) reaches one element: nothing to stop.
        const stopped = event.pressId == null ? new Set<string>() : stoppedFor(event.pressId)
        const type = event.eventType
        const pressEvent: PressEvent = {
          ...event,
          stopPropagation() {
            stopped.add(type)
          },
        }
        schedule(() => {
          if (!stopped.has(type)) handler(pressEvent)
        })
        return true
      }
      schedule(() => handler(event))
      return true
    },
    current: () => active,
    pushLayer(layer, options = {}) {
      const entry: LayerEntry = {
        layer,
        previous: options.previousFocus ?? renderer.getFocusedElementId?.() ?? null,
      }
      // A framework can open a nested layer before its parent in one commit
      // (React runs child effects first), so nesting decides, not call order.
      const firstChild = layers.findIndex((open) => isAncestor(layer, open.layer))
      if (firstChild < 0) {
        layers.push(entry)
        const target = layer.initialFocus?.() ?? null
        if (target !== null) renderer.focusElement?.(target)
      } else {
        layers.splice(firstChild, 0, entry)
        // The children opened in the same commit and captured the same
        // pre-open focus. Closing one alone must return into this layer.
        const inside = layer.initialFocus?.() ?? null
        for (const child of layers.slice(firstChild + 1)) {
          if (isAncestor(layer, child.layer) && child.previous === entry.previous) {
            child.previous = inside
          }
        }
      }
      return () => {
        const index = layers.indexOf(entry)
        if (index < 0) return
        layers.splice(index, 1)
        // A parent that closed first already restored focus for its subtree.
        if (layer.parent && !layers.some((open) => open.layer === layer.parent)) return
        const target = layer.finalFocus?.(entry.previous) ?? null
        if (target !== null) renderer.focusElement?.(target)
      }
    },
  }
  allStates().set(renderer, state)
  return state
}

export function registerEventHandler(
  eventHandlers: EventHandlerMap,
  elementId: number,
  eventType: string,
  handler: HostEventHandler
): void {
  const handlers = eventHandlers.get(elementId) ?? new Map()
  handlers.set(eventType, handler)
  eventHandlers.set(elementId, handlers)
}

export function unregisterEventHandler(
  eventHandlers: EventHandlerMap,
  elementId: number,
  eventType: string
): void {
  const handlers = eventHandlers.get(elementId)
  if (!handlers) return
  handlers.delete(eventType)
  if (handlers.size === 0) eventHandlers.delete(elementId)
}

export function unregisterEventHandlers(
  eventHandlers: EventHandlerMap,
  elementId: number
): void {
  eventHandlers.delete(elementId)
}

/** Anything a framework hands out for a mounted host element. */
export interface FocusableNode {
  readonly id: number
}

/**
 * Where an overlay moves focus when it opens or closes, like Base UI's
 * `initialFocus` / `finalFocus`:
 *
 * - `true` or unset: the overlay's default target
 * - `false`: focus does not move
 * - an element or a ref to one: that element
 * - a function: returns one of the above; `null` means the default
 */
type FocusTargetValue<Node extends FocusableNode> =
  | boolean
  | Node
  | { readonly current: Node | null }

export type FocusTarget<Node extends FocusableNode = FocusableNode> =
  | FocusTargetValue<Node>
  | (() => FocusTargetValue<Node> | null | undefined)

/** Resolve a `FocusTarget` to an element id, or null when focus must stay. */
export function resolveFocusTarget<Node extends FocusableNode>(
  target: FocusTarget<Node> | undefined,
  fallback: () => number | null
): number | null {
  const value = typeof target === "function" ? target() : target
  if (value === undefined && typeof target === "function") return null
  if (value === undefined || value === null || value === true) return fallback()
  if (value === false) return null
  if ("current" in value) return value.current?.id ?? fallback()
  return value.id
}
