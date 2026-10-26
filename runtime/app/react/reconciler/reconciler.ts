import React from "react"
import type { ReactNode } from "react"
import ReactReconciler from "react-reconciler"
import type { OpaqueRoot } from "react-reconciler"
import { ConcurrentRoot } from "react-reconciler/constants.js"
import { createMutationQueue } from "../../native/host"
import { GpuixContext, type GpuixContextValue } from "../hooks/use-gpuix.js"
import type {
  Container,
  NativeRenderer,
  RootEventHandlers,
} from "../types/host.js"
import {
  attachRoot,
  detachRoot,
  idAllocatorFor,
  unregisterEventHandlers,
} from "./event-registry.js"
import { hostConfig } from "./host-config.js"

// Cast to any because @types/react-reconciler is out of date with react-reconciler 0.31.0
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const reconciler = ReactReconciler(hostConfig as any)

/**
 * Register with the React DevTools global hook.
 *
 * This is not only for DevTools, and it is not optional. React Fast Refresh
 * reaches a renderer through the same hook: `react-refresh` patches
 * `hook.inject`, keeps the `scheduleRefresh` and `setRefreshHandler` helpers
 * this call passes in, and drives hot updates through them.
 *
 * Drop this call and there is **no error and no page reload**. Bun still marks
 * the edited module self-accepting and still calls `performReactRefresh()`,
 * which iterates zero mounted roots and schedules nothing. The bundle updates
 * and the painted UI silently stays stale.
 *
 * The hook has to already exist when this module evaluates. Bun's HMR runtime
 * calls `injectIntoGlobalHook(window)` during bundle init, so it does in the
 * dev server, and `injectIntoDevTools()` is a no-op returning `false` in plain
 * Node. Do not test that return value: it ends in `hook.checkDCE ? true : false`
 * and `react-refresh` installs no `checkDCE`, so a working injection still
 * reports `false`.
 */
try {
  // @ts-expect-error the types for `react-reconciler` are not up to date with the library
  reconciler.injectIntoDevTools()
} catch {
  // No DevTools hook in this process.
}

const _r = reconciler as typeof reconciler & {
  flushSyncFromReconciler?: typeof reconciler.flushSync
}
export const flushSync = _r.flushSyncFromReconciler ?? _r.flushSync

export interface Root {
  render: (node: ReactNode) => void
  unmount: () => void
}

export function createRoot(
  renderer: NativeRenderer,
  rootEventHandlers: RootEventHandlers = {}
): Root {
  let container: OpaqueRoot | null = null
  const ids = idAllocatorFor(renderer)
  let gpuixContainer!: Container
  const batchedRenderer = createMutationQueue(renderer, (destroyedIds) => {
    for (const id of destroyedIds) {
      unregisterEventHandlers(gpuixContainer.eventHandlers, id)
    }
  })
  gpuixContainer = {
    nativeRenderer: renderer,
    renderer: batchedRenderer,
    ids,
    eventHandlers: new Map(),
    windowKeyEventHandlers: rootEventHandlers,
    windowKeyEventId: 0,
    windowSelectionEventId: 0,
    onEvent: rootEventHandlers.onEvent,
  }
  attachRoot(renderer, gpuixContainer)
  // Sent on every root, so a remount without the option restores the default.
  batchedRenderer.setKeyboardFocusDim(rootEventHandlers.keyboardFocusDim !== false)
  const { windowKeyEventId, windowSelectionEventId } = gpuixContainer
  try {
    renderer.setWindowKeyEvents?.(
      // Always on: the Tab and Escape defaults run on the window event.
      true,
      Boolean(rootEventHandlers.onKeyUp),
      windowKeyEventId
    )
    renderer.setWindowSelectionChange?.(
      Boolean(rootEventHandlers.onSelectionChange),
      windowSelectionEventId
    )
  } catch (error) {
    detachRoot(renderer, gpuixContainer)
    throw error
  }

  const cleanup = (): void => {
    if (container) {
      // Must be sync. A late unmount destroy()s remounted ids and the window goes black.
      flushSync(() => {
        reconciler.updateContainer(null, container, null, () => {})
      })
      container = null
    }
    if (detachRoot(renderer, gpuixContainer)) {
      renderer.setWindowKeyEvents?.(false, false, windowKeyEventId)
      renderer.setWindowSelectionChange?.(false, windowSelectionEventId)
    }
  }

  const onUncaughtError =
    rootEventHandlers.onUncaughtError ??
    ((error: Error) => {
      console.error(error)
    })

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  container = (reconciler.createContainer as any)(
    gpuixContainer,
    ConcurrentRoot,
    null,
    false,
    null,
    "",
    onUncaughtError,
    console.error,
    console.error,
    null
  )

  // One object for the life of the root. A fresh `{ renderer }` per `render()`
  // call is a context change, which re-renders every `useGpuix()` consumer, and
  // through them busts `memo` below it, on every re-render of the root.
  const contextValue: GpuixContextValue = { renderer }

  return {
    render: (node): void => {
      const activeContainer = container
      if (!activeContainer) {
        throw new Error("Cannot render an unmounted GPUIX root")
      }
      reconciler.updateContainer(
        React.createElement(
          GpuixContext.Provider,
          { value: contextValue },
          node
        ),
        activeContainer,
        null,
        () => {}
      )
    },

    unmount: cleanup,
  }
}
