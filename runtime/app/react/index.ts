// GPUIX React - React bindings for GPUI
export { createRoot, flushSync } from "./reconciler/index.js"
export { createPortal } from "./reconciler/portal.js"
export {
  createRenderer,
  installRuntimeErrorHandlers,
  render,
  resetRender,
  startFrameLoop,
} from "./reconciler/renderer.js"
export { GpuixContext, useGpuix, useGpuixRequired } from "./hooks/use-gpuix.js"
export { useWindowInsets, useWindowSize } from "./hooks/use-window-size.js"
export {
  observeSelectedText,
  observeWindowInsets,
  observeWindowSize,
  pushDismissLayer,
  readWindowInsets,
  readWindowSize,
} from "../native/host"
export type { DismissLayer } from "../native/host"
export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "./components/select.js"
export type {
  SelectContentProps,
  SelectItemData,
  SelectItemProps,
  SelectItemState,
  SelectProps,
  SelectTriggerProps,
  SelectTriggerState,
  SelectValueProps,
} from "./components/select.js"
export { DismissableLayer } from "./components/floating.js"
export type { DismissableLayerProps } from "./components/floating.js"
export { motion, useIsPresent, usePresence } from "./components/index.js"
export type { Root, FrameLoop, RenderOptions } from "./reconciler/renderer.js"
export type {
  WindowInsets,
  WindowInsetsOptions,
  WindowSize,
  WindowSizeOptions,
} from "./hooks/use-window-size.js"

// Re-export types
export type { MotionDivProps } from "./components/index.js"
export type {
  AnchoredProps,
  CursorValue,
  DebugFrameOverlayMode,
  DebugFrameOverlayStats,
  DimensionValue,
  EdgeInsets,
  ElementBounds,
  GpuixTheme,
  HighlightMatch,
  HighlightSpec,
  ImgInstance,
  ImgProps,
  FocusTarget,
  InputProps,
  KeyEvent,
  LinearGradientBackground,
  LinearGradientStop,
  MotionEase,
  MotionProps,
  MotionStyle,
  MotionTransition,
  MutationHost,
  NativeRenderer,
  NativeWindowInsets,
  PathPromptOptions,
  PressEvent,
  Props,
  PublicInstance,
  StyleDesc,
  SvgProps,
  SelectionHost,
  TextareaProps,
  VirtualListProps,
  WindowInsetsHost,
  WindowKeyEventHandler,
  WindowKeyEventHandlers,
  WindowSizeHost,
} from "./types/host.js"
export { handleGpuixEvent } from "./reconciler/event-registry.js"
export {
  applyMacCpuThrottleFromEnv,
  MAC_CPU_THROTTLES,
  readMacCpuThrottle,
} from "../native/cpu-throttle.js"
export type { MacCpuThrottle } from "../native/cpu-throttle.js"
export { collectGarbageWhenIdle } from "../native/idle-gc.js"
export type { IdleGcOptions } from "../native/idle-gc.js"
export type {
  EventPayload,
  EventModifiers,
  WindowOptions,
  WindowSize as NativeWindowSize,
} from "../native/addon"

export { GpuixRenderer } from "../native/addon"
