import { GpuixRenderer } from "./addon"
import type { EventPayload } from "./addon"
import { createRendererState } from "./renderer-state.js"

const DEFAULT_FRAME_MS = 8
/** How long to wait for a frame wake before pumping anyway, when the addon does not say
 *  (`frameWakeFallbackMs`: about a frame while a window is key, so a lost wake costs one frame). */
const FRAME_WAKE_FALLBACK_MS = 100

export interface FrameLoop {
  stop(): void
}

export interface CreateNativeRendererOptions {
  onEvent?: (event: EventPayload) => void
  onError?: (error: unknown) => void
  /** The window's own events: `windowActivated` (`hovered`: key now), `windowClosed`, and
   *  `windowHidden` (closing it hid the app: `setHideOnClose`). */
  onWindow?: (event: EventPayload) => void
}

export function createNativeRenderer(
  options: CreateNativeRendererOptions = {}
): GpuixRenderer {
  let renderer!: GpuixRenderer
  renderer = new GpuixRenderer((error, event) => {
    if (error) {
      options.onError?.(error)
      return
    }
    try {
      if (event.eventType === "windowActivated" || event.eventType === "windowClosed" || event.eventType === "windowHidden") return options.onWindow?.(event)
      if (createRendererState(renderer).dispatch(event)) options.onEvent?.(event)
    } catch (dispatchError) {
      options.onError?.(dispatchError)
    }
  })
  return renderer
}

export function startFrameLoop(
  renderer: Pick<GpuixRenderer, "requiresTick" | "tick"> &
    Partial<Pick<GpuixRenderer, "setFrameWakeCallback" | "frameWakeFallbackMs">>,
  options: {
    frameMs?: number
    onTerminated?: () => void
    onError?: (error: unknown) => void
  } = {}
): FrameLoop {
  if (!renderer.requiresTick()) return { stop() {} }
  let timer: ReturnType<typeof setTimeout> | undefined
  let stopped = false
  const stop = () => {
    if (stopped) return
    stopped = true
    if (frameWake) renderer.setFrameWakeCallback?.(null)
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
  }
  const loop = () => {
    if (stopped) return
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
    const started = performance.now()
    let running = true
    try {
      running = renderer.tick()
    } catch (error) {
      options.onError?.(error)
    }
    if (running === false) {
      stop()
      options.onTerminated?.()
      return
    }
    if (stopped) return
    const delay = frameWake
      ? renderer.frameWakeFallbackMs?.() ?? FRAME_WAKE_FALLBACK_MS
      : Math.max(0, (options.frameMs ?? DEFAULT_FRAME_MS) - (performance.now() - started))
    timer = setTimeout(loop, delay)
  }
  const frameWake = options.frameMs === undefined &&
    renderer.setFrameWakeCallback?.(loop) === true
  loop()
  return { stop }
}

