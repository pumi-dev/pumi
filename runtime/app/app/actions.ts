// The app's own actions, as data in the slots that hold them: a tab's menu (Open With, Move To,
// Split, Zoom, Close), an agent's (the same), a view's (Rename, Close) and a session's (Rename,
// New View, New Worktree, Open in New Window, Close). Whatever theme draws a menu gets them out of
// `useSlot(slot, args)`, after the plugins' items, and draws them as it likes.
import { call, via } from "./engine.ts"
import { windowContext } from "../sdk/context.ts"
import { addActions, type Action, type ActionContext } from "../sdk/actions.ts"
import { FILE, TAB_RENDERER, rendererFor, renderersFor, RENDERERS } from "../sdk/contributions.ts"
import { startRename } from "../sdk/rename.ts"
import { leavesOf, paneOfTab } from "./layout.ts"
import { tabLabel, viewName } from "./names.ts"
import * as worktreeViews from "./worktree-views.ts"

const swallow = (err: unknown) => console.error(String(err))
const PUMI = "pumi"

/** An action of the app's. */
function act(id: string, title: string, run: () => unknown, more: Partial<Action> = {}): Action {
  return { id, title, plugin: PUMI, run: () => void Promise.resolve(run()).catch(swallow), ...more }
}

/** A submenu of the app's. */
function sub(id: string, title: string, items: Action[], more: Partial<Action> = {}): Action {
  return { id, title, plugin: PUMI, items, run: () => {}, ...more }
}

/** A tab's own items: in its menu, and in its agent's. */
function tabActions(args: Record<string, unknown>, { state }: ActionContext): Action[] {
  const id = typeof args.tab === "string" ? args.tab : undefined
  const tab = id ? state.snapshot?.tabs.find((x) => x.tab_id === id) : undefined
  if (!id || !tab) return []
  const run = (method: string, ...params: unknown[]) => () => call(via(id, method), ...params)
  // What draws the tab: the app's pin (`@tab_renderer`), and for a file the `@viewer` its own
  // methods go to.
  const pick = (plugin: string | null) => () => {
    const set = (name: string) => call(via(id, "option.set"), name, plugin, { scope: "tab", id })
    return tab.block === FILE ? Promise.all([set(TAB_RENDERER), set("@viewer")]) : set(TAB_RENDERER)
  }
  const layout = state.snapshot?.layouts.find((l) => l.view_id === tab.view_id)
  const tree = state.trees[tab.view_id]
  const pane = paneOfTab(tree, id)
  // Its pane is zoomed: the one with the view's focus, while the view is.
  const zoomed = !!layout?.zoomed && !!pane && pane === paneOfTab(tree, layout.focused_tab_id)
  const floating = (tab.place as { kind?: string } | undefined)?.kind === "float"
  const zoomable = !floating && !!pane && (zoomed || (!!tree && "axis" in tree))
  const tabs = state.options.pane_tabs !== false
  // The other panes of its view, and what a tab moved into one of them is called by.
  const others = tree ? leavesOf(tree).filter((p) => p !== pane) : []
  const views = (state.snapshot?.views ?? []).filter((v) => v.workspace_id === tab.workspace_id && v.view_id !== tab.view_id)
  const out: Action[] = []
  const { list: renderers, pinned } = renderersFor(tab)
  if (renderers.length > 1) {
    const current = rendererFor(tab, state.options[RENDERERS] as Record<string, string> | undefined)
    out.push(
      sub("open_with", "Open With", [
        ...renderers.map((r) => act(`open_with.${r.plugin}`, r.title ?? r.plugin, pick(r.plugin), { checked: r.plugin === current?.plugin, group: "renderers" })),
        ...(pinned ? [act("open_with.default", "Default", pick(null), { group: "default" })] : []),
      ], { group: "place" }),
    )
  }
  out.push(
    sub("move_to", "Move To", [
      act("move_to.view", "New View", run("tab.update", id, { place: { kind: "view" } }), { group: "new" }),
      act("move_to.session", "New Session", run("tab.update", id, { place: { kind: "session" } }), { group: "new" }),
      floating
        ? act("move_to.tile", "Tile", run("tab.update", id, { place: { kind: "tiled", view: tab.view_id } }), { group: "new" })
        : act("move_to.float", "Float", run("tab.update", id, { place: { kind: "float", view: tab.view_id } }), { group: "new" }),
      ...views.map((v) => act(`move_to.view.${v.view_id}`, viewName(state.snapshot, state.trees, v.view_id), run("tab.update", id, { place: { kind: "tiled", view: v.view_id } }), { group: "views" })),
      // Into another pane of its view, as one of its tabs (named by its tab in front).
      ...(tabs && !floating
        ? others.map((p) => {
            const front = state.snapshot?.tabs.find((t) => t.tab_id === (p.active ?? p.tabs[0]))
            return act(`move_to.pane.${p.pane}`, `Pane: ${front ? tabLabel(front) : p.pane}`, run("tab.update", id, { place: { kind: "tiled", pane: p.pane } }), { group: "panes" })
          })
        : []),
    ], { group: "place" }),
  )
  // Out of its pane into one of its own beside it, while the pane holds others.
  if (pane && pane.tabs.length > 1) {
    out.push(act("split_out", "Move to Pane Right", run("tab.update", id, { place: { kind: "tiled", pane: pane.pane, split: "right" } }), { icon: "split", group: "split" }))
  }
  if (tabs && pane) out.push(act("new_tab", "New Tab", run("tab.open", "terminal", {}, { kind: "tiled", pane: pane.pane }), { command: "new_tab", icon: "plus", group: "split" }))
  out.push(
    act("split_right", "Split Right", run("tab.open", "terminal", {}, { kind: "tiled", beside: id, split: "right" }), { command: "split_vertical", icon: "split", group: "split" }),
    act("split_down", "Split Down", run("tab.open", "terminal", {}, { kind: "tiled", beside: id, split: "down" }), { command: "split_horizontal", icon: "split_down", group: "split" }),
  )
  if (zoomable) out.push(act("zoom", zoomed ? "Restore Layout" : "Zoom Pane", run("pane.update", pane!.pane, { zoom: !zoomed }), { command: "zoom", icon: zoomed ? "minimize" : "maximize", checked: zoomed, group: "split" }))
  if (pane && pane.tabs.length > 1) {
    out.push(act("close_others", "Close Other Tabs", () => Promise.all(pane.tabs.filter((t) => t !== id).map((t) => call(via(t, "tab.close"), t))), { danger: true, group: "close" }))
  }
  out.push(act("close", "Close Tab", run("tab.close", id), { command: "close_tab", icon: "close", danger: true, group: "close" }))
  if (pane && pane.tabs.length > 1) out.push(act("close_pane", `Close Pane (${pane.tabs.length} tabs)`, run("pane.close", pane.pane), { command: "close_pane", danger: true, group: "close" }))
  return out
}

/** A view's own items. */
function viewActions(args: Record<string, unknown>, { state }: ActionContext): Action[] {
  const id = typeof args.view === "string" ? args.view : undefined
  const view = id ? state.snapshot?.views.find((v) => v.view_id === id) : undefined
  if (!id || !view) return []
  const tabs = (state.snapshot?.tabs ?? []).filter((t) => t.view_id === id).length
  const label = viewName(state.snapshot, state.trees, id)
  return [
    act("rename", "Rename…", () => startRename({ view: id }, label), { group: "name" }),
    act("close", tabs > 1 ? `Close View (${tabs} tabs)` : "Close View", () => call(via(id, "view.close"), id), { command: "close_view", icon: "close", danger: true, group: "close" }),
  ]
}

/** A session's own items. */
function sessionActions(args: Record<string, unknown>, { state, window }: ActionContext): Action[] {
  const id = typeof args.session === "string" ? args.session : undefined
  const session = id ? state.snapshot?.workspaces.find((w) => w.workspace_id === id) : undefined
  if (!id || !session) return []
  const label = session.label || id
  const current = window?.session === id
  const win = () => (window ? windowContext(window.id)?.window : undefined)
  const out: Action[] = [
    act("rename", "Rename…", () => startRename({ session: id }, label), { group: "name" }),
    act(
      "new_view",
      "New View",
      () => call(via(id, "tab.open"), "terminal", {}, { kind: "view", session: id }).then(() => current || window?.show(id)),
      { command: "new_view", icon: "plus", group: "open" },
    ),
  ]
  const w = win()
  // Once the side bar has read its worktrees (on hover): a session in a repository.
  if (w && worktreeViews.cached(id).length) {
    out.push(act("new_worktree", "New Worktree…", () => worktreeViews.createAsking(w, id), { command: "new_worktree", icon: "git_worktree", group: "open" }))
  }
  if (w) out.push(act("new_window", "Open in New Window", () => w.manager.open(id), { group: "open" }))
  out.push(
    act("close", "Close Session", () => call(via(id, "session.close"), id), { command: "close_workspace", icon: "close", danger: true, group: "close" }),
  )
  return out
}

addActions("tab.menu", tabActions)
addActions("tab.row.menu", tabActions)
addActions("view.menu", viewActions)
addActions("session.menu", sessionActions)
