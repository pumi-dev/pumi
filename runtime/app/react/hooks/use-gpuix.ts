import { useContext, createContext } from "react"
import type { NativeRenderer } from "../types/host.js"

export interface GpuixContextValue {
  renderer: NativeRenderer | null
}

export const GpuixContext = createContext<GpuixContextValue>({
  renderer: null,
})

/**
 * Access the GPUIX renderer from within a component
 */
export function useGpuix(): GpuixContextValue {
  return useContext(GpuixContext)
}

/**
 * Access the GPUIX renderer, throwing if not available
 */
export function useGpuixRequired(): NativeRenderer {
  const { renderer } = useGpuix()
  if (!renderer) {
    throw new Error("useGpuixRequired must be used within a GpuixProvider")
  }
  return renderer
}
