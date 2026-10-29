// `import … from "pumi"` in a plugin's `ui.tsx`: what the window draws with. Its hooks read the
// same live cache `main.ts` does, filled with `<kind>.get` and kept by the public events; its logic belongs in `main.ts`,
// whose exports `ui.tsx` imports and calls (each call goes to the worker and answers a promise).
// The app's own theme uses more of the app than this; a plugin sees only this.
export {
  call,
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
  workspace,
  type OptionScope,
  type OptionSetter,
  type RendererProps,
  type WindowInfo,
} from "./state.ts"
// The models, as `main.ts` has them: one cache serves both halves.
export { cache, Model } from "../worker/sdk/model.ts"
export { Pane, Plugin, Request, Session, Tab, View } from "../worker/sdk/models.gen.ts"
export type { Data, Fields } from "../worker/sdk/model.ts"
export { defineTheme, useAppearance, useMetrics, usePalette, useTheme, type Appearance, type DockSpec, type IconButtonProps, type Metrics, type TabIconProps, type TerminalPalette, type ThemeSpec, type Tokens } from "./theme.ts"
export { usePlugin } from "./contributions.ts"
export { useFont, type Font } from "./font.ts"
// A plugin's own channel from its `main.ts` (`ui.post`), and the server's events.
export { useChannel, useChannelListener } from "./channels.ts"
export { useEvent, type UseEventOptions, type WindowEvent } from "./events.ts"
export { conversationNames, type ConversationChanged, type ConversationProvider, type ConversationRead } from "../worker/sdk/conversation.ts"
export { useWebview, webview, webviewRun, webviewSnapshot, WebView, type WebState, type WebViewProps } from "./webview.tsx"
export { features, Icon, IconButton, IconImage, TabIcon, type IconProps } from "./elements.tsx"
// A tab's icon and status, for a theme's `TabIcon` part or a plugin's own list of tabs.
export { ICON, iconSource, showsStatus, useTabIcon, useTabStatus, type IconFrom, type IconSource, type TabIconInfo } from "./icons.ts"
// The renderer's own kit, for a theme or a component: native motion (keyframes, repeats, springs),
// presence, portals, and the headless select to style.
export {
  createPortal,
  motion,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
  type MotionDivProps,
} from "../react/index.ts"
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
  PressEvent,
  RadialGradientBackground,
  SpriteSheet,
  StateStyleDesc,
  StyleDesc,
} from "../native/host.ts"
export type { EventPayload } from "../native/addon"
export { Select, type SelectFieldProps, type SelectProps } from "./select.tsx"
// Settings' kit: a plugin's page or section (`contributes.settings`) is drawn with these.
export * from "./settings/index.ts"
export { haptic, type HapticPattern } from "./haptic.ts"
export { heldShortcut, keyGlyphs, shortcut } from "./actions.ts"
// What is held down in the window (`cmd`), for a theme to show the keys it has while ⌘ is.
export { useHeldModifiers } from "../app/state.ts"
export { openSettings } from "../app/settings/open.ts"
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
} from "./kit.tsx"

