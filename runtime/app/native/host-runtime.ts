import type { EventPayload } from "./addon"
import type {
  EdgeInsets,
  NativeRenderer,
  NativeWindowInsets,
  SelectionHost,
  WindowInsetsHost,
  WindowSizeHost,
} from "./host.js"

export interface WindowSize {
  width: number
  height: number
}

export interface ObserverOptions {
  /** Poll interval in milliseconds. Defaults to 100. Set false for one read. */
  intervalMs?: number | false
}

export interface WindowInsets extends NativeWindowInsets {
  keyboardTop: number
  keyboardVisible: boolean
  visibleHeight: number
}

const DEFAULT_WINDOW_SIZE: WindowSize = { width: 800, height: 600 }
const ZERO_EDGES: EdgeInsets = { top: 0, right: 0, bottom: 0, left: 0 }

export function readWindowSize(
  renderer: WindowSizeHost | NativeRenderer | null
): WindowSize {
  try {
    const size = renderer?.getWindowSize?.()
    if (size && size.width > 0 && size.height > 0) return { ...size }
  } catch {
    // The platform window can still be opening.
  }
  return { ...DEFAULT_WINDOW_SIZE }
}

export function readWindowInsets(
  renderer: (WindowSizeHost & WindowInsetsHost) | NativeRenderer | null
): WindowInsets {
  let size = DEFAULT_WINDOW_SIZE
  let insets: NativeWindowInsets = {
    safeArea: ZERO_EDGES,
    ime: ZERO_EDGES,
    effective: ZERO_EDGES,
  }
  try {
    size = renderer?.getWindowSize?.() ?? size
    insets = renderer?.getWindowInsets?.() ?? insets
  } catch {
    // The platform window can still be opening.
  }
  return {
    safeArea: { ...insets.safeArea },
    ime: { ...insets.ime },
    effective: { ...insets.effective },
    keyboardTop: size.height - insets.ime.bottom,
    keyboardVisible: insets.ime.bottom > 0,
    visibleHeight: size.height - insets.effective.top - insets.effective.bottom,
  }
}

function sameWindowSize(left: WindowSize, right: WindowSize): boolean {
  return left.width === right.width && left.height === right.height
}

function sameEdges(left: EdgeInsets, right: EdgeInsets): boolean {
  return left.top === right.top && left.right === right.right &&
    left.bottom === right.bottom && left.left === right.left
}

function sameWindowInsets(left: WindowInsets, right: WindowInsets): boolean {
  return left.keyboardTop === right.keyboardTop &&
    left.keyboardVisible === right.keyboardVisible &&
    left.visibleHeight === right.visibleHeight &&
    sameEdges(left.safeArea, right.safeArea) &&
    sameEdges(left.ime, right.ime) &&
    sameEdges(left.effective, right.effective)
}

function observe<Value>(
  read: () => Value,
  same: (left: Value, right: Value) => boolean,
  callback: (value: Value) => void,
  options: ObserverOptions
): () => void {
  let current = read()
  callback(current)
  if (options.intervalMs === false) return () => {}
  const timer = setInterval(() => {
    const next = read()
    if (same(current, next)) return
    current = next
    callback(next)
  }, Math.max(16, options.intervalMs ?? 100))
  return () => clearInterval(timer)
}

export function observeWindowSize(
  renderer: WindowSizeHost | NativeRenderer | null,
  callback: (size: WindowSize) => void,
  options: ObserverOptions = {}
): () => void {
  return observe(() => readWindowSize(renderer), sameWindowSize, callback, options)
}

export function observeWindowInsets(
  renderer: (WindowSizeHost & WindowInsetsHost) | NativeRenderer | null,
  callback: (insets: WindowInsets) => void,
  options: ObserverOptions = {}
): () => void {
  return observe(() => readWindowInsets(renderer), sameWindowInsets, callback, options)
}

export function observeSelectedText(
  renderer: SelectionHost,
  subscribe: (listener: (event: EventPayload) => void) => () => void,
  callback: (text: string | null) => void
): () => void {
  const stop = subscribe((event) => callback(event.value ?? null))
  try {
    callback(renderer.getSelectedText() ?? null)
  } catch (error) {
    stop()
    throw error
  }
  return stop
}
