import { useEffect, useState } from "react"
import {
  observeWindowInsets,
  observeWindowSize,
  readWindowInsets,
  readWindowSize,
} from "../../native/host"
import type {
  ObserverOptions,
  WindowInsets,
  WindowSize,
} from "../../native/host"
import { useGpuix } from "./use-gpuix.js"

export interface WindowSizeOptions extends ObserverOptions {}
export interface WindowInsetsOptions extends ObserverOptions {}
export type { WindowInsets, WindowSize }

/** The current window size, sampled every 100ms by default. */
export function useWindowSize(options: WindowSizeOptions = {}): WindowSize {
  const { renderer } = useGpuix()
  const [size, setSize] = useState(() => readWindowSize(renderer))
  const intervalMs = options.intervalMs ?? 100
  useEffect(
    () => observeWindowSize(renderer, setSize, { intervalMs }),
    [renderer, intervalMs]
  )
  return size
}

/** Get safe-area and keyboard geometry, sampled every 100ms by default. */
export function useWindowInsets(
  options: WindowInsetsOptions = {}
): WindowInsets {
  const { renderer } = useGpuix()
  const [insets, setInsets] = useState(() => readWindowInsets(renderer))
  const intervalMs = options.intervalMs ?? 100
  useEffect(
    () => observeWindowInsets(renderer, setInsets, { intervalMs }),
    [renderer, intervalMs]
  )
  return insets
}
