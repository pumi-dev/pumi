// `import … from "pumi"`, for a plugin's `tsc`: both halves of the SDK, as one module. The app
// gives a plugin's `main.ts` the worker's half and its `ui.tsx` the window's half at run time;
// this file is only what the type checker reads. Both halves share the models (`View`, `Tab`,
// `Session`, …) over one cache, so a name they both have (`call`, `workspace`) is the worker's.
/// <reference path="./assets.d.ts" />
export * from "../worker/sdk/index.ts"
export {
  useOption,
  useRequests,
  useRegistry,
  useSession,
  useSessions,
  useTab,
  useTabs,
  useView,
  useViews,
  useWatchable,
  useWindow,
  type OptionScope,
  type OptionSetter,
  type RendererProps,
  type WindowInfo,
} from "../sdk/state.ts"
export { defineTheme, useAppearance, useMetrics, usePalette, useTheme, type Appearance, type DockSpec, type IconButtonProps, type Metrics, type TabIconProps, type TerminalPalette, type ThemeSpec, type Tokens } from "../sdk/theme.ts"
export { usePlugin } from "../sdk/contributions.ts"
export * from "../sdk/settings/index.ts"
export { haptic, type HapticPattern } from "../sdk/haptic.ts"
export { keyGlyphs } from "../sdk/actions.ts"
export { openSettings } from "../app/settings/open.ts"
export { useFont, type Font } from "../sdk/font.ts"
export { useChannel, useChannelListener } from "../sdk/channels.ts"
export { useEvent, type UseEventOptions, type WindowEvent } from "../sdk/events.ts"
export { useWebview, webview, webviewRun, webviewSnapshot, WebView, type WebState, type WebViewProps } from "../sdk/webview.tsx"
export { Icon, IconButton, IconImage, TabIcon, type IconProps } from "../sdk/elements.tsx"
export { ICON, iconSource, showsStatus, useTabIcon, useTabStatus, type IconFrom, type IconSource, type TabIconInfo } from "../sdk/icons.ts"
// The renderer's own kit, for a theme or a component: native motion (keyframes, repeats, springs),
// presence, portals, and the headless select to style.
export {
  createPortal,
  motion,
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
  type MotionDivProps,
} from "../react/index.ts"
/** What an element's `ref` holds: its `focus()`, `scrollTo()` and `scrollIntoView()`, a list's `scrollToItem()`. */
export type { PublicInstance as ElementRef } from "../react/index.ts"
export type {
  BackgroundImageLayer,
  BackgroundPosition,
  BackgroundRepeat,
  BackgroundSize,
  ConicGradientBackground,
  GradientBackground,
  KeyEvent,
  LinearGradientBackground,
  MotionEase,
  MotionProps,
  MotionStyle,
  MotionTransition,
  MotionValue,
  RadialGradientBackground,
  SpriteSheet,
  StateStyleDesc,
  StyleDesc,
} from "../native/host.ts"
export type { EventPayload } from "../native/addon"
export {
  useDragState,
  useDraggable,
  useDroppable,
  useHover,
  useInterval,
  useNow,
  type DraggableOptions,
  type Drag,
  type DroppableOptions,
} from "../sdk/kit.tsx"


/** The app's JSX: its intrinsics (`div`, `text`, `icon`, `terminal`, …) and React's runtime. */
export {}
