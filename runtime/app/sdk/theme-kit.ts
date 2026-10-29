// `import … from "pumi/theme"`: the kit a UI theme draws the window with, the one the app's own
// theme (app/theme/default.tsx) uses. Its words are the window's own: ids are the strings
// formats print (`%3`, `@2`, `$1`; `num()` makes a number of one), and its hooks read the raw
// state the app keeps (`useApp`). A plugin's component uses `pumi` instead; a theme uses both.
//
// - the window's parts: `useComponents()` (the theme's own, over the app's) and `Components`
// - the state: `useApp`, `useSessions`, `useViews`, `useTab`, `useFloats`, `useZoomed`, …
// - the layout: `useLayout`, `leavesOf`, `panesOf`, `tabsOf`, `shownOf`, `paneOfTab`, `leafOf`,
//   `usePaneTabs` (whether panes hold tabs)
// - a tab's icon: `useTabIcon` (its `@icon`, its agent's mark, its block's), `useTabStatus`,
//   `showsStatus` (the app's rule for which to show it), and the `TabIcon` part, which every
//   place showing a tab's icon draws (`IconImage` and `TabIcon` in `pumi` draw them)
// - slots and blocks: `useSlot` (components, and actions as data: the plugins' and the app's own),
//   `useRenderer`, `useRenderers`, `renderComponent`, `Boundary`
// - actions: `useMenu` (a slot's actions in the theme's `Menu`), `addActions`, `grouped`,
//   `confirm` and `prompt` (in the theme's `Dialog`), `useRename`, `startRename`
// - the layout: `useSidebar`, `usePaneTitles`, `useAutoEqualize`, `equalize`, `runMove`, `useLayoutStyle`
// - moving things by dragging: `useDropBox` (a drop target that knows its box), `carried`/`tabDrop`/
//   `stripDrop`/`rowDrop` (where a drag lands, and what that does as `Move`s), `runMove` (`Move`s
//   as the server's calls)
// - the theme's own settings the kit reads (`useThemeSetting`, `THEME_SETTINGS`): `usePaneTitles`,
//   `useAutoEqualize`, `useFocusFollowsMouse` (`useFocusOnHover`), `useOutlineFocused`,
//   `useSidebarSwipe` (the side bar's `onScroll`), `useViewIcons`, `useAnimate`, and `haptics`. A
//   plugin's theme declares the ones it wants in its manifest (`contributes.options.pane_titles`);
//   one it does not declare is its default
// - docks (`defineTheme({ docks })`): `Dock`, `useDock`, `useDockPanels`, `componentKey`
// - commands: `commands`, `useBinding`, `run`, `windowContext`, `setContextResolver`
// - what the theme keeps: `useLocal` (by key, kept across restarts)
// - the theme: `useAppearance`, `useThemeOption`
export {
  agentsOf,
  useAgentOf,
  useAgents,
  useApp,
  useConnection,
  useFloats,
  useFocusedTab,
  useLayout,
  useOption,
  useSession,
  useSessions,
  useTab,
  useView,
  useViews,
  useWindow,
  useZoomed,
  type Agent,
  type PaneLeaf,
  type Session,
  type Snapshot,
  type State,
  type Tab,
  type Tree,
  type View,
  type WindowInfo,
} from "../app/state.ts"
export { call, num, serverOf, via } from "../app/engine.ts"
export {
  useAppearance,
  useComponents,
  useLayoutStyle,
  useThemeOption,
  type ActionItem,
  type Components,
  type DialogProps,
  type IconButtonProps,
  type LayoutStyle,
  type MenuProps,
  type TabIconProps,
} from "./theme.ts"
export { ICON, iconSource, showsStatus, tabIcon, useTabIcon, useTabStatus, type IconFrom, type IconSource, type TabIconInfo } from "./icons.ts"
export { Boundary, renderComponent, usePlugin, useRenderer, useRenderers, useSlot, type ComponentItem } from "./contributions.ts"
export { addActions, grouped, shortcut, type Action, type ActionContext, type ActionProvider } from "./actions.ts"
export { useMenu } from "./menu.ts"
export {
  carried,
  plan,
  regionAt,
  regionBox,
  REGIONS,
  reorder,
  rowDrop,
  sortSessions,
  stripDrop,
  tabDrop,
  whereIn,
  type Carried,
  type Move,
  type Region,
  type Row,
  type Side,
  type Step,
  type Where,
} from "../app/theme/moves.ts"
export { useDropBox, within, type Bounds } from "../app/theme/drop.tsx"
export { confirm, prompt, type ConfirmOptions, type PromptOptions } from "./dialogs.ts"
export { rename, startRename, useRename, type Renamable, type RenameInput } from "./rename.ts"
export {
  editOf,
  equalize,
  equalized,
  leafOf,
  leavesOf,
  paneOfTab,
  panesOf,
  runMove,
  shownOf,
  tabsOf,
  useAutoEqualize,
  VIEW_ICONS_MAX,
  useFocusFollowsMouse,
  useFocusOnHover,
  useOutlineFocused,
  usePaneTabs,
  usePaneTitles,
  useSidebar,
  useSidebarSwipe,
  useSwipeSidebar,
  useViewIcons,
  type LayoutEdit,
} from "../app/layout.ts"
export { THEME_SETTINGS, useAnimate, useThemeSetting, type ThemeSetting } from "../app/theme-settings.ts"
export { componentKey, Dock, useDock, useDockPanels, type DockEdge, type DockPanels, type DockProps, type DockState } from "./dock.tsx"
export { useBinding } from "../app/commands.ts"
export * as commands from "../app/commands.ts"
export { getLocal, setLocal, useLocal } from "../app/local.ts"
export { run, setContextResolver, windowContext } from "./context.ts"
export { useStatus } from "./elements.tsx"
export {
  DismissableLayer,
  useIsPresent,
  usePresence,
  useWindowInsets,
  useWindowSize,
  type DismissableLayerProps,
} from "../react/index.ts"
