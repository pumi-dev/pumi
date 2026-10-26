// What any theme's window does to a view's layout, and the theme settings it draws by: whether the
// side bar shows (the window's own state, kept on this Mac), and the drawing theme's own settings
// (`theme-settings.ts`): whether tiled panes have title bars, whether a view's splits are made even
// as tabs open and close, the focus following the mouse, the focused pane's outline, the side
// bar's swipe, how many tab icons a view shows. The commands for them (`toggle_sidebar`,
// `toggle_pane_titles`, `equalize_panes`, `toggle_auto_equalize`) are the app's, whatever the theme
// draws; they set the drawing theme's settings.
import { useRef } from "react"
import type { EventPayload } from "../native/addon.js"
import { dragging } from "../sdk/drag.ts"
import { call, store, via, type PaneLeaf, type Tree } from "./engine.ts"
import { getLocal, removeLocal, setLocal, useLocal } from "./local.ts"
import { setOption } from "./settings/store.ts"
import { useOption } from "./state.ts"
import { THEME_SETTINGS, VIEW_ICONS_MAX, useThemeSetting } from "./theme-settings.ts"
import * as moves from "./theme/moves.ts"

export { VIEW_ICONS_MAX, toggleSetting } from "./theme-settings.ts"

const swallow = (err: unknown) => console.error(String(err))

/** The window's state, kept on this Mac (their keys are the default theme's). */
export const PREFS = {
  sidebar: "default.sidebar",
  sidebarWidth: "default.sidebar.width",
} as const

/** What this Mac kept before they were the default theme's settings, by the setting each is now. */
const MOVED = {
  pane_titles: "default.panes.titles",
  pane_equalize: "default.panes.equalize",
  focus_follows_mouse: "default.panes.focus_follows_mouse",
  pane_outline: "default.panes.outline",
  sidebar_swipe: "default.sidebar.swipe",
  view_icons: "default.views.icons",
} as const

/**
 * Moves what this Mac kept of the default theme's settings, from before they were settings, into
 * settings.json, once the server's options are here: a value already set there wins.
 */
export function moveLocalPrefs() {
  const options = store.get().options
  for (const [key, local] of Object.entries(MOVED) as [keyof typeof MOVED, string][]) {
    const value = getLocal<unknown>(local, undefined)
    if (value === undefined) continue
    const option = THEME_SETTINGS[key].option
    if (options[option] === undefined) setOption(option, value as boolean | number).catch(swallow)
    removeLocal(local)
  }
}

/** Whether the side bar shows (`toggle_sidebar`, ⌘B), and what sets it. On by default. */
export function useSidebar() {
  return useLocal(PREFS.sidebar, true)
}

/** Whether a tiled pane has a title bar (`toggle_pane_titles`): the theme's `pane_titles`, on by default. */
export function usePaneTitles() {
  return useThemeSetting("pane_titles")
}

/** Whether a view's splits are made even each time a tab opens in it or closes: the theme's `pane_equalize`, on by default. */
export function useAutoEqualize() {
  return useThemeSetting("pane_equalize")
}

/** Whether a pane takes the focus as the mouse comes onto it, without a click: the theme's `focus_follows_mouse`, off by default. */
export function useFocusFollowsMouse() {
  return useThemeSetting("focus_follows_mouse")
}

/**
 * What a tab does as the mouse comes onto it (call it from its `onMouseEnter`): takes the focus,
 * while `useFocusFollowsMouse` is on, it has not, and nothing is being dragged over it.
 */
export function useFocusOnHover(tab: string | undefined, focused: boolean): () => void {
  const [follow] = useFocusFollowsMouse()
  return () => {
    if (follow && tab && !focused && !dragging()) call(via(tab, "tab.focus"), tab).catch(swallow)
  }
}

/** Whether the focused pane has an outline while its view has other panes: the theme's `pane_outline`, off by default. */
export function useOutlineFocused() {
  return useThemeSetting("pane_outline")
}

/** Whether two fingers swept left over the side bar hide it: the theme's `sidebar_swipe`, on by default (`useSidebarSwipe`). */
export function useSwipeSidebar() {
  return useThemeSetting("sidebar_swipe")
}

/** How many of a view's tabs show as icons on its row or its bar: the theme's `view_icons`, 0 to `VIEW_ICONS_MAX` (3 by default). */
export function useViewIcons(): [number, (next: number | ((old: number) => number)) => void] {
  const [n, set] = useThemeSetting("view_icons")
  const shown = typeof n === "number" && Number.isFinite(n) ? Math.min(Math.max(Math.round(n), 0), VIEW_ICONS_MAX) : VIEW_ICONS_MAX
  return [shown, set]
}

/** How far a sideways swipe goes before it hides the side bar, in px, and how much more sideways than up or down it must be. */
const SWIPE_DISTANCE = 60
const SWIPE_SLANT = 2
/** A pause this long between scroll events ends a swipe that said no phase. */
const SWIPE_GAP = 250

/**
 * The side bar's `onScroll`: two fingers swept left over it (a trackpad's, not a wheel's) hide it,
 * once a swipe, while `useSwipeSidebar` is on. Its lists scroll up and down: a swipe
 * mostly up or down is theirs.
 */
export function useSidebarSwipe(hide: () => void): ((e: EventPayload) => void) | undefined {
  const [on] = useSwipeSidebar()
  const swipe = useRef({ x: 0, y: 0, at: 0, done: false })
  if (!on) return undefined
  return (e) => {
    if (!e.precise) return
    const s = swipe.current
    const now = Date.now()
    const ended = e.touchPhase === "ended" || e.touchPhase === "cancelled"
    if (ended || e.touchPhase === "started" || now - s.at > SWIPE_GAP) Object.assign(s, { x: 0, y: 0, done: false })
    s.at = now
    if (ended) return
    s.x += e.deltaX ?? 0
    s.y += e.deltaY ?? 0
    // Fingers moving left move the content left: a negative delta, with natural scrolling.
    if (!s.done && s.x <= -SWIPE_DISTANCE && Math.abs(s.x) >= SWIPE_SLANT * Math.abs(s.y)) {
      s.done = true
      hide()
    }
  }
}

export function toggle(pref: (typeof PREFS)["sidebar"]) {
  setLocal(pref, !getLocal(pref, true))
}

export type Branch = Extract<Tree, { axis: unknown }>

/** The panes of a tree, in order. */
export function leavesOf(tree: Tree): PaneLeaf[] {
  if ("pane" in tree) return [tree]
  if ("app" in tree) return []
  return [...leavesOf(tree.first), ...leavesOf(tree.second)]
}

/** The panes' ids in a tree, in order. */
export function panesOf(tree: Tree): string[] {
  return leavesOf(tree).map((p) => p.pane)
}

/** The tiled tabs in a tree: pane by pane in order, each pane's in its strip's order. */
export function tabsOf(tree: Tree): string[] {
  return leavesOf(tree).flatMap((p) => p.tabs)
}

/** The tabs in front of their panes, in order: the ones drawn. */
export function shownOf(tree: Tree): string[] {
  return leavesOf(tree).flatMap((p) => p.active ?? p.tabs[0] ?? [])
}

/** The pane in a tree holding `tab`. */
export function paneOfTab(tree: Tree | undefined, tab: string | null | undefined): PaneLeaf | undefined {
  if (!tree || !tab) return undefined
  return leavesOf(tree).find((p) => p.tabs.includes(tab))
}

/** A pane of a tree, by its id. */
export function leafOf(tree: Tree | undefined, pane: string | undefined): PaneLeaf | undefined {
  if (!tree || !pane) return undefined
  return leavesOf(tree).find((p) => p.pane === pane)
}

/** Whether a pane holds several tabs, with a strip of them over it (`pane_tabs`, on by default); off, every tab is a pane of its own, as in tmux. */
export function usePaneTabs(): boolean {
  return useOption<boolean>("pane_tabs") !== false
}

/** How many panes sit side by side along `axis` in a tree: what an even split shares by. */
function span(tree: Tree, axis: Branch["axis"]): number {
  if (!("axis" in tree)) return 1
  const a = span(tree.first, axis)
  const b = span(tree.second, axis)
  return tree.axis === axis ? a + b : Math.max(a, b)
}

/**
 * The tree with every pane its even share, as Vim's `CTRL-W =`: a split of three panes gives each
 * a third, and a column beside a pane counts as one. The same tree when it is already even.
 */
export function equalized(tree: Tree): Tree {
  if (!("axis" in tree)) return tree
  const first = equalized(tree.first)
  const second = equalized(tree.second)
  const a = span(tree.first, tree.axis)
  const ratio = a / (a + span(tree.second, tree.axis))
  if (first === tree.first && second === tree.second && Math.abs(ratio - tree.ratio) < 0.002) return tree
  return { ...tree, ratio, first, second }
}

/** The tree with `node`'s split moved to `ratio`. */
export function withRatio(tree: Tree, node: Branch, ratio: number): Tree {
  if (tree === node) return { ...node, ratio }
  if (!("axis" in tree)) return tree
  return { ...tree, first: withRatio(tree.first, node, ratio), second: withRatio(tree.second, node, ratio) }
}

/** The tree as `view.update` takes it, each pane by its id (its tabs stay as they are); nothing if it holds an app. */
function layoutOf(tree: Tree): Exclude<Tree, { app: number }> | undefined {
  if ("app" in tree) return undefined
  if ("pane" in tree) return { pane: tree.pane, tabs: [] }
  const first = layoutOf(tree.first)
  const second = layoutOf(tree.second)
  if (!first || !second) return undefined
  return { axis: tree.axis, ratio: tree.ratio, first, second }
}

export function setLayout(view: string, tree: Tree): Promise<void> {
  const layout = layoutOf(tree)
  if (!layout) return Promise.reject(new Error("pumi: a layout with an app in it is not laid out again"))
  return call(via(view, "view.update"), view, { layout })
}

/** Makes a view's splits even, unless they are (its tree as the store has it, by default). */
export function equalize(view: string, tree: Tree | undefined = store.get().trees[view]): Promise<void> {
  if (!tree) return Promise.resolve()
  const even = equalized(tree)
  // A pane opened or closed since the tree came is refused (the panes differ); the next tree is
  // equalized in its turn, so the refusal is not worth a line in the log.
  return even === tree ? Promise.resolve() : setLayout(view, even).catch(() => {})
}

/** What the layout's dividers do to the view they are in, and what a tab dropped on its panes does. */
export interface LayoutEdit {
  resize(node: Branch, ratio: number): Promise<void>
  equalize(): void
  move(move: moves.Move): void
}

/** The order the side bar keeps its sessions in (theirs on the server is the order they were made in). */
export const SESSION_ORDER = "default.agents.order"
const NONE: string[] = []

/** Does what a drag dropped: its calls to the server one after another, or a sort of the side bar's own. */
export function runMove(move: moves.Move) {
  const snapshot = store.get().snapshot
  if (move.kind === "session.order") {
    const shown = moves.sortSessions(snapshot?.workspaces ?? [], getLocal<string[]>(SESSION_ORDER, NONE)).map((s) => s.workspace_id)
    return setLocal(SESSION_ORDER, moves.reorder(shown, move.session, move.anchor, move.after))
  }
  moves
    .plan(move, snapshot)
    .reduce<Promise<unknown>>((done, step) => done.then(() => call(via(step.on, step.method), ...step.params)), Promise.resolve())
    .catch(swallow)
}

/** What a view's dividers do, for whichever part draws them: the edit reads the store when it acts. */
export function editOf(view: string | undefined): LayoutEdit | undefined {
  if (!view) return undefined
  return {
    resize(node, ratio) {
      const tree = store.get().trees[view]
      return tree ? setLayout(view, withRatio(tree, node, ratio)) : Promise.resolve()
    },
    equalize: () => void equalize(view),
    move: runMove,
  }
}
