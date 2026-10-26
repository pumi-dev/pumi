import type { EventPayload } from "../../native/addon"
import {
  createRendererState,
  registerEventHandler,
  unregisterEventHandler,
  unregisterEventHandlers,
} from "../../native/host"
import type {
  Container,
  ElementIdAllocator,
  EventHandlerMap,
  NativeRenderer,
} from "../types/host.js"

const bindings = new WeakMap<Container, { detach(): boolean }>()

export function idAllocatorFor(renderer: NativeRenderer): ElementIdAllocator {
  return createRendererState(renderer).ids
}

export function attachRoot(renderer: NativeRenderer, container: Container): void {
  const binding = createRendererState(renderer).attach({
    eventHandlers: container.eventHandlers,
    onWindowKeyDown: container.windowKeyEventHandlers.onKeyDown,
    onWindowKeyUp: container.windowKeyEventHandlers.onKeyUp,
    onSelectionChange: container.windowKeyEventHandlers.onSelectionChange,
    onModifiersChanged: container.windowKeyEventHandlers.onModifiersChanged,
    tabNavigation: container.windowKeyEventHandlers.tabNavigation,
    onEvent: container.onEvent,
  })
  container.windowKeyEventId = binding.windowKeyEventId
  container.windowSelectionEventId = binding.windowSelectionEventId
  bindings.set(container, binding)
}

export function detachRoot(_renderer: NativeRenderer, container: Container): boolean {
  const binding = bindings.get(container)
  if (!binding) return false
  bindings.delete(container)
  return binding.detach()
}

export function handleGpuixEvent(
  payload: EventPayload,
  renderer: NativeRenderer
): boolean {
  return createRendererState(renderer).dispatch(payload)
}

export {
  registerEventHandler,
  unregisterEventHandler,
  unregisterEventHandlers,
}

export type { EventHandlerMap }
