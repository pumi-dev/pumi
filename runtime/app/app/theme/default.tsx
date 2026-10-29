// The app's own window: what draws every part a theme leaves out, and the whole window when no
// theme is in use (and in safe mode). It is pumi's own look: a side bar of what plugins
// contribute, the title bar at its top and its groups' buttons at its foot, beside the view's panes;
// with the side bar hidden, the views are a header over the panes.
import { createContext, Fragment, memo, useCallback, useContext, useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react"
import { basename } from "node:path"
import { assets } from "../assets.ts"
import { call, store, via } from "../engine.ts"
import { openSession } from "../commands.ts"
import { FILE, renderComponent, useRenderer, useSlot } from "../../sdk/contributions.ts"
import { grouped, heldShortcut, keyGlyphs, shortcut, type Action as ActionItem } from "../../sdk/actions.ts"
import { run as runCommand } from "../../sdk/context.ts"
import { homedir } from "node:os"
import { useMenu } from "../../sdk/menu.ts"
import { useRename } from "../../sdk/rename.ts"
import { equalize, equalized, editOf, leafOf, paneOfTab, panesOf, runMove, SESSION_ORDER, shownOf, tabsOf, useAutoEqualize, useFocusOnHover, useOutlineFocused, usePaneTabs, usePaneTitles, useSidebar, useSidebarSwipe, useViewIcons, PREFS } from "../layout.ts"
import { motion } from "../../react/index.ts"
import { useAnimate } from "../theme-settings.ts"
import { useDragState, useDraggable } from "../../sdk/drag.ts"
import * as drop from "./drop.tsx"
import { useGpuix } from "../../react/hooks/use-gpuix.ts"
import * as sidebar from "./sidebar.ts"
import * as moves from "./moves.ts"
import { confirm } from "../../sdk/dialogs.ts"
import {
  useApp,
  useFloats,
  useFocusedTab,
  useLayout,
  useSession,
  useSessions,
  useTab,
  useView,
  useViews,
  useWindow,
  useZoomed,
  type HeldModifiers,
  type Tree,
} from "../state.ts"
import { METRICS, useComponents, useMetrics, useTheme, withOpacity, type Components, type DialogProps, type IconButtonProps, type Metrics, type MenuProps, type TabIconProps } from "../../sdk/theme.ts"
import { IconImage, TabIcon } from "../../sdk/elements.tsx"
import { showsStatus, useTabIcon, useTabStatus } from "../../sdk/icons.ts"
import { useHover } from "../../sdk/kit.tsx"
import { useTab as useTabModel } from "../../sdk/state.ts"
import { fromServer } from "../serverkeys.ts"
import { useLocal } from "../local.ts"
import { tabLabel, viewName } from "../names.ts"
import { FONT_DEFAULT } from "../../sdk/font.ts"
import { AtTop, TileMotion, Tiling } from "./tiles.tsx"
import * as worktreeViews from "../worktree-views.ts"
import { nameOf, shortName } from "../worktrees.ts"
import { runsTerminal } from "../terminals.ts"
import { useStarters, type Starters } from "../starters.ts"
import { AgentsChoice, PluginCards, SchemeChoice, SETUP_WIDTH, WindowChoice } from "./start-plugins.tsx"
import { explainNoNewTab } from "../new-tab.ts"
import { openSettings } from "../settings/open.ts"

// A new tab nothing can open (`new_tab` names no block) says so in the window; the rest go to the log.
const swallow = (err: unknown) => {
  if (!explainNoNewTab(err)) console.error(String(err))
}

/**
 * The default's text sizes at the theme's `fontSize` (`ui_font_size` for pumi's own): each part
 * says the size it has at 13pt (`ts(11)`), and the text scales from there.
 */
function useTextSize(): (size: number) => number {
  const base = useMetrics().fontSize
  return useCallback((size: number) => (base === METRICS.fontSize ? size : Math.round((size * base) / METRICS.fontSize)), [base])
}

const Icon = memo(function Icon({ name, size = 14, color }: { name: string; size?: number; color?: string }) {
  const src = assets.icon(name)
  if (!src) return null
  return <svg src={src} style={{ width: size, height: size, flexShrink: 0, ...(color ? { color } : {}) }} />
})

/** A button of the window's, as the theme draws it (its `IconButton` part). */
function IconButton(props: IconButtonProps) {
  const C = useComponents()
  return <C.IconButton {...props} />
}

/** The app's own `IconButton` part: the pane's split and close, a plugin's toolbar buttons. */
function DefaultIconButton({ icon, onClick, title, active, color }: IconButtonProps) {
  const t = useTheme()
  const m = useMetrics()
  return (
    <div
      onClick={(e: any) => {
        // The press is the button's, not the title bar's (which selects and drags its tab).
        e?.stopPropagation?.()
        onClick()
      }}
      // A double click on a button is two clicks of it, not a double click of what holds it.
      onDoubleClick={(e: any) => e?.stopPropagation?.()}
      aria-label={title}
      aria-pressed={active}
      title={title}
      style={{
        display: "flex",
        width: 24,
        height: 24,
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: m.radiusSmall,
        color: color ?? (active ? t.text : t.faint),
        cursor: "pointer",
        hover: { backgroundColor: t.hover, color: t.text },
        active: { backgroundColor: t.active },
      }}
    >
      <Icon name={icon} />
    </div>
  )
}

function Window() {
  const t = useTheme()
  const m = useMetrics()
  const C = useComponents()
  const [open, setShown] = useSidebar()
  // While the start screen shows the first run's choices, neither the side bar nor the header over the panes.
  const [setup, setSetup] = useState(false)
  const shown = open && !setup
  const [width, setWidth] = useLocal(PREFS.sidebarWidth, 260)
  const swipe = useSidebarSwipe(() => setShown(false))
  const overlays = useSlot("window.overlay").components
  return (
    <SetupContext.Provider value={setSetup}>
      <div style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%", position: "relative", backgroundColor: t.window, color: t.text, fontSize: m.fontSize }}>
        <ProjectTracker />
        {/* Open, the side bar runs top to bottom: the title bar at its top, its tree holding the views (a section with no header), the sections plugins put there, the buttons for their groups at its foot. */}
        {shown ? (
          <div cache onScroll={swipe} style={{ display: "flex", flexDirection: "column", width, flexShrink: 0, minWidth: 0, backgroundColor: t.surface, borderRightWidth: 1, borderColor: t.border, position: "relative" }}>
            <SidebarHeader />
            <SessionHeader />
            <C.Panel point="panel" />
            <PanelButtons />
            <Resizer width={width} setWidth={setWidth} />
          </div>
        ) : null}
        {/* The panes run top to bottom beside it, their title bars in the title bar's row; with it hidden, the views are a header over them. */}
        <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0, backgroundColor: t.editor }}>
          {shown || setup ? null : <Header />}
          <div style={{ display: "flex", flexGrow: 1, minWidth: 0, minHeight: 0 }}>
            <C.Layout />
          </div>
        </div>
        <MoveGhost />
        {/* The window's top edge moves it wherever it is pressed, over the tabs too; with no title bar, the title bar's height does. */}
        <div style={{ position: "absolute", top: 0, left: 0, width: "100%", height: setup ? TITLE_BAR : TOP_EDGE, appRegion: "drag" }} />
        {overlays.map((v) => (
          <div key={`${v.plugin}/${v.id}`} style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", pointerEvents: "none" }}>
            {renderComponent(v)}
          </div>
        ))}
      </div>
    </SetupContext.Provider>
  )
}

/** The side bar's edge: drag it to size the bar. */
function Resizer({ width, setWidth }: { width: number; setWidth(w: number): void }) {
  const t = useTheme()
  const [from, setFrom] = useState<{ x: number; width: number } | undefined>()
  return (
    <div
      onMouseDown={(e) => setFrom({ x: e.x ?? 0, width })}
      onMouseMove={(e) => from && setWidth(Math.min(600, Math.max(160, from.width + (e.x ?? 0) - from.x)))}
      onMouseUp={() => setFrom(undefined)}
      style={{ position: "absolute", top: 0, right: -3, width: 6, height: "100%", cursor: "col-resize", hover: { backgroundColor: `${t.accent}66` } }}
    />
  )
}

/** The title bar's height, and where its content starts: past the traffic lights. */
const TITLE_BAR = 38
const TITLE_INSET = 78
/** How far down from the window's top edge a press moves the window, whatever is under it. */
const TOP_EDGE = 4

/** The title bar while the side bar is hidden: the button that shows it, the session, the views. */
function Header() {
  const t = useTheme()
  const C = useComponents()
  const [, setOpen] = useSidebar()
  return (
    <div cache style={{ display: "flex", flexDirection: "row", alignItems: "center", height: TITLE_BAR, flexShrink: 0, gap: 6, paddingLeft: TITLE_INSET, paddingRight: 8, backgroundColor: t.surface, position: "relative", appRegion: "drag" }}>
      {/* The bar's bottom edge, drawn first so the front view can paint over it and open into the pane below. */}
      <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 1, backgroundColor: t.border }} />
      <IconButton icon="layout_sidebar_left" title="Show Side Bar" onClick={() => setOpen(true)} />
      <SessionButton shrink={false} />
      <C.ViewBar />
    </div>
  )
}

/** The side bar's top, the title bar while it shows: the traffic lights' room, the button that hides it, the session, and at its far end the button that opens a folder. */
function SidebarHeader() {
  const t = useTheme()
  const window = useWindow()
  const [, setOpen] = useSidebar()
  return (
    // Its bottom edge is the top panes' title bars' bottom edge, in one line across the window.
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", height: TITLE_BAR, flexShrink: 0, gap: 4, paddingLeft: TITLE_INSET, paddingRight: 8, backgroundColor: t.surface, borderBottomWidth: 1, borderColor: t.border, appRegion: "drag" }}>
      <IconButton icon="layout_sidebar_left" title="Hide Side Bar" onClick={() => setOpen(false)} />
      <SessionButton shrink />
      <div style={{ flexGrow: 1, minWidth: 0 }} />
      <IconButton icon="plus" title="Open Folder…" onClick={() => openFolder(window).catch(swallow)} />
    </div>
  )
}

/** The session the window shows, as a button that opens the session picker. `shrink`: it gives up width before what is beside it does. */
function SessionButton({ shrink }: { shrink: boolean }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const session = useSession()
  const [picking, setPicking] = useState(false)
  // A press on the button while the picker is open closes it (the press outside does), and the
  // click that follows must not open it again.
  const pressedWhileOpen = useRef(false)
  return (
    <div style={{ display: "flex", flexShrink: shrink ? 1 : 0, minWidth: 0, maxWidth: 220 }}>
      <div
        onMouseDown={() => (pressedWhileOpen.current = picking)}
        onClick={() => {
          if (pressedWhileOpen.current) pressedWhileOpen.current = false
          else setPicking(true)
        }}
        style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, height: 24, paddingLeft: 8, paddingRight: 8, borderRadius: m.radius, cursor: "pointer", minWidth: 0, backgroundColor: picking ? t.active : undefined, hover: { backgroundColor: picking ? t.active : t.hover } }}
      >
        <text style={{ fontWeight: 600, fontSize: ts(12), whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{session?.label || "pumi"}</text>
        <Icon name="chevron_up_down" size={12} color={t.faint} />
      </div>
      {picking ? <ProjectPicker close={() => setPicking(false)} /> : null}
    </div>
  )
}

// The sessions a window has open ("This Window", in the order it opened them) and the order the
// whole app last showed sessions in ("Recent", most recent first), kept across runs.
const hereKey = (window: { id: number }) => `default.projects.window.${window.id}`
const RECENT = "default.projects.recent"
const NONE: string[] = []

/** Notes the session the window shows as open in it, and as the most recent. */
function ProjectTracker() {
  const window = useWindow()
  const [, setHere] = useLocal(hereKey(window), NONE)
  const [, setRecent] = useLocal(RECENT, NONE)
  useEffect(() => {
    const shown = window.session
    if (!shown) return
    setHere((list) => (list.includes(shown) ? list : [...list, shown]))
    setRecent((list) => (list[0] === shown ? list : [shown, ...list.filter((id) => id !== shown)].slice(0, 50)))
  }, [window.session])
  return null
}

type PickerRow =
  | { kind: "here" | "recent"; id: string; label: string; current: boolean }
  | { kind: "action"; id: string; label: string; icon: string; run(): void }

/** The session picker, under the session button: a search field, this window's sessions, the
 * recent ones, and what opens another. The arrow keys move the highlight, Enter picks, Escape
 * closes, and the x on a row takes that session out of the window without closing it. */
function ProjectPicker({ close }: { close(): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const window = useWindow()
  const sessions = useSessions()
  const [here, setHere] = useLocal(hereKey(window), NONE)
  const [recent] = useLocal(RECENT, NONE)
  const [query, setQuery] = useState("")
  const [lit, setLit] = useState(0)

  const rows = useMemo(() => {
    const byId = new Map(sessions.map((s) => [s.workspace_id, s]))
    const q = query.trim().toLowerCase()
    const entry = (kind: "here" | "recent") => (id: string) => {
      const s = byId.get(id)!
      return { kind, id, label: s.label || id, current: id === window.session }
    }
    const open = [...here, ...(window.session && !here.includes(window.session) ? [window.session] : [])].filter((id) => byId.has(id))
    const rest = [...recent, ...sessions.map((s) => s.workspace_id)].filter((id, i, all) => byId.has(id) && !open.includes(id) && all.indexOf(id) === i)
    const matches = (r: { label: string }) => !q || r.label.toLowerCase().includes(q)
    const actions: PickerRow[] = [
      { kind: "action", id: "open-folder", label: "Open Local Folder…", icon: "folder_open", run: () => openFolder(window).catch(swallow) },
    ]
    return {
      here: open.map(entry("here")).filter(matches) as PickerRow[],
      recent: rest.map(entry("recent")).filter(matches) as PickerRow[],
      actions,
    }
  }, [sessions, here, recent, query, window])
  const all = [...rows.here, ...rows.recent, ...rows.actions]
  const at = Math.min(lit, all.length - 1)

  const pick = (r: PickerRow | undefined) => {
    if (!r) return
    close()
    if (r.kind === "action") r.run()
    else window.show(r.id)
  }

  const row = (r: PickerRow, i: number) => {
    const highlighted = i === at
    return (
      <div
        key={r.id}
        onMouseMove={() => i !== at && setLit(i)}
        onClick={() => pick(r)}
        style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, height: 30, flexShrink: 0, paddingLeft: 10, paddingRight: 6, borderRadius: m.radiusSmall, cursor: "pointer", backgroundColor: highlighted ? t.active : undefined }}
      >
        {r.kind === "action" ? <Icon name={r.icon} size={14} color={t.muted} /> : null}
        <text style={{ fontSize: ts(13), whiteSpace: "nowrap", textOverflow: "ellipsis", minWidth: 0, flexShrink: 1 }}>{r.label}</text>
        {r.kind !== "action" && r.current ? <Icon name="check" size={13} color={t.accent} /> : null}
        <div style={{ flexGrow: 1 }} />
        {r.kind === "here" && !r.current && highlighted ? (
          <div
            aria-label="Remove from This Window"
            onClick={(e: any) => {
              e?.stopPropagation?.()
              setHere((list) => list.filter((id) => id !== r.id))
            }}
            style={{ display: "flex", width: 20, height: 20, alignItems: "center", justifyContent: "center", borderRadius: m.radiusSmall, color: t.muted, hover: { backgroundColor: t.hover, color: t.text } }}
          >
            <Icon name="close" size={11} />
          </div>
        ) : null}
      </div>
    )
  }
  const heading = (title: string) => (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", height: 26, flexShrink: 0, paddingLeft: 10 }}>
      <text style={{ fontSize: ts(12), color: t.faint }}>{title}</text>
    </div>
  )
  const sep = <div style={{ height: 1, flexShrink: 0, marginTop: 4, marginBottom: 4, marginLeft: -6, marginRight: -6, backgroundColor: t.border }} />

  return (
    <anchored side="bottom" align="start" gap={6} fit="snap" snapMargin={8} deferred occlude priority={1} style={{ backgroundColor: "transparent" }}>
      <div
        onMouseDownOutside={close}
        style={{ display: "flex", flexDirection: "column", width: 320, maxHeight: 480, padding: 6, borderRadius: m.radius + 4, backgroundColor: t.surface, borderWidth: 1, borderColor: t.border, boxShadow: { offsetX: 0, offsetY: 8, blurRadius: 24, spreadRadius: 0, color: "#00000066" } }}
      >
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, height: 34, flexShrink: 0, paddingLeft: 10, paddingRight: 10, borderRadius: m.radius, borderWidth: 1, borderColor: t.accent, backgroundColor: t.editor }}>
          <Icon name="magnifying_glass" size={14} color={t.faint} />
          <input
            autoFocus
            value={query}
            placeholder="Search projects…"
            onChange={(e: any) => {
              setQuery(e.value ?? "")
              setLit(0)
            }}
            onSubmit={() => pick(all[at])}
            onKeyDown={(e) => {
              const step = e.key === "down" ? 1 : e.key === "up" ? -1 : 0
              if (step) setLit((at + step + all.length) % all.length)
              else if (e.key === "escape") close()
              else return
              e.preventDefault()
              e.stopPropagation()
            }}
            style={{ flexGrow: 1, minWidth: 0, fontSize: ts(13), color: t.text, backgroundColor: "transparent" }}
          />
        </div>
        {sep}
        <div style={{ display: "flex", flexDirection: "column", flexShrink: 1, minHeight: 0, overflowY: "scroll" }}>
          {!rows.here.length && !rows.recent.length ? (
            <div style={{ display: "flex", flexDirection: "row", alignItems: "center", height: 30, paddingLeft: 10 }}>
              <text style={{ fontSize: ts(13), color: t.faint }}>No matching projects</text>
            </div>
          ) : null}
          {rows.here.length ? heading("This Window") : null}
          {rows.here.map((r, i) => row(r, i))}
          {rows.here.length && rows.recent.length ? sep : null}
          {rows.recent.length ? heading("Recent") : null}
          {rows.recent.map((r, i) => row(r, rows.here.length + i))}
        </div>
        {sep}
        {rows.actions.map((r, i) => row(r, rows.here.length + rows.recent.length + i))}
      </div>
    </anchored>
  )
}

/** Asks for a folder and shows its session in `window`: the one already there, or a new one. */
async function openFolder(window: { id: number; show(session: string): void }) {
  const picked = await call("fs.pick", { kind: "directory", title: "Open Local Folder" })
  const dir = typeof picked === "string" ? picked : Array.isArray(picked) ? picked[0] : undefined
  if (!dir) return
  const found = store.get().snapshot?.workspaces.find((w) => w.cwd === dir)
  if (found) return window.show(found.workspace_id)
  const name = basename(dir) || dir
  await openSession(window, { cwd: dir, name })
}

function ViewBar({ session }: { session?: string }) {
  const t = useTheme()
  const C = useComponents()
  // By index, as the side bar has them: dragging a view there trades its index with its neighbours'.
  const views = [...useViews(session)].sort((a, b) => a.number - b.number)
  const front = useSession(session)?.active_view_id
  return (
    <div style={{ display: "flex", flexDirection: "row", alignSelf: "stretch", alignItems: "stretch", minWidth: 0, flexShrink: 1, overflowX: "scroll", borderLeftWidth: 1, borderColor: t.border }}>
      {views.map((view, index) => (
        <C.View key={view.view_id} view={view.view_id} index={index} active={view.view_id === front} />
      ))}
    </div>
  )
}

function View({ view: id, active }: { view: string; active: boolean; index: number }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const front = t.editor
  const view = useView(id)
  const { actions } = useSlot("view.actions")
  const label = useViewName(id)
  const [hovered, hover] = useHover()
  const { open, menu } = useMenu(["view.menu", "view.actions"], { view: id })
  // The bar shows the window's session: each view's keys by its number (⌘1…⌘9), while held.
  const keys = useHeldKeys("switch_view", view?.number ?? -1)
  if (!view) return null
  return (
    <div
      onClick={() => call(via(id, "view.focus"), id).catch(swallow)}
      onAuxClick={open}
      title={view.worktree ? `${label} · ${view.worktree}` : undefined}
      {...hover}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        // The bar's whole height, as tall as the header: not a strip at its top.
        alignSelf: "stretch",
        height: "100%",
        gap: 6,
        paddingLeft: 14,
        paddingRight: 8,
        maxWidth: 240,
        minWidth: 64,
        flexShrink: 1,
        cursor: "pointer",
        position: "relative",
        borderRightWidth: 1,
        borderBottomWidth: active ? 0 : 1,
        borderColor: t.border,
        // The front view is the pane area's colour and has no bottom edge: it opens into the panes.
        backgroundColor: active ? front : undefined,
        color: active ? t.text : t.muted,
        hover: { backgroundColor: active ? front : t.hover, color: t.text },
      }}
    >
      <ViewIcons view={id} ring={active ? front : hovered ? t.hover : t.surface} />
      {view.worktree ? <Icon name="git_worktree" size={12} color={t.faint} /> : null}
      <text style={{ fontSize: ts(13), whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{label}</text>
      <Slot slot="view.badge" args={{ view: id }} />
      {actions.map((a) => (
        <Contributed key={`${a.plugin}/${a.id}`} action={a} args={{ view: id }} />
      ))}
      {keys ? <HeldKeys keys={keys} /> : null}
      <div
        onClick={(e: any) => {
          e?.stopPropagation?.()
          call(via(id, "view.close"), id).catch(swallow)
        }}
        style={{ display: "flex", width: 18, height: 18, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: m.radiusSmall, opacity: active || hovered ? 1 : 0, hover: { backgroundColor: t.active, opacity: 1 } }}
      >
        <Icon name="close" size={10} />
      </div>
      {/* A tab dropped on the view joins it; at its left or right edge, a view moves beside it (as on its row in the side bar). */}
      <RowDrop row={{ kind: "view", session: view.workspace_id, view: id }} across />
      {menu}
    </div>
  )
}

/** A view's tab icon, across with its ring, and how far along each next one starts: the next ring
 * cuts a sliver off the mark before it. */
const VIEW_ICON = 16
const VIEW_ICON_RING = 1
const VIEW_ICON_STEP = 12

/**
 * A view's tabs as overlapping icons, in the order the side bar lists them (as the layout has them,
 * then floats): each as its `TabIcon` draws it; the count is in the colour of the most urgent
 * status among the tabs it hides. `ring` is the view's colour, what each icon is cut out of the
 * one before it with.
 */
function ViewIcons({ view, ring }: { view: string; ring: string }) {
  const ts = useTextSize()
  const t = useTheme()
  const tabs = useApp((s) => s.snapshot?.tabs ?? NO_SNAPSHOT_TABS)
  const tree = useLayout(view)
  // The most it shows (Settings' "Tab icons per view"); past them the last is a count, unless it shows only one.
  const [most] = useViewIcons()
  // The tabs in front of their panes first, as the layout has them, then the ones behind.
  const at = tree ? [...shownOf(tree), ...tabsOf(tree)] : []
  const rank = (tab: string) => (at.indexOf(tab) < 0 ? at.length : at.indexOf(tab))
  const held = tabs.filter((p) => p.view_id === view).sort((a, b) => rank(a.tab_id) - rank(b.tab_id))
  if (!held.length || most <= 0) return null
  const more = most > 1 ? held.length - most : 0
  const shown = held.slice(0, more > 0 ? most - 1 : most)
  const count = shown.length + (more > 0 ? 1 : 0)
  const worst = lead(held.slice(shown.length))
  const countColor = worst === "unknown" ? t.muted : (t[`status.${worst}`] ?? t.muted)
  // The stack's own width, given outright, with each icon placed in it: the label starts after
  // the last one, never under them.
  const width = VIEW_ICON + (count - 1) * VIEW_ICON_STEP
  // Not hit: an absolute box takes the press itself, and these are the view's to click.
  const chip = (i: number) => ({ position: "absolute" as const, pointerEvents: "none" as const, left: i * VIEW_ICON_STEP, top: 0, display: "flex", width: VIEW_ICON, height: VIEW_ICON, alignItems: "center" as const, justifyContent: "center" as const, borderRadius: VIEW_ICON / 2, borderWidth: VIEW_ICON_RING, borderColor: ring, backgroundColor: ring })
  const inner = VIEW_ICON - 2 * VIEW_ICON_RING
  return (
    <div style={{ position: "relative", width, minWidth: width, height: VIEW_ICON, flexShrink: 0 }}>
      {shown.map((p, i) => (
        // Each later one over the one before it, as the tabs read left to right.
        <div key={p.tab_id} style={chip(i)}>
          <TabIcon tab={p.tab_id} size={14} />
        </div>
      ))}
      {more > 0 ? (
        <div style={chip(shown.length)}>
          <div style={{ display: "flex", width: inner, height: inner, alignItems: "center", justifyContent: "center", borderRadius: inner / 2, borderWidth: 1, borderColor: countColor, backgroundColor: t.hover }}>
            <text style={{ fontSize: ts(8), color: countColor }}>{`+${more + 1}`}</text>
          </div>
        </div>
      ) : null}
    </div>
  )
}

/**
 * The app's own `TabIcon` part: a tab's status while it says something happened (working, done,
 * blocked, failed), else its own icon (`useTabIcon`): its `@icon`, a page's favicon, a plugin
 * block's. A tab with only the core's icon for its block shows any status it has instead, a ring
 * as much as a spinner.
 */
function DefaultTabIcon({ tab, size = 14, color }: TabIconProps) {
  const t = useTheme()
  const status = useTabStatus(tab)
  const icon = useTabIcon(tab)
  if (showsStatus(status, icon)) return <StatusIcon status={status} size={size} />
  return <IconImage icon={icon} size={size} color={color ?? t.muted} />
}

/** A slot's components, given `args`, and its actions as buttons. Memoized: give it the same `args` object to keep it from drawing again. */
const Slot = memo(function Slot({ slot, args, gap = 2 }: { slot: string; args?: Record<string, unknown>; gap?: number }) {
  const C = useComponents()
  const { components, actions } = useSlot(slot)
  if (!components.length && !actions.length) return null
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap, flexShrink: 0 }}>
      {components.map((v) => renderComponent(v, args))}
      {actions.map((a) => (
        <C.Action key={`${a.plugin}/${a.id}`} action={a} args={args} />
      ))}
    </div>
  )
})

/** A line between a menu's groups, from one edge of the card to the other. */
function MenuRule() {
  const t = useTheme()
  return <div style={{ height: 1, flexShrink: 0, marginTop: 5, marginBottom: 5, backgroundColor: t.border }} />
}

/** A menu's card, and a submenu's. */
function menuCard(t: Record<string, string>, m: Metrics) {
  return { display: "flex", flexDirection: "column", minWidth: 200, paddingTop: 4, paddingBottom: 4, borderRadius: m.radius, backgroundColor: t.surface, borderWidth: 1, borderColor: t.border, boxShadow: { offsetX: 0, offsetY: 6, blurRadius: 18, spreadRadius: 0, color: "#00000066" } } as const
}

/** How long a submenu stays once the mouse leaves it and its row: time to cross to it at a slant. */
const SUBMENU_GRACE = 200

/**
 * What a menu card's rows share: which of its submenus is open. A row's hover opens its submenu
 * and closes the others (after `SUBMENU_GRACE`, so the mouse can cross another row on the way);
 * `over` is the mouse going in or out of an open submenu, told to every card above it.
 */
interface MenuState {
  sub?: string
  enter(sub?: string): void
  /** Open `sub` (none: close the open one) now, without the grace: the keys' way. */
  show(sub?: string): void
  leave(): void
  over(inside: boolean): void
  /** Whether the mouse is over a submenu of this card's, which a press there is not outside. */
  inSub(): boolean
}

const MenuContext = createContext<MenuState | undefined>(undefined)

function useMenuState(parent?: MenuState): MenuState {
  const [sub, setSub] = useState<string | undefined>()
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const inside = useRef(false)
  useEffect(() => () => clearTimeout(timer.current), [])
  return useMemo(() => {
    const later = () => {
      clearTimeout(timer.current)
      timer.current = setTimeout(() => setSub(undefined), SUBMENU_GRACE)
    }
    return {
      sub,
      enter(next) {
        clearTimeout(timer.current)
        if (next) setSub(next)
        else if (sub) later()
      },
      show(next) {
        clearTimeout(timer.current)
        setSub(next)
      },
      leave: later,
      over(now) {
        inside.current = now
        if (now) clearTimeout(timer.current)
        else later()
        parent?.over(now)
      },
      inSub: () => inside.current,
    }
  }, [sub, parent])
}

/**
 * A menu card where the mouse is; a press outside it (and its submenus) closes it. The layer
 * itself is transparent (without a fill of its own it paints a square one behind the card's
 * corners), so the card is what hears the press outside.
 */
function MenuCard({ at, close, children }: { at: { x: number; y: number }; close(): void; children: ReactNode }) {
  const t = useTheme()
  const m = useMetrics()
  const state = useMenuState()
  return (
    <anchored position={at} fit="snap" snapMargin={8} deferred occlude priority={1} style={{ backgroundColor: "transparent" }}>
      <MenuContext.Provider value={state}>
        <div onMouseDownOutside={() => state.inSub() || close()} style={menuCard(t, m)}>
          {children}
        </div>
      </MenuContext.Provider>
    </anchored>
  )
}

/** The `Menu` part: a slot's actions in their groups (a rule between), submenus beside, then its components. */
function Menu({ at, actions, components, args, close }: MenuProps) {
  return (
    <MenuCard at={at} close={close}>
      <MenuRows actions={actions} close={close} />
      {components?.length && actions.length ? <MenuRule /> : null}
      {components?.map((v) => renderComponent(v, { ...args, close }))}
    </MenuCard>
  )
}

/** Actions as a menu's rows, in their groups; one with `items` opens a submenu. */
function MenuRows({ actions, close }: { actions: ActionItem[]; close(): void }) {
  return (
    <>
      {grouped(actions).map((group, i) => (
        <Fragment key={`${i}:${group[0]!.group ?? ""}`}>
          {i > 0 ? <MenuRule /> : null}
          {group.map((a) =>
            a.items ? (
              <SubMenu key={`${a.plugin}/${a.id}`} title={a.title}>
                <MenuRows actions={a.items} close={close} />
              </SubMenu>
            ) : (
              <MenuItem
                key={`${a.plugin}/${a.id}`}
                title={a.title}
                keys={a.keys}
                checked={a.checked}
                disabled={a.disabled}
                onClick={() => {
                  close()
                  a.run()
                }}
              />
            ),
          )}
        </Fragment>
      ))}
    </>
  )
}

/**
 * A row that opens a card of `children` beside it, on hover or on a click, as a Mac menu's does.
 * The card's top row lines up with this one.
 */
function SubMenu({ title, lit, onEnter, children }: { title: string; lit?: boolean; onEnter?(): void; children: ReactNode }) {
  const t = useTheme()
  const m = useMetrics()
  const menu = useContext(MenuContext)
  const own = useMenuState(menu)
  const open = menu?.sub === title
  return (
    <div style={{ display: "flex", flexDirection: "column", position: "relative", flexShrink: 0 }}>
      <MenuItem
        title={title}
        more
        active={open}
        lit={lit}
        onEnter={() => {
          onEnter?.()
          menu?.enter(title)
        }}
        onLeave={() => menu?.leave()}
        onClick={() => menu?.enter(title)}
      />
      {open ? (
        <anchored side="right" align="start" gap={6} offset={{ x: 0, y: -5 }} fit="snap" snapMargin={8} deferred occlude priority={2} style={{ backgroundColor: "transparent" }}>
          <MenuContext.Provider value={own}>
            {/* A long one (a session's views) a few rows' worth, then it scrolls. */}
            <div onMouseEnter={() => menu?.over(true)} onMouseLeave={() => menu?.over(false)} style={{ ...menuCard(t, m), maxHeight: 22 * 14 + 8, overflowY: "scroll" }}>
              {children}
            </div>
          </MenuContext.Provider>
        </anchored>
      ) : null}
    </div>
  )
}

/**
 * A menu's row: its title, and at the far end its keys (`⇧ ⌘ D`), a check, or the chevron of a
 * row that opens more (`SubMenu`'s, which is `active` while open). Hovering any other row closes
 * its card's open submenu. `lit`, given: a menu the keys move through says which row is lit (and
 * scrolled to), so the mouse's hover draws none of its own.
 */
function MenuItem({ title, icon, keys, checked, disabled, more, active, lit, detail, onEnter, onLeave, onClick }: { title: string; icon?: string; keys?: string; checked?: boolean; disabled?: boolean; more?: boolean; active?: boolean; lit?: boolean; detail?: string; onEnter?(): void; onLeave?(): void; onClick(): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const menu = useContext(MenuContext)
  const row = useRef<{ scrollIntoView?(): void } | null>(null)
  useEffect(() => {
    if (lit) row.current?.scrollIntoView?.()
  }, [lit])
  const keyed = lit !== undefined
  return (
    <div
      ref={row as any}
      onClick={disabled ? undefined : onClick}
      onMouseEnter={() => (onEnter ? onEnter() : menu?.enter())}
      onMouseLeave={onLeave}
      title={detail}
      aria-disabled={disabled}
      style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, height: 22, flexShrink: 0, marginLeft: 4, marginRight: 4, paddingLeft: 6, paddingRight: 6, borderRadius: m.radiusSmall, cursor: disabled ? "default" : "pointer", opacity: disabled ? 0.5 : 1, ...(active || lit ? { backgroundColor: t.hover } : {}), ...(disabled || keyed ? {} : { hover: { backgroundColor: t.hover } }) }}
    >
      {icon ? <Icon name={icon} size={13} color={t.muted} /> : null}
      <text style={{ fontSize: ts(13), color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{title}</text>
      {keys || checked !== undefined || more ? <div style={{ flexGrow: 1, minWidth: 16 }} /> : null}
      {keys ? <text style={{ fontSize: ts(12), color: t.muted, whiteSpace: "nowrap", flexShrink: 0 }}>{keys}</text> : null}
      {checked ? <Icon name="check" size={12} color={t.accent} /> : null}
      {more ? <Icon name="chevron_right" size={12} color={t.muted} /> : null}
    </div>
  )
}

/**
 * The `Dialog` part: a question in the middle of the window, over everything else (`confirm`,
 * `prompt`). Escape, a press outside it or Cancel gives up; Enter goes ahead. It holds the keys
 * while it is up, so no binding behind it runs.
 */
function Dialog({ title, detail, confirm: yes, cancel, danger, input, onConfirm, onCancel }: DialogProps) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  return (
    <anchored fill="window" occlude priority={1} style={{ backgroundColor: "transparent" }}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", width: "100%", height: "100%", padding: 20 }}>
        <div
          autoFocus={!input}
          tabIndex={0}
          onMouseDownOutside={onCancel}
          onKeyDown={(e: any) => {
            if (e.key === "escape") {
              e.preventDefault?.()
              onCancel()
            } else if (e.key === "enter" && !input) {
              e.preventDefault?.()
              onConfirm()
            }
          }}
          style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%", maxWidth: 360, padding: 16, borderRadius: m.radius + 4, backgroundColor: t.surface, borderWidth: 1, borderColor: t.border, keyboardFocusDim: false, boxShadow: { offsetX: 0, offsetY: 12, blurRadius: 32, spreadRadius: 0, color: "#00000080" } }}
        >
          <text style={{ fontSize: ts(13), fontWeight: 600, color: t.text }}>{title}</text>
          {detail ? <text style={{ fontSize: ts(12), color: t.muted }}>{detail}</text> : null}
          {input ? (
            <input
              autoFocus
              value={input.value}
              placeholder={input.placeholder}
              onChange={(e: any) => input.onChange(e.value ?? "")}
              onSubmit={onConfirm}
              onKeyDown={(e) => {
                if (e.key !== "escape") return
                e.preventDefault()
                e.stopPropagation()
                onCancel()
              }}
              style={{ height: 26, fontSize: ts(13), color: t.text, paddingLeft: 6, paddingRight: 6, borderRadius: m.radiusSmall, borderWidth: 1, borderColor: t.accent, backgroundColor: t.editor }}
            />
          ) : null}
          <div style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8, marginTop: 4 }}>
            <DialogButton title={cancel} onClick={onCancel} />
            <DialogButton title={yes} danger={danger} primary onClick={onConfirm} />
          </div>
        </div>
      </div>
    </anchored>
  )
}

/** A button of a `Dialog`: the one that goes ahead in the accent (or, when it ends something, the error colour). */
function DialogButton({ title, danger, primary, onClick }: { title: string; danger?: boolean; primary?: boolean; onClick(): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const fill = danger ? t.error : primary ? t.accent : t.active
  return (
    <div
      aria-label={title}
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e: any) => {
        if (e.key !== "enter" && e.key !== " ") return
        e.preventDefault?.()
        onClick()
      }}
      style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "center", height: 26, flexShrink: 0, paddingLeft: 12, paddingRight: 12, borderRadius: m.radiusSmall, cursor: "pointer", backgroundColor: fill, keyboardFocusDim: false, hover: { backgroundColor: primary || danger ? fill : t.hover, opacity: primary || danger ? 0.9 : 1 } }}
    >
      <text style={{ fontSize: ts(12), fontWeight: 500, color: danger ? t.window : primary ? t.accentText : t.text, whiteSpace: "nowrap" }}>{title}</text>
    </div>
  )
}

function Contributed({ action, args }: { action: ActionItem; args?: unknown }) {
  const C = useComponents()
  return <C.Action action={action} args={args} />
}

/** The panes each view was last drawn with, so a change in them is seen once whichever window draws it. */
const drawnPanes = new Map<string, string>()

function Layout({ view }: { view?: string }) {
  const tree = useLayout(view)
  const shown = useView(view)?.view_id
  const focused = useFocusedTab(view)
  const zoomed = useZoomed(view)
  const C = useComponents()
  const [autoEqualize] = useAutoEqualize()
  // Before the early return: a hook after it changes the hook order once a view has a tree.
  const floats = useFloats(view ?? undefined)
  const panes = tree ? [...panesOf(tree)].sort().join(" ") : ""
  useEffect(() => {
    if (!shown || !tree) return
    const was = drawnPanes.get(shown)
    drawnPanes.set(shown, panes)
    // Not on the first sight of a view: the layout it came with is the one it was left in.
    if (autoEqualize && was !== undefined && was !== panes) equalize(shown, tree)
  }, [shown, panes])
  // The even tree is drawn as soon as a pane opens or closes, until the server's comes back: the
  // panes move once, to their even shares, not to the ones the split left and then again (and a
  // terminal is told one new size, not two).
  const [even, setEven] = useState<{ from: string; to: Tree }>()
  const [seen, setSeen] = useState({ shown, panes })
  if (seen.shown !== shown || seen.panes !== panes) {
    setSeen({ shown, panes })
    if (autoEqualize && tree && seen.shown === shown && seen.panes !== "") setEven({ from: JSON.stringify(tree), to: equalized(tree) })
    else if (even) setEven(undefined)
  }
  const drawn = tree && even && even.from === JSON.stringify(tree) ? even.to : tree
  if (!tree || !drawn) return <StartScreen />
  // Keyed by the view: another view's panes are drawn as they are, not grown out of this one's.
  const zoom = zoomed ? paneOfTab(drawn, focused)?.pane : undefined
  const body = <Tiles key={shown} tree={drawn} view={shown} focused={focused} zoomed={zoom} />
  if (!floats.length) return body
  return (
    <div style={{ display: "flex", flexGrow: 1, minWidth: 0, minHeight: 0, position: "relative" }}>
      {body}
      {floats.map((p) => (
        <C.Float key={p.tab_id} tab={p.tab_id} />
      ))}
    </div>
  )
}

/** `/Users/me/src/app` as `~/src/app`. */
const HOME = homedir()
const tildePath = (path: string) => (HOME && (path === HOME || path.startsWith(`${HOME}/`)) ? `~${path.slice(HOME.length)}` : path)

/** How many sessions the start screen lists under Recent. */
const START_RECENT = 5
/** The start screen's width, but while it shows the first run's choices (`SETUP_WIDTH`). */
const START_WIDTH = 420

/** Whether the start screen shows the first run's choices, told the window, which puts its side bar away meanwhile. */
const SetupContext = createContext<(on: boolean) => void>(() => {})

/**
 * What the panes' room shows while there are none: pumi's mark, what starts something (a view of
 * the session shown, a session, a folder), the sessions shown last, and the settings. On the first
 * run, until its plugins are installed, a welcome and the choices instead (`StartPlugins`).
 */
function StartScreen() {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const window = useWindow()
  const sessions = useSessions()
  const [recent] = useLocal(RECENT, NONE)
  const animate = useAnimate()
  const shown = useSession()
  // The sessions shown last first, then the rest, without the one this window shows.
  const others = useMemo(() => {
    const byId = new Map(sessions.map((s) => [s.workspace_id, s]))
    const ids = [...recent, ...sessions.map((s) => s.workspace_id)].filter((id, i, all) => byId.has(id) && id !== window.session && all.indexOf(id) === i)
    return ids.slice(0, START_RECENT).map((id) => byId.get(id)!)
  }, [sessions, recent, window.session])
  const command = (name: string) => () => runCommand(name, window.id)
  const starters = useStarters()
  const appIcon = assets.appIcon()
  // The first run's choices take the window: the side bar is put away while they show, and the
  // screen widens to four cards to a row.
  const setup = !!starters.offered?.length
  const setSetup = useContext(SetupContext)
  useEffect(() => {
    setSetup(setup)
    return () => setSetup(false)
  }, [setup, setSetup])
  const logo = (size: number) =>
    appIcon ? (
      <img src={appIcon} objectFit="contain" alt="pumi" style={{ width: size, height: size, flexShrink: 0 }} />
    ) : (
      <div style={{ display: "flex", width: size, height: size, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: m.radius + 8, backgroundColor: t.accent }}>
        <Icon name="pumi" size={Math.round(size * 0.6)} color={t.accentText} />
      </div>
    )
  // The first run's choices are the screen, under a welcome: there is nothing to open, or to go back to, before them.
  const body = setup ? (
    <>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 16, paddingLeft: 10 }}>
        {logo(56)}
        <div style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
          <text style={{ fontSize: ts(22), lineHeight: ts(26), fontWeight: 600, color: t.text }}>Welcome to Pumi!</text>
          <text style={{ fontSize: ts(13), lineHeight: ts(18), color: t.muted }}>Make it yours.</text>
        </div>
      </div>
      <StartPlugins starters={starters} />
    </>
  ) : (
    <>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 10 }}>
        {logo(72)}
        <text style={{ fontSize: ts(20), fontWeight: 600, color: t.text, marginTop: 4 }}>{shown ? shown.label || "pumi" : "Welcome to pumi"}</text>
        <text style={{ fontSize: ts(12), color: t.muted, textAlign: "center" }}>
          {shown ? (shown.cwd ? tildePath(shown.cwd) : "Nothing is open in this session.") : "A terminal workspace for your coding agents."}
        </text>
      </div>
      <StartSection title="Start">
        {shown ? <StartRow icon="terminal" title="New View" command="new_view" onClick={command("new_view")} /> : null}
        <StartRow icon={shown ? "square_plus" : "terminal"} title="New Session" command="new_workspace" onClick={command("new_workspace")} />
        <StartRow icon="folder_open" title="Open Folder…" onClick={() => openFolder(window).catch(swallow)} />
      </StartSection>
      {others.length ? (
        <StartSection title="Recent">
          {others.map((s) => (
            <StartRow key={s.workspace_id} icon="folder" title={s.label || s.workspace_id} detail={s.cwd ? tildePath(s.cwd) : undefined} onClick={() => window.show(s.workspace_id)} />
          ))}
        </StartSection>
      ) : null}
      <div style={{ display: "flex", flexDirection: "column", borderTopWidth: 1, borderColor: t.border, paddingTop: 12 }}>
        <StartRow icon="settings" title="Settings" command="settings" quiet onClick={command("settings")} />
      </div>
    </>
  )
  return (
    // Centred while it fits; taller than the room, it starts at the top and scrolls.
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0, minHeight: 0, overflowY: "scroll" }}>
      <div style={{ display: "flex", flexShrink: 0, minHeight: "100%", alignItems: "center", justifyContent: "center" }}>
        <motion.div
          initial={animate ? { opacity: 0, y: 8 } : false}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.25, ease: "easeOut" }}
          style={{ display: "flex", flexDirection: "column", width: setup ? SETUP_WIDTH : START_WIDTH, flexShrink: 1, minWidth: 0, paddingLeft: 24, paddingRight: 24, paddingTop: 32, paddingBottom: 32, gap: 28 }}
        >
          {body}
        </motion.div>
      </div>
    </div>
  )
}

/**
 * The first run's choices. Plugins: pumi opens nothing by itself, so it offers Launcher, Terminal,
 * Agents and Browser, as cards with a picture of each, each ticked, to install at once (with Agents,
 * whether agents open in a chat or their terminal). New tabs then open the first installed
 * (`starters.ts`). Look: a transparent window or not, and the colour scheme, set as picked.
 */
function StartPlugins({ starters }: { starters: Starters }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const { offered = [], picked, toggle, busy, error, install, agents, setAgents } = starters
  const count = offered.filter((s) => picked.has(s.name)).length
  const installing = offered.find((s) => s.name === busy)
  const button = (label: string, primary: boolean, enabled: boolean, onClick: () => void) => (
    <div
      role="button"
      aria-label={label}
      tabIndex={enabled ? 0 : -1}
      onClick={() => enabled && onClick()}
      onKeyDown={(e: any) => {
        if (!enabled || (e.key !== "enter" && e.key !== " ")) return
        e.preventDefault?.()
        onClick()
      }}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: 28,
        paddingLeft: 12,
        paddingRight: 12,
        borderRadius: m.radiusSmall,
        cursor: enabled ? "pointer" : "default",
        opacity: enabled ? 1 : 0.5,
        backgroundColor: primary ? t.accent : "transparent",
        ...(enabled ? { hover: { backgroundColor: primary ? withOpacity(t.accent, 0.85) : t.hover } } : {}),
      }}
    >
      <text style={{ fontSize: ts(13), color: primary ? t.accentText : t.muted, whiteSpace: "nowrap" }}>{label}</text>
    </div>
  )
  const label = (text: string, first?: boolean) => <text style={{ fontSize: ts(12), color: t.muted, paddingLeft: 10, paddingTop: first ? 0 : 16, paddingBottom: 8 }}>{text}</text>
  return (
    <>
      <StartSection title="Plugins">
        {label("pumi opens what its plugins bring. Pick the ones to install.", true)}
        <PluginCards offered={offered} picked={picked} busy={busy} onToggle={toggle} />
        {picked.has("agents") ? (
          <>
            {label("Open agents in")}
            <AgentsChoice value={agents} onChange={setAgents} />
          </>
        ) : null}
      </StartSection>
      <StartSection title="Look">
        {label("Window", true)}
        <WindowChoice />
        {label("Color scheme")}
        <SchemeChoice />
      </StartSection>
      <div style={{ display: "flex", flexDirection: "column", marginTop: -12 }}>
        {error ? <text style={{ fontSize: ts(12), color: t.error, paddingLeft: 10, paddingBottom: 8 }}>{error}</text> : null}
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "flex-end", gap: 6 }}>
          {button(installing ? `Installing ${installing.title}…` : count ? `Install ${count} ${count === 1 ? "plugin" : "plugins"}` : "Install", true, !busy && count > 0, install)}
        </div>
      </div>
    </>
  )
}

/** A group of the start screen's rows, under its title in small capitals. */
function StartSection({ title, children }: { title: string; children: ReactNode }) {
  const ts = useTextSize()
  const t = useTheme()
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
      <text style={{ fontSize: ts(11), fontWeight: 600, color: t.faint, paddingLeft: 10, paddingBottom: 6 }}>{title.toUpperCase()}</text>
      {children}
    </div>
  )
}

/**
 * A row of the start screen: its icon, its title (and a faint `detail` after it), and at the far
 * end the keys of its `command`. `quiet`: drawn in the muted colour until the mouse is over it.
 */
function StartRow({ icon, title, detail, command, quiet, onClick }: { icon: string; title: string; detail?: string; command?: string; quiet?: boolean; onClick(): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const keys = command ? shortcut(command) : undefined
  return (
    <div
      aria-label={title}
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e: any) => {
        if (e.key !== "enter" && e.key !== " ") return
        e.preventDefault?.()
        onClick()
      }}
      style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10, height: 32, flexShrink: 0, paddingLeft: 10, paddingRight: 8, borderRadius: m.radiusSmall, cursor: "pointer", color: quiet ? t.muted : t.text, hover: { backgroundColor: t.hover, color: t.text }, active: { backgroundColor: t.active } }}
    >
      <Icon name={icon} size={15} color={quiet ? t.faint : t.accent} />
      <text style={{ fontSize: ts(13), whiteSpace: "nowrap", flexShrink: 0 }}>{title}</text>
      {detail ? <text style={{ fontSize: ts(12), color: t.faint, whiteSpace: "nowrap", textOverflow: "ellipsis", minWidth: 0, flexShrink: 1 }}>{detail}</text> : null}
      <div style={{ flexGrow: 1 }} />
      {keys ? <Keycaps keys={keys} /> : null}
    </div>
  )
}

/** A shortcut as a Mac menu writes it (`⇧ ⌘ D`), each key on a cap of its own; of several bindings, the first. */
function Keycaps({ keys }: { keys: string }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const first = keys.split("   ")[0]!.split(" ")
  return (
    <div style={{ display: "flex", flexDirection: "row", gap: 3, flexShrink: 0 }}>
      {first.map((k, i) => (
        <div key={i} style={{ display: "flex", minWidth: 20, height: 20, paddingLeft: 5, paddingRight: 5, alignItems: "center", justifyContent: "center", borderRadius: m.radiusSmall, borderWidth: 1, borderColor: t.border, backgroundColor: t.surface }}>
          <text style={{ fontSize: ts(11), color: t.muted }}>{k}</text>
        </div>
      ))}
    </div>
  )
}

/**
 * How long the modifiers are held before the keys show beside what they do: a quick ⌘C shows
 * nothing. Let go, and they are gone at once.
 */
const HELD_AFTER = 300

/** A window's held modifiers once they have been held `HELD_AFTER`: one timer per window, however many rows read it. */
const settled = new WeakMap<HeldModifiers, { get(): string; subscribe(listener: () => void): () => void }>()
const NOT_HELD = { get: () => "", subscribe: () => () => {} }

function settledHeld(held: HeldModifiers | undefined) {
  if (!held) return NOT_HELD
  let store = settled.get(held)
  if (store) return store
  let now = ""
  let timer: ReturnType<typeof setTimeout> | undefined
  const listeners = new Set<() => void>()
  const publish = (next: string) => {
    if (next === now) return
    now = next
    for (const listener of listeners) listener()
  }
  held.subscribe(() => {
    clearTimeout(timer)
    const next = held.get()
    // Down to none, or to another set while they show (⇧ added to ⌘): at once.
    if (!next || now) publish(next)
    else timer = setTimeout(() => publish(held.get()), HELD_AFTER)
  })
  store = {
    get: () => now,
    subscribe: (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
  }
  settled.set(held, store)
  return store
}

/** The keys of `command` (of a range's `index`) while what they need is held down, else nothing. */
function useHeldKeys(command: string, index?: number): string | undefined {
  const store = settledHeld(useWindow().held)
  const held = useSyncExternalStore(store.subscribe, store.get)
  return heldShortcut(held, command, index)
}

/** Keys shown while they are held (`⌘1`), on one cap at a row's far end. */
function HeldKeys({ keys }: { keys: string }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  return (
    <div aria-label={keys} style={{ display: "flex", height: 20, minWidth: 28, flexShrink: 0, paddingLeft: 6, paddingRight: 6, alignItems: "center", justifyContent: "center", borderRadius: m.radiusSmall + 1, borderWidth: 1, borderColor: t.border }}>
      {/* `⌘ 1` as one glyph run, `⌘1`, as a cap that small has it. */}
      <text style={{ fontSize: ts(11), color: t.muted, whiteSpace: "nowrap" }}>{keys.split(" ").join("")}</text>
    </div>
  )
}

/**
 * A view's tiled panes; a zoom grows the pane over the others, and shrinks it back. A press
 * beside the zoomed pane shrinks it back too.
 */
function Tiles({ tree, view, focused, zoomed }: { tree: Tree; view?: string; /** The tab with the focus. */ focused?: string; /** The pane zoomed. */ zoomed?: string }) {
  const motion = useMemo<TileMotion>(
    () => ({
      zoom: zoomed,
      unzoom: zoomed ? () => void call(via(zoomed, "pane.update"), zoomed, { zoom: false }).catch(swallow) : undefined,
    }),
    [zoomed],
  )
  return (
    <TileMotion.Provider value={motion}>
      <Split tree={tree} view={view} focused={focused} />
    </TileMotion.Provider>
  )
}

/**
 * A node of a view's layout: a pane, or a split of two. `view` is the view it belongs to, whose
 * layout a dragged edge moves; without it the split is drawn but its edges do nothing.
 */
function Split({ tree, view, focused }: { tree: Tree; view?: string; focused?: string }) {
  if ("app" in tree) return null
  return <Tiling tree={tree} focused={focused} edit={editOf(view)} />
}

/**
 * A tiled pane's button to fill its view, or to give the view back its layout, shown while the
 * mouse is over the pane (`over`) and it has no bar. Nothing while the view has one pane.
 */
function ZoomButton({ pane: id, zoomed, over }: { pane: string; zoomed: boolean; over: boolean }) {
  const t = useTheme()
  const m = useMetrics()
  const tree = useLayout()
  // The button covers the pane, so the pane is not under the mouse while the button is.
  const [onIt, setOnIt] = useState(false)
  if (!(zoomed || (tree && "axis" in tree)) || !(over || onIt)) return null
  const title = zoomed ? "Restore the layout" : "Fill the view"
  return (
    <div
      aria-label={title}
      title={title}
      onMouseEnter={() => setOnIt(true)}
      onMouseLeave={() => setOnIt(false)}
      onClick={() => {
        // The host gives the pane the focus as it zooms it.
        setOnIt(false)
        call(via(id, "pane.update"), id, { zoom: !zoomed }).catch(swallow)
      }}
      style={{
        position: "absolute",
        top: 4,
        right: 4,
        display: "flex",
        width: 22,
        height: 22,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: m.radiusSmall,
        backgroundColor: t.surface,
        borderWidth: 1,
        borderColor: t.border,
        color: t.muted,
        cursor: "pointer",
        opacity: onIt ? 1 : 0.75,
        hover: { backgroundColor: t.hover, color: t.text },
      }}
    >
      <Icon name={zoomed ? "minimize" : "maximize"} size={12} />
    </div>
  )
}

/** How wide a tab in a pane's strip is at most, and at least before the strip scrolls. */
const PANE_TAB_MAX = 320
const PANE_TAB_MIN = 72

/**
 * A tiled pane's bar, as Zed's. While panes hold tabs (`pane_tabs`), the strip of its tabs (each
 * a `PaneTab`; a tab dropped on it goes in there); else its one tab's title, which drags the
 * tab. Then what plugins put at
 * `tab.toolbar` (the front tab's), and its buttons: a new tab (while panes hold tabs), a split
 * (`SplitButton`: a menu of the four sides), a zoom (while the view has other panes, or it is zoomed) and a close:
 * the pane's with a strip (`closePane`), else its one tab's. The buttons show on the focused pane and the one under the mouse (`over`); with a
 * strip they are a section of their own at the bar's end, an edge before them, which the strip
 * never pushes out or covers. A double click on the bar outside the strip zooms the pane. In the
 * title bar's row (a top pane's, beside the side bar), the strip's empty end and the room round
 * the buttons move the window.
 */
function PaneTitle({ pane: id, tabs, active, focused, zoomed, over, menu }: { pane: string; tabs: string[]; active?: string | null; focused: boolean; zoomed: boolean; over?: boolean; menu(e: { x?: number; y?: number }): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const C = useComponents()
  const strip = usePaneTabs()
  const front = active ?? tabs[0]
  const tab = useTab(front)
  const tree = useLayout(tab?.view_id)
  const draggable = useDraggable<moves.Carried>({ kind: "pumi.tab", tab: front ?? "" })
  const toolbar = useMemo(() => ({ tab: front }), [front])
  // With the side bar hidden, the title bar is the header over the panes instead.
  const [sidebarShown] = useSidebar()
  const inTitleBar = useContext(AtTop) && sidebarShown
  const [splitting, setSplitting] = useState(false)
  const [adding, setAdding] = useState(false)
  // The bar and the buttons' section each stop the mouse, so each tells the pane it is over it.
  const inBar = usePaneHoverPart("bar")
  const inButtons = usePaneHoverPart("buttons")
  // On the focused pane the buttons are as bright as its title.
  const color = focused ? t.text : undefined
  if (!tab || !front) return null
  const zoomable = zoomed || (!!tree && "axis" in tree)
  // A split holds what a new tab does (`new_tab`: the Launcher's list, a shell, …).
  const split = (side: SplitSide) => {
    call(via(id, "tab.open"), null, {}, { kind: "tiled", pane: id, split: side }).catch(swallow)
  }
  const zoom = () => call(via(id, "pane.update"), id, { zoom: !zoomed }).catch(swallow)
  const close = () => closePane(id, tabs).catch(swallow)
  const bar = {
    display: "flex",
    flexDirection: "row",
    alignItems: "center",
    // As tall as the title bar: the top panes' bars sit in its row, beside the side bar's top.
    height: TITLE_BAR,
    flexShrink: 0,
    userSelect: "none",
    borderBottomWidth: 1,
    borderColor: t.border,
  } as const
  const shown = focused || !!over || splitting || adding
  const buttons = (
    // Hidden, not removed, so the bar's other contents do not move as the mouse comes and goes.
    // Kept shown while the split menu or the new tab menu is open, or the menu would go with them.
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2, flexShrink: 0, paddingLeft: 4, paddingRight: 4, opacity: shown ? 1 : 0 }}>
      {strip ? <NewTabButton pane={id} color={color} onOpen={setAdding} /> : null}
      <SplitButton color={color} split={split} onOpen={setSplitting} />
      {zoomable ? <IconButton icon={zoomed ? "minimize" : "maximize"} title={zoomed ? "Restore the layout" : "Fill the view"} color={color} onClick={zoom} /> : null}
      {strip ? <IconButton icon="close" title="Close Pane" color={color} onClick={close} /> : <IconButton icon="close" title="Close Tab" color={color} onClick={() => call(via(front, "tab.close"), front).catch(swallow)} />}
    </div>
  )
  if (strip) {
    return (
      <div
        {...inBar}
        onAuxClick={(e: any) => {
          // The front tab's menu, from the bar itself: the pane around it would open it again.
          e?.stopPropagation?.()
          menu(e)
        }}
        // No bottom edge of its own: each tab but the front one draws its own, and so does the
        // rest of the bar, so the front tab opens into its body as Zed's does.
        style={{ ...bar, alignItems: "stretch", borderBottomWidth: 0, backgroundColor: t.surface }}
      >
        <div style={{ display: "flex", flexDirection: "row", alignItems: "stretch", alignSelf: "stretch", minWidth: 0, flexShrink: 1, overflowX: "scroll" }}>
          {tabs.map((tab, index) => (
            <C.PaneTab key={tab} tab={tab} pane={id} index={index} active={tab === front} focused={focused} />
          ))}
        </div>
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", alignSelf: "stretch", flexGrow: 1, minWidth: 0, borderBottomWidth: 1, borderColor: t.border, appRegion: inTitleBar ? "drag" : undefined }}>
          {/* The strip's empty end: a tab dropped there goes last. */}
          <StripEnd pane={id} tabs={tabs} />
        </div>
        {/* The buttons' own section, there only while they are shown: it takes the strip's empty
            end, so the tabs move only when the strip is full (then it scrolls, never covered). */}
        {shown ? (
          <div
            {...inButtons}
            style={{ display: "flex", flexDirection: "row", alignItems: "center", alignSelf: "stretch", flexShrink: 0, gap: 2, paddingLeft: 4, paddingRight: 4, borderLeftWidth: 1, borderBottomWidth: 1, borderColor: t.border, appRegion: inTitleBar ? "drag" : undefined }}
          >
            <Slot slot="tab.toolbar" args={toolbar} gap={2} />
            {buttons}
          </div>
        ) : null}
      </div>
    )
  }
  return (
    <div
      {...draggable.props}
      {...inBar}
      onDoubleClick={() => zoomable && zoom()}
      onAuxClick={(e: any) => {
        // The tab's menu, from the bar itself: the pane around it would open it again.
        e?.stopPropagation?.()
        menu(e)
      }}
      title={tab.cwd ?? undefined}
      style={{ ...bar, gap: 6, paddingLeft: 10, opacity: draggable.dragging ? 0.55 : 1, cursor: draggable.dragging ? "grabbing" : "default" }}
    >
      <TabIcon tab={tab.tab_id} size={12} color={t.faint} />
      <text style={{ fontSize: ts(12), color: focused ? t.text : t.muted, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{tabLabel(tab)}</text>
      <div style={{ flexGrow: 1 }} />
      {/* A plugin's buttons sit with the pane's own, spaced as they are. */}
      <Slot slot="tab.toolbar" args={toolbar} gap={2} />
      {buttons}
    </div>
  )
}

/**
 * Closes a pane and its tabs: at once, unless a tab in them is working (`@status`), when it asks
 * first (naming the one, or counting them).
 */
async function closePane(pane: string, tabs: string[]) {
  const working = (store.get().snapshot?.tabs ?? []).filter((p) => p.status === "working" && tabs.includes(p.tab_id))
  if (working.length) {
    const one = working.length === 1 ? working[0]! : undefined
    const yes = await confirm({
      title: one ? `${tabLabel(one)} is still working` : `${working.length} tabs are still working`,
      detail: "Closing the pane stops them and closes its tabs.",
      confirm: "Close Pane",
      danger: true,
    })
    if (!yes) return
  }
  await call(via(pane, "pane.close"), pane)
}

type SplitSide = "right" | "left" | "up" | "down"

/** The split menu's rows, and the command whose keys each shows (the ones that have one). */
const SPLITS: { side: SplitSide; title: string; command?: string }[] = [
  { side: "right", title: "Split Right", command: "split_vertical" },
  { side: "left", title: "Split Left" },
  { side: "up", title: "Split Up" },
  { side: "down", title: "Split Down", command: "split_horizontal" },
]

/**
 * A pane bar's split button, as Zed's: a press opens a menu under it of the four sides to split
 * the pane to. A press on the button while it is open closes it.
 */
function SplitButton({ color, split, onOpen }: { color?: string; split(side: SplitSide): void; onOpen(open: boolean): void }) {
  const [open, setOpenState] = useState(false)
  // The press that closes it from the button is outside the card; the button's own click then
  // toggles it, so that press must not close it first.
  const onButton = useRef(false)
  const setOpen = (now: boolean) => {
    setOpenState(now)
    onOpen(now)
  }
  return (
    <div style={{ display: "flex", position: "relative", flexShrink: 0 }}>
      <div onMouseEnter={() => (onButton.current = true)} onMouseLeave={() => (onButton.current = false)} style={{ display: "flex" }}>
        <IconButton icon="split" title="Split Pane" active={open} color={color} onClick={() => setOpen(!open)} />
      </div>
      {open ? <SplitMenu split={split} close={() => setOpen(false)} onButton={onButton} /> : null}
    </div>
  )
}

function SplitMenu({ split, close, onButton }: { split(side: SplitSide): void; close(): void; onButton: { current: boolean } }) {
  const t = useTheme()
  const m = useMetrics()
  return (
    <anchored side="bottom" align="end" gap={4} fit="snap" snapMargin={8} deferred occlude priority={1} style={{ backgroundColor: "transparent" }}>
      <div onMouseDownOutside={() => onButton.current || close()} style={menuCard(t, m)}>
        {SPLITS.map((s) => (
          <MenuItem
            key={s.side}
            title={s.title}
            keys={s.command ? shortcut(s.command) : undefined}
            onClick={() => {
              close()
              split(s.side)
            }}
          />
        ))}
      </div>
    </anchored>
  )
}

/** Where a new tab goes: into a pane, as one of its tabs. */
type NewPlace = { kind: "tiled"; pane: string }

/**
 * One thing a new tab can be: a row of the `+` menu. `at` is what the call goes through (the
 * pane: it names the server).
 */
interface NewKind {
  id: string
  title: string
  icon: string
  /** The keys that open one elsewhere, when they are bound: the app's command, or the server's. */
  keys?: string
  /** A menu draws a rule between groups: the app's own, then each plugin's (`tab.new`). */
  group: string
  open(at: string, place: NewPlace): Promise<unknown>
}

/** The keys the server's tables (`keys`) bind to `fn`, as the OS writes them (`⇧ ⌘ E`). */
function serverShortcut(tables: unknown, fn: string): string | undefined {
  if (!tables || typeof tables !== "object") return undefined
  for (const name of ["root", "global"]) {
    const table = (tables as Record<string, unknown>)[name]
    if (!table || typeof table !== "object") continue
    for (const [key, binding] of Object.entries(table as Record<string, { call?: unknown } | null>)) {
      if (binding?.call === fn) return keyGlyphs(key.split(" ").map((k) => fromServer(k) ?? k).join(" "))
    }
  }
  return undefined
}

/**
 * What a new tab can be: what the plugins add to `tab.new` (a terminal, a file, an image, a page,
 * the agents), each plugin's a group of its own. A plugin's action is run with `{tab, place}`: the
 * pane or tab it opens beside (it names the server), and where; one with a `pick` asks for its
 * files first. Each shows the keys that open one elsewhere when there are any.
 */
function useNewKinds(): NewKind[] {
  const { actions } = useSlot("tab.new")
  const tables = useApp((s) => s.options.keys)
  return useMemo(() => {
    const out: NewKind[] = []
    actions.forEach((a) => {
      out.push({
        id: `${a.plugin}/${a.id}`,
        title: a.title,
        icon: a.icon ?? "plus",
        keys: a.keys ?? (a.command ? serverShortcut(tables, a.command) : undefined),
        group: a.plugin,
        open: async (on, place) => a.run({ tab: on, place }),
      })
    })
    return out
  }, [actions, tables])
}

/**
 * A pane bar's `+`: a press opens a menu under it of what the new tab can be (`useNewKinds`),
 * which opens into the pane as one of its tabs. A press on the button while it is open closes it.
 */
function NewTabButton({ pane, color, onOpen }: { pane: string; color?: string; onOpen(open: boolean): void }) {
  const [open, setOpenState] = useState(false)
  // As the split button's: the press that closes it from the button is not outside it.
  const onButton = useRef(false)
  const setOpen = (now: boolean) => {
    setOpenState(now)
    onOpen(now)
  }
  return (
    <div style={{ display: "flex", position: "relative", flexShrink: 0 }}>
      <div onMouseEnter={() => (onButton.current = true)} onMouseLeave={() => (onButton.current = false)} style={{ display: "flex" }}>
        <IconButton icon="plus" title="New Tab" active={open} color={color} onClick={() => setOpen(!open)} />
      </div>
      {open ? <NewTabMenu pane={pane} close={() => setOpen(false)} onButton={onButton} /> : null}
    </div>
  )
}

function NewTabMenu({ pane, close, onButton }: { pane: string; close(): void; onButton: { current: boolean } }) {
  const t = useTheme()
  const m = useMetrics()
  const kinds = useNewKinds()
  return (
    <anchored side="bottom" align="end" gap={4} fit="snap" snapMargin={8} deferred occlude priority={1} style={{ backgroundColor: "transparent" }}>
      <div onMouseDownOutside={() => onButton.current || close()} style={menuCard(t, m)}>
        {kinds.map((k, i) => (
          <Fragment key={k.id}>
            {i > 0 && kinds[i - 1]!.group !== k.group ? <MenuRule /> : null}
            <MenuItem
              icon={k.icon}
              title={k.title}
              keys={k.keys}
              onClick={() => {
                close()
                k.open(pane, { kind: "tiled", pane }).catch(swallow)
              }}
            />
          </Fragment>
        ))}
        {kinds.length ? null : (
          // Nothing is installed that opens a tab: the way to what does.
          <MenuItem
            icon="puzzle"
            title="Get Plugins…"
            onClick={() => {
              close()
              openSettings("store")
            }}
          />
        )}
      </div>
    </anchored>
  )
}

/** Whether a drag carries a tab that could go into `pane`'s strip at some index. */
const intoStrip = (pane: string, tabs: string[]) => (data: unknown): data is moves.Carried => {
  const it = moves.carried(data)
  if (!it) return false
  const leaf = { pane, tabs }
  return [...tabs.keys(), tabs.length].some((i) => moves.stripDrop(it, leaf, i) !== undefined)
}

/** The rest of a pane's bar after its strip: a tab dropped there goes last. */
function StripEnd({ pane, tabs }: { pane: string; tabs: string[] }) {
  const t = useTheme()
  const zone = drop.useDropBox(intoStrip(pane, tabs), (it) => {
    const m = moves.stripDrop(it, { pane, tabs }, tabs.length)
    if (m) runMove(m)
  })
  const lit = zone.isOver && zone.drag && moves.carried(zone.drag.data) && moves.stripDrop(moves.carried(zone.drag.data)!, { pane, tabs }, tabs.length)
  return (
    <div
      ref={zone.ref}
      // No room of its own past the strip: a full strip runs up to the buttons, and a tab dropped
      // on the last one's right half goes last all the same.
      style={{ display: "flex", flexGrow: 1, alignSelf: "stretch", minWidth: 0, borderLeftWidth: lit ? 2 : 0, borderColor: t.accent }}
    />
  )
}

/**
 * The `PaneTab` part: one tab in a pane's strip, as Zed's. Its icon (`TabIcon`: its status, or
 * its own icon), its title, then dimmer where it is (its directory) when that says more than its
 * title does or another tab in the strip has the same title, and a close, shown on the front tab
 * and the one under the mouse (its room kept, so the tab does not change width). A press
 * brings it to the front and gives its pane the focus; a right press opens its menu. It drags:
 * along its strip, into another pane's (or onto a side of a pane, for a pane of its own), or
 * onto a row of the side bar. A tab dropped on it goes before it, or after it past its middle.
 * The front tab is the pane's colour and has no bottom edge: it opens into its pane.
 */
function PaneTab({ tab: id, pane, index, active, focused }: { tab: string; pane: string; index: number; active: boolean; focused: boolean }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const frontColor = t.editor
  const tab = useTab(id)
  const tree = useLayout(tab?.view_id)
  const [hovered, hover] = useHover()
  // The front tab's background stops the mouse there: it tells the pane it is over it itself.
  const inPane = usePaneHoverPart(`tab:${id}`)
  const draggable = useDraggable<moves.Carried>({ kind: "pumi.tab", tab: id })
  const { open, menu } = useMenu("tab.menu", { tab: id })
  const leaf = { pane, tabs: leafOf(tree, pane)?.tabs ?? [id] }
  // Whether another tab of the strip goes by the same title: then where each is tells them apart.
  const twin = useApp((s) => {
    const all = s.snapshot?.tabs ?? []
    const me = all.find((t) => t.tab_id === id)
    if (!me) return false
    const title = tabLabel(me)
    return leaf.tabs.some((other) => {
      const it = other !== id && all.find((t) => t.tab_id === other)
      return !!it && tabLabel(it) === title
    })
  })
  const zone = drop.useDropBox(intoStrip(pane, leaf.tabs), (it, at, box) => {
    const m = moves.stripDrop(it, leaf, firstHalf(at, box, true) ? index : index + 1)
    if (m) runMove(m)
  })
  if (!tab) return null
  const over = zone.isOver && zone.drag ? firstHalf(zone.drag, zone.box(), true) : undefined
  const carried = zone.drag ? moves.carried(zone.drag.data) : undefined
  const lit = over !== undefined && carried && moves.stripDrop(carried, leaf, over ? index : index + 1) ? (over ? "left" : "right") : undefined
  const title = tabLabel(tab)
  const where = tab.cwd ? tildePath(tab.cwd) : undefined
  // A terminal named after its directory says where it is already, unless a twin needs telling apart.
  const detail = where && (twin || (title !== basename(tab.cwd!) && where !== title)) ? where : undefined
  return (
    <div
      ref={zone.ref}
      {...draggable.props}
      onMouseEnter={() => {
        hover.onMouseEnter()
        inPane.onMouseEnter()
      }}
      onMouseLeave={() => {
        hover.onMouseLeave()
        inPane.onMouseLeave()
      }}
      onMouseDown={(e: any) => {
        draggable.props.onMouseDown(e)
        if ((e?.button ?? 0) !== 0) return
        if (!active || !focused) call(via(id, "tab.focus"), id).catch(swallow)
      }}
      onAuxClick={(e: any) => {
        e?.stopPropagation?.()
        open(e)
      }}
      title={tab.cwd ?? undefined}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        // The strip's whole height (a child of a scrolling row does not stretch on its own).
        alignSelf: "stretch",
        height: "100%",
        gap: 8,
        paddingLeft: 14,
        paddingRight: 8,
        maxWidth: PANE_TAB_MAX,
        minWidth: PANE_TAB_MIN,
        flexShrink: 1,
        cursor: draggable.dragging ? "grabbing" : "pointer",
        opacity: draggable.dragging ? 0.55 : 1,
        position: "relative",
        // An edge either side (the first tab's left one too) and, but on the front tab, under it.
        borderLeftWidth: lit === "left" ? 2 : index === 0 ? 1 : 0,
        borderRightWidth: 1,
        borderBottomWidth: active ? 0 : 1,
        borderColor: lit ? t.accent : t.border,
        // The front tab is its body's colour; the others are the bar's.
        backgroundColor: active ? frontColor : undefined,
        color: active ? t.text : t.muted,
        hover: { backgroundColor: active ? frontColor : t.hover, color: t.text },
      }}
    >
      <TabIcon tab={id} size={14} color={active ? t.muted : t.faint} />
      <text style={{ fontSize: ts(13), whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{title}</text>
      {detail ? <text style={{ fontSize: ts(12), color: t.faint, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 2, minWidth: 0 }}>{detail}</text> : null}
      <div
        aria-label="Close Tab"
        title="Close Tab"
        onMouseDown={(e: any) => e?.stopPropagation?.()}
        onClick={(e: any) => {
          e?.stopPropagation?.()
          call(via(id, "tab.close"), id).catch(swallow)
        }}
        style={{ display: "flex", width: 18, height: 18, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: m.radiusSmall, opacity: active || hovered ? 1 : 0, hover: { backgroundColor: t.active, opacity: 1 } }}
      >
        <Icon name="close" size={10} />
      </div>
      {lit === "right" ? <div style={{ position: "absolute", top: 0, right: -1, width: 2, height: "100%", backgroundColor: t.accent, pointerEvents: "none" }} /> : null}
      {menu}
    </div>
  )
}

/** A file tab no plugin draws (none fits its file, or none is enabled): the file, and a way out. */
function FilePlaceholder({ tab, path }: { tab: string; path: string }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const [hover, setHover] = useState(false)
  const name = basename(path) || path
  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, alignItems: "center", justifyContent: "center", gap: 8, padding: 16 }}>
      <text style={{ color: t.text }}>{name}</text>
      <text style={{ color: t.faint, fontSize: ts(11) }}>{path}</text>
      <text style={{ color: t.faint, fontSize: ts(11) }}>No plugin draws this file.</text>
      <div
        onClick={() => call(via(tab, "fs.open"), path).catch(swallow)}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
        style={{ paddingTop: 2, paddingBottom: 2, paddingLeft: 8, paddingRight: 8, borderRadius: m.radiusSmall, backgroundColor: hover ? `${t.accent}22` : "#00000000" }}
      >
        <text style={{ color: t.accent, fontSize: ts(11) }}>Open in its default app</text>
      </div>
    </div>
  )
}

/**
 * The `Pane` part: a pane's bar (a tiled one's, while bars show; its strip while panes hold
 * tabs), the body of the tab in front, and that tab's menu on a right press. Only the front tab's
 * body is drawn: the others keep running, sized as the pane is. Its frame is the layout's
 * (`ThemeSpec.layout`); a lifted one (zoomed) is a card, its strip still on top.
 */
function Pane({ pane: id, tabs, active, focused, zoomed }: { pane: string; tabs: string[]; active?: string | null; focused: boolean; zoomed: boolean; lifted?: boolean }) {
  const C = useComponents()
  const front = active ?? tabs[0] ?? id
  // A closed tab is still drawn while its pane shrinks away: as what it was, not as a terminal.
  const live = useTab(front)
  const last = useRef(live)
  if (live) last.current = live
  const tab = live ?? last.current
  const [titles] = usePaneTitles()
  const strip = usePaneTabs()
  const followMouse = useFocusOnHover(live ? front : undefined, focused)
  const follow = useRef(followMouse)
  follow.current = followMouse
  // Under the mouse: the pane, or any part of its bar (`PaneHover`).
  const [parts, setParts] = useState<ReadonlySet<string>>(NO_PARTS)
  const hoverPart = useCallback((part: string, on: boolean) => {
    if (on) follow.current()
    setParts((p) => (p.has(part) === on ? p : on ? new Set(p).add(part) : new Set([...p].filter((k) => k !== part))))
  }, [])
  const over = parts.size > 0
  const [outline] = useOutlineFocused()
  const tree = useLayout(tab?.view_id)
  const t = useTheme()
  const toolbar = useSlot("tab.toolbar")
  const { open, menu } = useMenu("tab.menu", { tab: front })
  // A float draws its own frame (or none): the bar is a tiled pane's.
  const kind = (tab?.place as { kind?: string } | undefined)?.kind
  const tiled = kind === undefined || kind === "tiled"
  // A pane of several tabs shows its strip, bars or not: it is the only way to its other tabs.
  const titled = !!live && tiled && (titles || (strip && tabs.length > 1))
  // The focused pane's outline, while its view has others to tell it from.
  const outlined = outline && focused && !!live && !!tree && "axis" in tree
  // The tab is in front of the user: what its program left for them (done, error, a report) is seen.
  useEffect(() => {
    if (focused && live) call(via(front, "tab.update"), front, { seen: true }).catch(swallow)
  }, [front, focused])
  return (
    <div
      onMouseDown={() => {
        if (!focused && live) call(via(front, "tab.focus"), front).catch(swallow)
      }}
      onAuxClick={open}
      onMouseEnter={() => hoverPart("pane", true)}
      onMouseLeave={() => hoverPart("pane", false)}
      style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0, minHeight: 0, position: "relative" }}
    >
      {titled ? (
        <PaneHover.Provider value={hoverPart}>
          <C.PaneTitle pane={id} tabs={tabs} active={front} focused={focused} zoomed={zoomed} over={over} menu={open} />
        </PaneHover.Provider>
      ) : toolbar.components.length || toolbar.actions.length ? (
        <div style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", height: 24, flexShrink: 0, paddingRight: 4 }}>
          <Slot slot="tab.toolbar" args={{ tab: front }} gap={6} />
        </div>
      ) : null}
      {/* Keyed by the tab: another tab's body is drawn as itself, not as this one's. */}
      <C.TabBody key={front} tab={front} focused={focused} />
      {titled || !tiled ? null : <ZoomButton pane={id} zoomed={zoomed} over={over} />}
      {outlined ? <div style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", borderWidth: 1, borderColor: withOpacity(t.accent, 0.7), pointerEvents: "none" }} /> : null}
      {menu}
    </div>
  )
}

const NO_PARTS: ReadonlySet<string> = new Set()

/**
 * Tells a pane the mouse is over a part of its bar. A part with a background of its own (the bar,
 * the front tab, the buttons' card) stops the mouse there, so the pane around it no longer counts
 * as under it: each such part says so itself (`usePaneHoverPart`).
 */
const PaneHover = createContext<((part: string, on: boolean) => void) | undefined>(undefined)

/** The mouse props that tell the pane around it the mouse is over `part`; gone, it is not. */
function usePaneHoverPart(part: string): { onMouseEnter(): void; onMouseLeave(): void } {
  const hover = useContext(PaneHover)
  useEffect(() => () => hover?.(part, false), [hover, part])
  return useMemo(() => ({ onMouseEnter: () => hover?.(part, true), onMouseLeave: () => hover?.(part, false) }), [hover, part])
}

/**
 * The `TabBody` part: what plugins put at `tab.strip`, then the tab's block as its renderer
 * draws it (`useRenderer`), with the `tab.overlay` components over it. A file no renderer fits says
 * so, with a way to open it elsewhere; a terminal nothing draws is drawn plain. A terminal
 * fades a little while another pane has the focus (the theme's `paneFade`).
 */
function TabBody({ tab: id, focused }: { tab: string; focused: boolean }) {
  const t = useTheme()
  const fade = Math.min(Math.max(useMetrics().paneFade ?? METRICS.paneFade, 0), 1)
  const live = useTab(id)
  const last = useRef(live)
  if (live) last.current = live
  const tab = live ?? last.current
  const renderer = useRenderer(id)
  // A renderer is given the SDK's model (`RendererProps`), not the app's snapshot of the tab.
  const model = useTabModel(id)
  const { components } = useSlot("tab.strip")
  const overlays = useSlot("tab.overlay").components
  const body: ReactNode = !tab ? null : renderer ? (
    renderComponent(renderer, { tab: model, block: tab.block, focused })
  ) : tab.block === FILE ? (
    <FilePlaceholder tab={id} path={String(tab.options?.path ?? "")} />
  ) : runsTerminal(tab) ? (
    // A terminal whose block nothing draws (its plugin off, or not yet back after a restart).
    <terminal tab={id} autoFocus={focused} style={{ flexGrow: 1, minWidth: 0, minHeight: 0 }} />
  ) : (
    <div style={{ display: "flex", flexGrow: 1, alignItems: "center", justifyContent: "center" }}>
      <text style={{ color: t.faint }}>{`Nothing draws ${tab.block} blocks`}</text>
    </div>
  )
  return (
    <>
      {components.map((v) => renderComponent(v, { tab: id }))}
      <div style={{ display: "flex", flexGrow: 1, minWidth: 0, minHeight: 0, position: "relative", opacity: !focused && fade > 0 && runsTerminal(tab) ? 1 - fade : undefined }}>
        {body}
        {overlays.map((v) => (
          // An overlay draws over the body and leaves its mouse to it. To take a tab over, a
          // plugin draws its block instead (`@tab_renderer`).
          <div
            key={`${v.plugin}/${v.id}`}
            style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", display: "flex", flexDirection: "column", pointerEvents: "none" }}
          >
            {renderComponent(v, { tab: id, focused })}
          </div>
        ))}
      </div>
    </>
  )
}

/** The `Splitter` part: the edge between two tiled panes. It paints nothing: the line does, and
 * the cursor says the rest. */
function Splitter() {
  return <div style={{ flexGrow: 1 }} />
}

/** A length of a float: cells (a number), `"60%"` or `"36px"`. */
function length(value: unknown, cells: number): number | string {
  if (typeof value === "number") return Math.round(value * cells)
  if (typeof value === "string" && /^\d+(\.\d+)?(px|%)$/.test(value)) return value
  return value === "fill" ? "100%" : 0
}

/** What a length is a share of its parent, when it was written as one (`"80%"`). */
function share(value: unknown): number | undefined {
  if (typeof value !== "string" || !value.trim().endsWith("%")) return undefined
  const percent = Number.parseFloat(value)
  return Number.isFinite(percent) ? percent : undefined
}

/** Where a float sits in the view: its anchor's corner, its row and column in from it. */
function placement(place: any, cells: { w: number; h: number }): Record<string, unknown> {
  const anchor = String(place?.anchor ?? "NW")
  const style: Record<string, unknown> = {
    width: length(place?.width, cells.w) || undefined,
    height: length(place?.height, cells.h) || undefined,
  }
  const row = length(place?.row, cells.h)
  const col = length(place?.col, cells.w)
  if (anchor === "center") {
    // A cell or pixel size is centred by pulling its own corner back by half of itself. A share
    // has no pixel size to halve — and a percentage margin would resolve against the parent, not
    // against the box — so its corner moves in by half of what the share leaves over instead.
    const w = share(place?.width)
    const h = share(place?.height)
    return {
      ...style,
      left: w === undefined ? "50%" : `${(100 - w) / 2}%`,
      top: h === undefined ? "50%" : `${(100 - h) / 2}%`,
      transform: undefined,
      marginLeft: typeof style.width === "number" ? -style.width / 2 : 0,
      marginTop: typeof style.height === "number" ? -style.height / 2 : 0,
    }
  }
  const top = anchor.startsWith("N")
  const bottom = anchor.startsWith("S")
  const left = anchor.endsWith("W")
  const right = anchor.endsWith("E")
  return {
    ...style,
    ...(top ? { top: row } : bottom ? { bottom: row } : { top: "50%" }),
    ...(left ? { left: col } : right ? { right: col } : { left: "50%" }),
  }
}

/** Where a float's drag started: the mouse, the box's corner in the view, and the room it moves in. */
interface FloatDrag {
  x: number
  y: number
  left: number
  top: number
  room: { width: number; height: number }
  free: { width: number; height: number }
}

/** A float's place as `tab.update` takes it, from the core's (`zindex`, its view a number), at `left`/`top` in the room. */
function floatPlaceAt(place: any, view: string, at: { left: number; top: number }, room: { width: number; height: number }): Record<string, unknown> {
  // In shares of the room, so it keeps its spot as the window is resized.
  const share = (v: number, of: number) => `${of > 0 ? Math.round((v / of) * 10000) / 100 : 0}%`
  const out: Record<string, unknown> = {
    kind: "float",
    view,
    anchor: "NW",
    row: share(at.top, room.height),
    col: share(at.left, room.width),
    width: place.width ?? "50%",
    height: place.height ?? "50%",
    relative: place.relative,
    border: place.border,
    focusable: place.focusable,
    mouse: place.mouse,
    dismiss: place.dismiss,
    backdrop: place.backdrop,
    zIndex: place.zindex ?? place.zIndex,
    focus: false,
  }
  for (const key of Object.keys(out)) if (out[key] === undefined || out[key] === null) delete out[key]
  return out
}

/**
 * A float's bar: its tab's status or kind, its title and a close button. Pressing it starts a
 * drag that moves the float (`onPress`; the float follows the mouse from there).
 */
function FloatBar({ tab: id, held, onPress }: { tab: string; held: boolean; onPress(e: any): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const tab = useTab(id)
  if (!tab) return null
  return (
    <div
      onMouseDown={onPress}
      title={tab.cwd ?? undefined}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        height: 30,
        flexShrink: 0,
        paddingLeft: 10,
        paddingRight: 4,
        userSelect: "none",
        borderBottomWidth: 1,
        borderColor: t.border,
        cursor: held ? "grabbing" : "grab",
      }}
    >
      <TabIcon tab={id} size={12} color={t.faint} />
      <text style={{ fontSize: ts(12), color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{tabLabel(tab)}</text>
      <div style={{ flexGrow: 1 }} />
      <IconButton icon="close" title="Close Tab" color={t.text} onClick={() => call(via(id, "tab.close"), id).catch(swallow)} />
    </div>
  )
}

/**
 * A pane floating over the view (`kind: "float"`): its box with a bar that drags it about, its
 * backdrop, and Escape closing it. The box takes the wheel, so scrolling it never scrolls the
 * panes under it.
 */
function Float({ tab: id }: { tab: string }) {
  const t = useTheme()
  const m = useMetrics()
  const C = useComponents()
  const tab = useTab(id)
  const { renderer } = useGpuix()
  const place: any = (tab as any)?.place ?? {}
  const close = () => call(via(id, "tab.close"), id).catch(swallow)
  const dismiss: string[] = place.dismiss ?? []
  const roomRef = useRef<number | undefined>(undefined)
  const boxRef = useRef<number | undefined>(undefined)
  const drag = useRef<FloatDrag | undefined>(undefined)
  // Where the box is drawn while it is dragged, and after, until the core's place says it.
  const [at, setAt] = useState<{ left: number; top: number } | undefined>()
  const [held, setHeld] = useState(false)
  useEffect(() => setAt(undefined), [place.anchor, place.row, place.col])
  const press = (e: any) => {
    const room = roomRef.current === undefined ? null : renderer?.getElementBounds?.(roomRef.current)
    const box = boxRef.current === undefined ? null : renderer?.getElementBounds?.(boxRef.current)
    if (!room || !box) return
    const start = { left: box.x - room.x, top: box.y - room.y }
    drag.current = {
      x: e.x ?? 0,
      y: e.y ?? 0,
      ...start,
      room: { width: room.width, height: room.height },
      free: { width: Math.max(0, room.width - box.width), height: Math.max(0, room.height - box.height) },
    }
    setHeld(true)
    setAt(start)
  }
  const move = (e: any) => {
    const d = drag.current
    if (!d) return
    // No button down any more: the release was missed (it ended outside the window).
    if (e.pressedButton === undefined || e.pressedButton === null) return drop()
    const clamp = (v: number, max: number) => Math.min(Math.max(v, 0), max)
    setAt({ left: clamp(d.left + (e.x ?? d.x) - d.x, d.free.width), top: clamp(d.top + (e.y ?? d.y) - d.y, d.free.height) })
  }
  const drop = () => {
    const d = drag.current
    // The box and the room both hear a release: the first one ends the drag.
    if (!d) return
    drag.current = undefined
    setHeld(false)
    if (!at || !tab) return setAt(undefined)
    if (at.left === d.left && at.top === d.top) return setAt(undefined)
    call(via(id, "tab.update"), id, { place: floatPlaceAt(place, tab.view_id, at, d.room) }).catch(() => setAt(undefined))
  }
  const placed = placement(place, { w: 7.8, h: 18 })
  const style = at ? { ...placed, left: at.left, top: at.top, right: undefined, bottom: undefined, marginLeft: 0, marginTop: 0 } : placed
  return (
    <div
      ref={(it: { id: number } | null) => {
        roomRef.current = it?.id
      }}
      style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", backgroundColor: place.backdrop ? `#000000${Math.round((place.backdrop / 100) * 255).toString(16).padStart(2, "0")}` : undefined }}
      onMouseDown={dismiss.includes("press") || dismiss.includes("press-outside") ? close : undefined}
      // While it is dragged, the mouse can outrun the bar: the room and the box follow it too.
      onMouseMove={held ? move : undefined}
      onMouseUp={held ? drop : undefined}
    >
      <div
        ref={(it: { id: number } | null) => {
          boxRef.current = it?.id
        }}
        onMouseMove={held ? move : undefined}
        onMouseUp={held ? drop : undefined}
        autoFocus
        tabIndex={0}
        onKeyDown={(e: any) => {
          if (e.key === "escape" && dismiss.includes("escape")) {
            e.preventDefault?.()
            close()
          }
        }}
        onMouseDown={(e: any) => e?.stopPropagation?.()}
        style={{
          position: "absolute",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          borderRadius: m.radius,
          backgroundColor: t.surface,
          borderWidth: 1,
          borderColor: t.border,
          boxShadow: { offsetX: 0, offsetY: 10, blurRadius: 30, spreadRadius: 0, color: "#00000080" },
          // An absolute box lets the wheel through to what is under it unless it takes the pointer.
          pointerEvents: "auto",
          ...style,
        }}
      >
        <FloatBar tab={id} held={held} onPress={press} />
        <C.Pane pane={id} tabs={[id]} active={id} focused zoomed={false} />
      </div>
    </div>
  )
}

/** A section of a place: one contribution, with its header. */
interface Section {
  key: string
  title: string
  icon: string
  body: ReactNode
}

type Bounds = drop.Bounds

/**
 * A drop target for the side bar's own drags (`sidebar.Carried`) of `point`: `accept` picks which,
 * and `onDrop` is told where it landed and the target's box (to drop before or after it).
 */
function useDropZone(point: string, accept: (it: sidebar.Carried) => boolean, onDrop: (it: sidebar.Carried, at: { x: number; y: number }, box?: Bounds) => void) {
  return drop.useDropBox(
    (data): data is sidebar.Carried => {
      const it = sidebar.carried(data, point)
      return !!it && accept(it)
    },
    onDrop,
  )
}

/** Whether a point is in the first half of a box, down (or across) it. */
function firstHalf(at: { x: number; y: number }, box: Bounds | undefined, across = false): boolean {
  if (!box) return true
  return across ? at.x < box.x + box.width / 2 : at.y < box.y + box.height / 2
}

/**
 * A place's sections and how they are grouped (`sidebar.ts`), as the side bar and the buttons for
 * its groups (at the side bar's foot) both read them: one stored arrangement, so either changes both.
 */
function usePanel(point: string) {
  const C = useComponents()
  const { components } = useSlot(point)
  const [stored, setStored] = useLocal<unknown>(`default.${point}.groups`, sidebar.NO_ARRANGEMENT)
  // Whether the group shown is put away: its button clicked again.
  const [closed, setClosed] = useLocal(`default.${point}.closed`, false)
  const sections: Section[] = [
    ...(point === TREE_POINT ? [{ key: sidebar.TREE, title: "Sessions", icon: TREE_ICON, body: <C.Sessions /> }] : []),
    ...components.map((v) => ({ key: `${v.plugin}/${v.id}`, title: v.title ?? v.id, icon: v.icon ?? "blocks", body: renderComponent(v) })),
  ]
  const keys = sections.map((s) => s.key)
  const byKey = new Map(sections.map((s) => [s.key, s]))
  const arrangement = useMemo(() => sidebar.arrange(stored, keys), [stored, keys.join("\n")])
  const groups = sidebar.shown(arrangement, keys)
  const active = closed ? undefined : (groups.find((g) => g.id === arrangement.active) ?? groups[0])
  /** Changes the arrangement: from what is stored, with what is here now. */
  const change = (edit: (a: sidebar.Arrangement) => sidebar.Arrangement) => setStored((old) => edit(sidebar.arrange(old, keys)))
  return { byKey, groups, active, change, setClosed }
}

/** The place whose sections the tree is among: the side bar's. */
const TREE_POINT = "panel"

/** What the side bar shows with no page shown: the tree alone. */
const TREE_ALONE: sidebar.Group = { id: "", sections: [sidebar.TREE] }

const NO_SHARES: sidebar.Shares = {}

/** A place's open parts' shares of its pages' height (`sidebar.Shares`), the tree's among them. */
function useShares(point: string) {
  return useLocal<unknown>(`default.${point}.shares`, NO_SHARES)
}

/** The least an open part of a page is dragged down to. */
const PART_MIN = 64

/**
 * A place's contributions, each a section of its own, in groups: the group shown is stacked here,
 * and the side bar's foot has a button for each (`PanelButtons`). The side bar's tree is a section with
 * no header, heading its group; with no group shown it is all there is. A section's header drags to
 * sort it, onto a button to move it to that group, or onto the bar's `+` to make it a group of its
 * own; a button drags along the bar, or up onto the sections to join the group shown. The edge above
 * an open section drags it taller or shorter than what is open above it. All of it is remembered.
 */
function Panel({ point }: { point: string }) {
  const t = useTheme()
  const [collapsed, setCollapsed] = useLocal<string[]>(`default.${point}.collapsed`, [])
  const { byKey, active, change } = usePanel(point)
  const [shares, setShares] = useShares(point)
  const drag = useDragState()
  const carrying = sidebar.carried(drag?.data, point)
  const stack = useDropZone(
    point,
    (it) => it.kind === "pumi.group" && !!active && it.group !== active.id,
    (it) => it.kind === "pumi.group" && active && change((a) => sidebar.mergeGroup(a, it.group, active.id)),
  )
  const page = active ?? (point === TREE_POINT ? TREE_ALONE : undefined)
  if (!page) return null
  const grow = sidebar.grows(shares, page.sections.filter((key) => key === sidebar.TREE || !collapsed.includes(key)))
  let above: string | undefined
  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0 }}>
      <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0, position: "relative" }}>
        {page.sections.map((key, i, all) => {
          const s = byKey.get(key)!
          if (key === sidebar.TREE) {
            above = key
            // No header to drag, fold or drop on: it is always open, and heads its group.
            return (
              <div key={key} style={{ display: "flex", flexDirection: "column", flexGrow: grow[key], flexBasis: 0, minHeight: 0, overflowY: "scroll" }}>
                {s.body}
              </div>
            )
          }
          const open = !collapsed.includes(key)
          const edge = open ? above : undefined
          if (open) above = key
          return (
            <PanelSection
              key={key}
              point={point}
              section={s}
              next={all[i + 1]}
              open={open}
              share={sidebar.shareOf(shares, key)}
              grow={grow[key] ?? 0}
              prior={edge}
              shares={shares}
              setShares={setShares}
              carrying={carrying}
              toggle={() => setCollapsed(open ? [...collapsed, key] : collapsed.filter((k) => k !== key))}
              drop={(section, before) => active && change((a) => sidebar.moveSection(a, section, active.id, before))}
            />
          )
        })}
        {active && carrying?.kind === "pumi.group" && carrying.group !== active.id ? (
          // A group's button dragged up here: its sections join these.
          <div ref={stack.ref} style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", zIndex: 50, borderWidth: 2, borderColor: stack.isOver ? t.accent : "#00000000", backgroundColor: stack.isOver ? `${t.accent}22` : "#00000001" }} />
        ) : null}
      </div>
    </div>
  )
}

/**
 * The side bar's foot: a button for each group of `point`'s sections. A click shows that group (and
 * the side bar, when it is hidden); a click on the group shown puts it away, leaving the tree. They
 * take the side bar's drags too, as `GroupBar` says. With only the tree there is no foot.
 */
function PanelButtons({ point = "panel" }: { point?: string }) {
  const t = useTheme()
  const [open, setOpen] = useSidebar()
  const { byKey, groups, active, change, setClosed } = usePanel(point)
  const drag = useDragState()
  const carrying = sidebar.carried(drag?.data, point)
  const pick = (group: string) => {
    if (open && group === active?.id) return setClosed(true)
    change((a) => sidebar.show(a, group))
    setClosed(false)
    if (!open) setOpen(true)
  }
  // With only the tree there is nothing to pick between; the side bar's foot still has the tree's own toggle.
  const onlyTree = groups.every((g) => g.sections.every((s) => s === sidebar.TREE))
  const tree = point === TREE_POINT
  const footer = useSlot("sidebar.footer").actions
  if (onlyTree && !tree && footer.length === 0) return null
  return (
    <div cache style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2, height: 36, flexShrink: 0, paddingLeft: 6, paddingRight: 8, borderTopWidth: 1, borderColor: t.border }}>
      {tree ? <TreeModeButton /> : null}
      {tree && !onlyTree ? <div style={{ width: 1, height: 16, marginLeft: 4, marginRight: 4, flexShrink: 0, backgroundColor: t.border }} /> : null}
      {onlyTree ? null : <GroupBar point={point} groups={groups} active={open ? active?.id : undefined} byKey={byKey} carrying={carrying} change={change} pick={pick} />}
      <FooterButtons point={point} actions={footer} />
      {carrying && drag ? <DragGhost drag={drag} it={carrying} byKey={byKey} groups={groups} /> : null}
    </div>
  )
}

/**
 * The side bar's foot, after its panels: one button per `sidebar.footer` action, beside the panel
 * buttons and drawn like them. A plugin's way into the bar for a component that opens over the window
 * rather than filling the side bar — a board, a switcher, a palette.
 */
function FooterButtons({ point, actions }: { point: string; actions: ActionItem[] }) {
  const t = useTheme()
  const m = useMetrics()
  if (actions.length === 0) return null
  return (
    <>
      <div style={{ width: 1, height: 16, marginLeft: 4, marginRight: 4, flexShrink: 0, backgroundColor: t.border }} />
      {actions.map((action) => (
        <div
          key={`${action.plugin}/${action.id}`}
          aria-label={action.title}
          title={action.title}
          onClick={() => action.run({ point })}
          style={{
            display: "flex",
            width: BAR_BUTTON,
            height: BAR_BUTTON,
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
            borderRadius: m.radiusSmall,
            cursor: "pointer",
            userSelect: "none",
            hover: { backgroundColor: t.hover },
          }}
        >
          <Icon name={action.icon ?? "ellipsis"} size={BAR_ICON} color={t.muted} />
        </div>
      ))}
    </>
  )
}

/** A section: its header (which drags), and what it holds while open. */
function PanelSection({
  point,
  section: s,
  next,
  open,
  share,
  grow,
  prior,
  shares,
  setShares,
  carrying,
  toggle,
  drop,
}: {
  point: string
  section: Section
  /** The section after this one: what a drop on this one's lower half goes before. */
  next?: string
  open: boolean
  /** Its share of its page's height while open. */
  share: number
  /** What it grows by: its share over the open parts' shares, so together they fill the page. */
  grow: number
  /** The open part above it, the tree's key or a section's, whose height its top edge trades with its own. */
  prior?: string
  shares: unknown
  setShares(shares: unknown): void
  carrying?: sidebar.Carried
  toggle(): void
  drop(section: string, before?: string): void
}) {
  const ts = useTextSize()
  const t = useTheme()
  const { renderer } = useGpuix()
  const self = useRef<number | undefined>(undefined)
  // The edge's drag: where it started, the shares then, and how tall a share was.
  const [from, setFrom] = useState<{ at: number; shares: unknown; px: number } | undefined>()
  const draggable = useDraggable<sidebar.Carried>({ kind: "pumi.section", point, section: s.key })
  const moving = carrying?.kind === "pumi.section" && carrying.section === s.key
  const zone = useDropZone(
    point,
    (it) => it.kind === "pumi.section" && it.section !== s.key,
    (it, at, box) => it.kind === "pumi.section" && drop(it.section, firstHalf(at, box) ? s.key : next),
  )
  const above = zone.isOver && zone.drag ? firstHalf(zone.drag, zone.box()) : undefined
  const resizing = open && prior !== undefined
  return (
    <div
      ref={(it: { id: number } | null) => {
        self.current = it?.id
      }}
      style={{ display: "flex", flexDirection: "column", minHeight: 0, position: "relative", opacity: moving ? 0.45 : 1, ...(open ? { flexGrow: grow, flexBasis: 0 } : {}), borderTopWidth: 1, borderColor: t.border }}
    >
      <div
        {...draggable.props}
        onClick={() => {
          if (!draggable.wasDragged()) toggle()
        }}
        title="Drag to move"
        style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, height: 26, userSelect: "none", paddingLeft: 8, paddingRight: 8, flexShrink: 0, cursor: draggable.dragging ? "grabbing" : "pointer", hover: { backgroundColor: t.hover } }}
      >
        <Icon name={open ? "chevron_down" : "chevron_right"} size={12} color={t.faint} />
        <text style={{ fontSize: ts(11), fontWeight: 600, color: t.muted }}>{s.title}</text>
      </div>
      {open ? <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0, overflowY: "scroll" }}>{s.body}</div> : null}
      {resizing ? (
        // Its top edge, over the border: a drag trades height with the part above, a double press evens the two.
        <div
          aria-label={`Resize ${s.title}`}
          onMouseDown={(e: any) => {
            const height = self.current === undefined ? undefined : renderer?.getElementBounds?.(self.current)?.height
            if (height) setFrom({ at: e.y ?? 0, shares, px: height / share })
          }}
          onMouseMove={(e: any) => {
            if (!from) return
            // No button down any more: the release was missed (it ended outside the window).
            if (e.pressedButton === undefined || e.pressedButton === null) return setFrom(undefined)
            setShares(sidebar.resize(from.shares, prior, s.key, (e.y ?? from.at) - from.at, from.px, PART_MIN))
          }}
          onMouseUp={() => setFrom(undefined)}
          onDoubleClick={() => {
            const even = (sidebar.shareOf(shares, prior) + share) / 2
            setShares({ ...(shares as sidebar.Shares), [prior]: even, [s.key]: even })
          }}
          style={{ position: "absolute", left: 0, top: -3, width: "100%", height: 6, zIndex: 10, cursor: "row-resize", backgroundColor: from ? `${t.accent}99` : undefined, hover: { backgroundColor: `${t.accent}66` } }}
        />
      ) : null}
      {carrying?.kind === "pumi.section" && !moving ? (
        // Over the section only while one is dragged, so nothing a component draws takes the drop.
        <div ref={zone.ref} style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", zIndex: 50, backgroundColor: zone.isOver ? `${t.accent}14` : "#00000001" }}>
          {above !== undefined ? (
            <div style={{ position: "absolute", left: 0, width: "100%", height: 2, backgroundColor: t.accent, ...(above ? { top: 0 } : { bottom: 0 }) }} />
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

/** The side bar's buttons: one for each group, and while a section drags, a `+` to make one. */
function GroupBar({
  point,
  groups,
  active,
  byKey,
  carrying,
  change,
  pick,
}: {
  point: string
  groups: sidebar.Group[]
  active?: string
  byKey: Map<string, Section>
  carrying?: sidebar.Carried
  change(edit: (a: sidebar.Arrangement) => sidebar.Arrangement): void
  /** A button clicked: its group. */
  pick(group: string): void
}) {
  const t = useTheme()
  const m = useMetrics()
  // A section alone in its group already is a group of its own: the `+` has nothing to make.
  const lone = carrying?.kind === "pumi.section" && groups.some((g) => g.sections.length === 1 && g.sections[0] === carrying.section)
  const fresh = useDropZone(
    point,
    (it) => it.kind === "pumi.section" && !lone,
    (it) => it.kind === "pumi.section" && change((a) => sidebar.splitOut(a, it.section)),
  )
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2, height: "100%", flexShrink: 0, userSelect: "none" }}>
      {groups.map((g, i) => (
        <GroupButton key={g.id} point={point} group={g} next={groups[i + 1]?.id} active={g.id === active} byKey={byKey} carrying={carrying} change={change} pick={pick} />
      ))}
      {carrying?.kind === "pumi.section" && !lone ? (
        <div
          ref={fresh.ref}
          aria-label="Drop to make a group of its own"
          style={{
            display: "flex",
            width: BAR_BUTTON,
            height: BAR_BUTTON,
            alignItems: "center",
            justifyContent: "center",
            borderRadius: m.radiusSmall,
            borderWidth: 1,
            borderStyle: "dashed",
            borderColor: fresh.isOver ? t.accent : t.faint,
            backgroundColor: fresh.isOver ? `${t.accent}22` : "#00000001",
            color: fresh.isOver ? t.accent : t.faint,
          }}
        >
          <Icon name="plus" size={BAR_ICON} color={fresh.isOver ? t.accent : t.faint} />
        </div>
      ) : null}
    </div>
  )
}

/** A group's button: a click picks it, a drag sorts it along the bar; a section dropped on it joins it. */
function GroupButton({
  point,
  group,
  next,
  active,
  byKey,
  carrying,
  change,
  pick,
}: {
  point: string
  group: sidebar.Group
  next?: string
  active: boolean
  byKey: Map<string, Section>
  carrying?: sidebar.Carried
  change(edit: (a: sidebar.Arrangement) => sidebar.Arrangement): void
  pick(group: string): void
}) {
  const t = useTheme()
  const m = useMetrics()
  const draggable = useDraggable<sidebar.Carried>({ kind: "pumi.group", point, group: group.id })
  const zone = useDropZone(
    point,
    (it) => (it.kind === "pumi.group" ? it.group !== group.id : !group.sections.includes(it.section)),
    (it, at, box) => {
      if (it.kind === "pumi.section") change((a) => sidebar.moveSection(a, it.section, group.id))
      else change((a) => sidebar.moveGroup(a, it.group, firstHalf(at, box, true) ? group.id : next))
    },
  )
  const first = byKey.get(group.sections[0]!)
  const title = group.sections.map((k) => byKey.get(k)?.title ?? k).join(", ")
  const moving = carrying?.kind === "pumi.group" && carrying.group === group.id
  const side = zone.isOver && zone.drag && carrying?.kind === "pumi.group" ? (firstHalf(zone.drag, zone.box(), true) ? "left" : "right") : undefined
  return (
    <div
      ref={zone.ref}
      {...draggable.props}
      onClick={() => {
        if (!draggable.wasDragged()) pick(group.id)
      }}
      aria-label={title}
      aria-selected={active}
      title={title}
      style={{
        display: "flex",
        position: "relative",
        width: BAR_BUTTON,
        height: BAR_BUTTON,
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: m.radiusSmall,
        cursor: "pointer",
        opacity: moving ? 0.45 : 1,
        backgroundColor: zone.isOver && carrying?.kind === "pumi.section" ? `${t.accent}33` : active ? t.hover : undefined,
        hover: { backgroundColor: t.hover, color: t.text },
      }}
    >
      <Icon name={first?.icon ?? "blocks"} size={BAR_ICON} color={active ? t.text : t.muted} />
      {/* A dot, not a corner: always a circle, whatever the metrics round by. */}
      {group.sections.length > 1 ? (
        <div style={{ position: "absolute", right: 3, top: 3, width: 4, height: 4, borderRadius: 2, backgroundColor: active ? t.text : t.faint, pointerEvents: "none" }} />
      ) : null}
      {side ? <div style={{ position: "absolute", top: 2, width: 2, height: BAR_BUTTON - 4, backgroundColor: t.accent, ...(side === "left" ? { left: -2 } : { right: -2 }) }} /> : null}
    </div>
  )
}

/** Whether the side bar's tree lists each view's tabs under it (`true`), or only the views. */
const TREE_EXPANDED = "default.tree.expanded"

/** The side bar foot's first button: the tree as views only, or with each view's tabs under it. */
function TreeModeButton() {
  const t = useTheme()
  const m = useMetrics()
  const [expanded, setExpanded] = useLocal(TREE_EXPANDED, false)
  const title = expanded ? "Show Views Only" : "Show Tabs by View"
  return (
    <div
      onClick={() => setExpanded(!expanded)}
      aria-label={title}
      aria-pressed={expanded}
      title={title}
      style={{
        display: "flex",
        width: BAR_BUTTON,
        height: BAR_BUTTON,
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: m.radiusSmall,
        cursor: "pointer",
        userSelect: "none",
        backgroundColor: expanded ? t.hover : undefined,
        hover: { backgroundColor: t.hover },
      }}
    >
      <Icon name={expanded ? "list_tree" : "list"} size={BAR_ICON} color={expanded ? t.text : t.muted} />
    </div>
  )
}

/** The tree's icon, on the button of the group it heads. */
const TREE_ICON = "list_tree"

/** A side bar button's size, and its icon's: as the panes' title bar buttons are, small and square. */
const BAR_BUTTON = 22
const BAR_ICON = 14

/** What a side bar drag carries, by the mouse: the section's name, or the group's. */
function DragGhost({ drag, it, byKey, groups }: { drag: { x: number; y: number }; it: sidebar.Carried; byKey: Map<string, Section>; groups: sidebar.Group[] }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const keys = it.kind === "pumi.section" ? [it.section] : (groups.find((g) => g.id === it.group)?.sections ?? [])
  const first = byKey.get(keys[0] ?? "")
  if (!first) return null
  const label = keys.map((k) => byKey.get(k)?.title ?? k).join(", ")
  return (
    <anchored position={{ x: drag.x + 14, y: drag.y + 10 }} style={{ pointerEvents: "none" }}>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 24, paddingLeft: 8, paddingRight: 10, borderRadius: m.radius, backgroundColor: t.surface, borderWidth: 1, borderColor: t.accent, pointerEvents: "none", boxShadow: { offsetX: 0, offsetY: 4, blurRadius: 12, spreadRadius: 0, color: "#00000055" } }}>
        <Icon name={first.icon} size={12} color={t.accent} />
        <text style={{ fontSize: ts(11), fontWeight: 600, color: t.text }}>{label}</text>
      </div>
    </anchored>
  )
}

/** How far right of a dragged tab chip's left edge the mouse holds it: just past its icon. */
const GHOST_HOLD = 24

/**
 * What a tab, view or session drag carries (`moves.ts`), by the mouse. A tab is its strip chip,
 * lifted, as Zed's is: the mouse holds it near its start, so it reads where it will land. A view or
 * a session is its icon and its name, beside the mouse.
 */
function MoveGhost() {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const drag = useDragState()
  const it = moves.carried(drag?.data)
  if (!drag || !it) return null
  const snapshot = store.get().snapshot
  if (it.kind === "pumi.tab") {
    const tab = snapshot?.tabs.find((x) => x.tab_id === it.tab)
    if (!tab) return null
    const height = TITLE_BAR - 4
    return (
      <anchored position={{ x: drag.x - GHOST_HOLD, y: drag.y - height / 2 }} style={{ pointerEvents: "none" }}>
        <div
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            height,
            maxWidth: PANE_TAB_MAX,
            paddingLeft: 14,
            paddingRight: 14,
            backgroundColor: t.editor,
            borderWidth: 1,
            borderColor: t.border,
            opacity: 0.92,
            pointerEvents: "none",
            boxShadow: { offsetX: 0, offsetY: 4, blurRadius: 12, spreadRadius: 0, color: "#00000055" },
          }}
        >
          <TabIcon tab={tab.tab_id} size={14} color={t.muted} />
          <text style={{ fontSize: ts(13), color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{tabLabel(tab)}</text>
        </div>
      </anchored>
    )
  }
  let icon: string
  let label: string | undefined
  if (it.kind === "pumi.view") {
    icon = "split"
    label = viewName(snapshot, store.get().trees, it.view)
  } else {
    icon = "folder"
    label = snapshot?.workspaces.find((x) => x.workspace_id === it.session)?.label || it.session
  }
  if (!label) return null
  return (
    <anchored position={{ x: drag.x + 14, y: drag.y + 10 }} style={{ pointerEvents: "none" }}>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 26, paddingLeft: 8, paddingRight: 10, borderRadius: m.radius, backgroundColor: t.surface, borderWidth: 1, borderColor: t.border, pointerEvents: "none", boxShadow: { offsetX: 0, offsetY: 4, blurRadius: 12, spreadRadius: 0, color: "#00000055" } }}>
        <Icon name={icon} size={12} color={t.muted} />
        <text style={{ fontSize: ts(12), color: t.text, whiteSpace: "nowrap" }}>{label}</text>
      </div>
    </anchored>
  )
}

function SessionHeader() {
  const t = useTheme()
  const session = useSession()
  const { components, actions } = useSlot("session.header")
  if (!session || (!components.length && !actions.length)) return null
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, minHeight: 30, paddingLeft: 8, paddingRight: 4, flexShrink: 0, borderBottomWidth: 1, borderColor: t.border }}>
      <div style={{ display: "flex", flexGrow: 1, minWidth: 0 }}>
        <Slot slot="session.header" args={{ session: session.workspace_id }} />
      </div>
    </div>
  )
}

// ── The sessions tree ────────────────────────────────────────────────
// Sessions, their views, and the tabs in them, as a file tree draws folders: a chevron to fold a
// level, and a rail under each open level down the left of what it holds.

const TREE_ROW = 28
/** How far a level sits in from its parent. */
const TREE_INDENT = 16
const TREE_PAD = 8
/** The chevron's box: a level's rail runs down its middle. */
const CHEVRON = 12

/** How deep in the tree a tab's row is drawn (0 outside one: a theme's own list). */
const TreeDepth = createContext(0)
/** Where a tab's row is in the tree, as a drop target; none outside it, or for a float. */
const TreeSpot = createContext<moves.Row | undefined>(undefined)
/** Whether `it` drops anywhere on `row`: at its top, in its middle or at its foot (its left, middle or right `across`). */
function dropsOn(it: moves.Carried, row: moves.Row): boolean {
  const where = moves.whereIn(store.get().snapshot)
  return [0.1, 0.5, 0.9].some((fy) => moves.rowDrop(it, row, fy, where) !== undefined)
}

/**
 * A row's drop zone, over it while something that drops on it drags (and nothing then). `across`:
 * the row is laid out left to right with its kind (a view in the header's bar), so its before and
 * after are its left and right edges, not its top and foot.
 */
const RowDrop = memo(function RowDrop({ row, across }: { row: moves.Row; across?: boolean }) {
  const drag = useDragState()
  const it = moves.carried(drag?.data)
  if (!it || !dropsOn(it, row)) return null
  return <RowDropZone row={row} across={across} />
})

/** What a tab dragged makes at a session's foot: a view of its own there, or nothing. */
function newViewDrop(data: unknown, session: string): moves.Move | undefined {
  const it = moves.carried(data)
  if (it?.kind !== "pumi.tab") return undefined
  return moves.rowDrop(it, { kind: "session", session }, 0.5, moves.whereIn(store.get().snapshot))
}

/**
 * A row at the foot of an open session's views, there only while a tab that could be a view of its
 * own in it drags: dropped on it, the tab is a new view, last in the session.
 */
const NewViewDrop = memo(function NewViewDrop({ session }: { session: string }) {
  const drag = useDragState()
  if (!drag || !newViewDrop(drag.data, session)) return null
  return <NewViewDropZone session={session} />
})

function NewViewDropZone({ session }: { session: string }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const zone = drop.useDropBox(
    (data): data is moves.Carried => !!newViewDrop(data, session),
    (it) => {
      const move = newViewDrop(it, session)
      if (move) runMove(move)
    },
  )
  const lit = zone.isOver
  return (
    <div
      ref={zone.ref}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        height: TREE_ROW,
        flexShrink: 0,
        marginLeft: TREE_PAD + TREE_INDENT,
        marginRight: TREE_PAD,
        marginBottom: 4,
        paddingLeft: 8,
        borderRadius: m.radiusSmall,
        borderWidth: 1,
        borderColor: lit ? t.accent : t.border,
        backgroundColor: lit ? `${t.accent}22` : "#00000001",
      }}
    >
      <Icon name="plus" size={12} color={lit ? t.accent : t.faint} />
      <text style={{ fontSize: ts(12), color: lit ? t.text : t.faint, whiteSpace: "nowrap" }}>New view</text>
    </div>
  )
}

/** Lit as the drop would land: a line where it goes among its kind, or the row, to go into it. */
function RowDropZone({ row, across }: { row: moves.Row; across?: boolean }) {
  const t = useTheme()
  const m = useMetrics()
  const where = () => moves.whereIn(store.get().snapshot)
  const along = (at: { x: number; y: number }, box?: drop.Bounds) => {
    const { fx, fy } = drop.within(at, box)
    return across ? fx : fy
  }
  const zone = drop.useDropBox(
    (data): data is moves.Carried => {
      const it = moves.carried(data)
      return !!it && dropsOn(it, row)
    },
    (it, at, box) => {
      const move = moves.rowDrop(it, row, along(at, box), where())
      if (move) runMove(move)
    },
  )
  const it = moves.carried(zone.drag?.data)
  const move = zone.isOver && zone.drag && it ? moves.rowDrop(it, row, along(zone.drag, zone.box()), where()) : undefined
  const lit = !move
    ? undefined
    : move.kind === "session.order" || (move.kind === "view.move" && move.anchor !== undefined)
      ? move.after
        ? "after"
        : "before"
      : move.kind === "tab.beside"
        ? move.side === "left"
          ? "before"
          : "after"
        : "into"
  return (
    <div
      ref={zone.ref}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        width: "100%",
        height: "100%",
        zIndex: 50,
        borderRadius: across ? 0 : m.radiusSmall,
        borderWidth: lit === "into" ? 1 : 0,
        borderColor: t.accent,
        backgroundColor: lit === "into" ? `${t.accent}22` : "#00000001",
      }}
    >
      {lit === "before" || lit === "after" ? (
        across ? (
          <div style={{ position: "absolute", top: 0, width: 2, height: "100%", backgroundColor: t.accent, pointerEvents: "none", ...(lit === "before" ? { left: 0 } : { right: 0 }) }} />
        ) : (
          <div style={{ position: "absolute", left: 0, width: "100%", height: 2, backgroundColor: t.accent, pointerEvents: "none", ...(lit === "before" ? { top: 0 } : { bottom: 0 }) }} />
        )
      ) : null}
    </div>
  )
}

const NO_VIEWS: never[] = []
const NO_SNAPSHOT_TABS: never[] = []

/**
 * Brings a session to this window, and in it a view and a tab of it: the window shows only the
 * session it is given, so a view or tab of another session switches it first.
 */
function goTo(window: ReturnType<typeof useWindow>, session: string, view?: string, tab?: string) {
  if (window.session !== session) window.show(session)
  const shown = view ? call(via(view, "view.focus"), view) : Promise.resolve()
  shown.then(() => tab && call(via(tab, "tab.focus"), tab)).catch(swallow)
}


/** The most urgent first: what a folded level says of the tabs in it. */
const URGENCY = ["error", "blocked", "working", "done", "idle", "unknown"]

/** What wants a look: a tab waiting on the user, failed, or done with a result not yet seen. */
const wantsLook = (status: string) => status === "blocked" || status === "error" || status === "done"

/** What needs the user to act: a tab waiting on them, or failed. A done one only wants a look. */
const needsAttention = (status: string) => status === "blocked" || status === "error"

/** The most urgent of their statuses (`URGENCY`); `unknown` when none says anything. */
function lead(of: { status: string }[]): string {
  let best = "unknown"
  for (const a of of) {
    const status = a.status || "unknown"
    if (URGENCY.indexOf(status) >= 0 && URGENCY.indexOf(status) < URGENCY.indexOf(best)) best = status
  }
  return best
}

/** A row's rails: one under each of its parents' chevrons, the height of the row, so rows stack into lines. */
const Guides = memo(function Guides({ depth }: { depth: number }) {
  const t = useTheme()
  if (depth <= 0) return null
  return (
    <>
      {Array.from({ length: depth }, (_, d) => (
        <div
          key={d}
          style={{ position: "absolute", top: 0, left: TREE_PAD + d * TREE_INDENT + CHEVRON / 2, width: 1, height: "100%", backgroundColor: t.border, pointerEvents: "none" }}
        />
      ))}
    </>
  )
})

function treeRow(depth: number, selected: boolean, t: ReturnType<typeof useTheme>) {
  return {
    display: "flex",
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    minHeight: TREE_ROW,
    flexShrink: 0,
    paddingLeft: TREE_PAD + depth * TREE_INDENT,
    paddingRight: 8,
    position: "relative",
    cursor: "pointer",
    userSelect: "none",
    backgroundColor: selected ? t.selected : undefined,
    hover: { backgroundColor: selected ? t.selected : t.hover },
  } as const
}

const stop = (e: any) => e?.stopPropagation?.()

/** A small button on a tree row; its click is its own, not the row's. */
function RowButton({ icon, title, onClick }: { icon: string; title: string; onClick(e: any): void }) {
  const t = useTheme()
  const m = useMetrics()
  return (
    <div
      aria-label={title}
      title={title}
      onClick={(e: any) => {
        stop(e)
        onClick(e)
      }}
      onDoubleClick={stop}
      onAuxClick={stop}
      style={{ display: "flex", width: 20, height: 20, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: m.radiusSmall, color: t.muted, hover: { backgroundColor: t.active, color: t.text } }}
    >
      <Icon name={icon} size={12} />
    </div>
  )
}

/** What every row of the tree shows on hover, last: its menu (`…`) and its close (`×`). */
function RowButtons({ menu, close, children }: { menu(e: any): void; close?(e: any): void; children?: ReactNode }) {
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2, flexShrink: 0 }}>
      {children}
      <RowButton icon="ellipsis" title="More" onClick={menu} />
      {close ? <RowButton icon="close" title="Close" onClick={close} /> : null}
    </div>
  )
}

/** A name being edited in place, in a row of the tree: `useRename`'s field, styled as the tree's. */
function RenameField({ input }: { input: NonNullable<ReturnType<typeof useRename>["input"]> }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  return <input {...input} style={{ flexGrow: 1, minWidth: 0, fontSize: ts(13), color: t.text, paddingLeft: 4, paddingRight: 4, borderRadius: m.radiusSmall, borderWidth: 1, borderColor: t.accent, backgroundColor: t.editor }} />
}

const STATUS_ICON: Record<string, string> = {
  done: "status_check",
  blocked: "status_alert",
  error: "status_x",
  idle: "status_ring",
  unknown: "status_unknown",
}

/** A tab's status as a mark: a check, a ring, or, while it works, an arc turning round one. */
const StatusIcon = memo(function StatusIcon({ status, size = 14 }: { status: string; size?: number }) {
  const t = useTheme()
  const color = t[`status.${status}`] ?? t["status.unknown"]
  if (status !== "working") return <Icon name={STATUS_ICON[status] ?? "status_unknown"} size={size} color={color} />
  const src = assets.icon("status_spinner")
  if (!src) return null
  return (
    <motion.svg
      src={src}
      animate={{ rotate: [0, 360] }}
      // Each sample draws the window again: a turn a second at 24 a second, not the display's rate.
      transition={{ duration: 1, repeat: Infinity, ease: "linear", fps: 24 }}
      style={{ width: size, height: size, flexShrink: 0, color }}
    />
  )
})

/** The `Sessions` part: every session, and its views under it, with a search over them. */
function Sessions() {
  const ts = useTextSize()
  const t = useTheme()
  const sessions = useSessions()
  const views = useApp((s) => s.snapshot?.views ?? NO_VIEWS)
  const [folded, setFolded] = useLocal<string[]>("default.tree.folded", [])
  // Sessions in the order they were dragged into; views by their index.
  const [order] = useLocal<string[]>(SESSION_ORDER, NONE)
  // One function for every row (it is given the row's id), so a memoized row keeps its props.
  const toggle = useCallback((id: string) => setFolded((now) => (now.includes(id) ? now.filter((k) => k !== id) : [...now, id])), [setFolded])
  const [query, setQuery] = useState("")
  const [expanded] = useLocal(TREE_EXPANDED, false)
  const tabs = useApp((s) => s.snapshot?.tabs ?? NO_SNAPSHOT_TABS)
  const layouts = useApp((s) => s.trees)
  if (!sessions.length) {
    return <text style={{ padding: 10, fontSize: ts(12), color: t.faint }}>No sessions</text>
  }
  // While searching: only what matches (a session that matches keeps all its views), unfolded. A
  // view matches by its name or any of its tabs'.
  const q = query.trim().toLowerCase()
  const has = (...texts: (string | null | undefined)[]) => !q || texts.some((x) => !!x && x.toLowerCase().includes(q))
  const { snapshot, trees } = store.get()
  const worktreeOf = (view: string) => {
    const path = snapshot?.views.find((x) => x.view_id === view)?.worktree
    return path ? shortName(path) : undefined
  }
  const viewHit = (view: string) => has(viewName(snapshot, trees, view), worktreeOf(view), ...(snapshot?.tabs ?? []).filter((p) => p.view_id === view).map(tabLabel))
  // Expanded: a view's tabs pane by pane as its layout has them, then its floats.
  const laidOut = (view: string) => {
    const tree = layouts[view]
    const at = tree ? tabsOf(tree) : []
    const rank = (tab: string) => (at.indexOf(tab) < 0 ? at.length : at.indexOf(tab))
    return tabs.filter((p) => p.view_id === view).sort((a, b) => rank(a.tab_id) - rank(b.tab_id))
  }
  const leaves = (session: string, held: typeof tabs, view: string, whole: boolean) => {
    // Searching, a view that matches by its own name (or its session's) keeps all its tabs.
    const shown = whole ? held : held.filter((p) => has(tabLabel(p)))
    return shown.map((p) => <TreeLeaf key={p.tab_id} tab={p.tab_id} depth={1} status={!!p.status} session={session} view={view} alone={false} />)
  }
  let shownAny = false
  return (
    <div style={{ display: "flex", flexDirection: "column", paddingBottom: 4 }}>
      <TreeSearch query={query} setQuery={setQuery} />
      {q ? null : <NeedsAttention />}
      {moves.sortSessions(sessions, order).map((session) => {
        const id = session.workspace_id
        const own = views.filter((view) => view.workspace_id === id).sort((a, b) => a.number - b.number)
        const shown = has(session.label || id) ? own : own.filter((view) => viewHit(view.view_id))
        if (q && !shown.length && !has(session.label || id)) return null
        shownAny = true
        const shut = !q && folded.includes(id)
        return (
          <div key={id} style={{ display: "flex", flexDirection: "column", flexShrink: 0, borderBottomWidth: 1, borderColor: t.border }}>
            <SessionRow session={id} label={session.label || id} folded={shut} toggle={toggle} status={shut ? lead(own) : undefined} views={own.length} />
            {shut
              ? null
              : shown.map((view) => {
                  // A view of one tab stays one row, as compact draws it: there is nothing to group.
                  const held = expanded ? laidOut(view.view_id) : NO_SNAPSHOT_TABS
                  const grouped = held.length > 1
                  return (
                    <Fragment key={view.view_id}>
                      <ViewRow view={view.view_id} session={id} expanded={grouped} />
                      {grouped ? leaves(id, held, view.view_id, has(session.label || id, viewName(snapshot, trees, view.view_id))) : null}
                    </Fragment>
                  )
                })}
            {shut ? null : <NewViewDrop session={id} />}
          </div>
        )
      })}
      {q && !shownAny ? <text style={{ paddingLeft: 12, paddingTop: 6, fontSize: ts(12), color: t.faint }}>No matching views</text> : null}
    </div>
  )
}

/**
 * The tabs that need attention (`needsAttention`, by their `@status`), the most urgent first, on a
 * yellow band over the sessions; a click brings one up. Nothing is drawn while none do. A block
 * sets its own tabs' status; a terminal's comes from its program, which reports it (OSC 7501) or
 * is read by a fallback (OSC 9;4, a spinning title, a notification), as its icon in the tree shows.
 */
function NeedsAttention() {
  const ts = useTextSize()
  const t = useTheme()
  const window = useWindow()
  const tabs = useApp((s) => s.snapshot?.tabs ?? NO_SNAPSHOT_TABS)
  const loud = tabs.filter((p) => needsAttention(p.status)).sort((a, b) => URGENCY.indexOf(a.status) - URGENCY.indexOf(b.status))
  if (!loud.length) return null
  // On the yellow, the window's colour reads: dark on the dark theme's light yellow, light on the light theme's amber.
  const ink = t.window
  return (
    <div style={{ display: "flex", flexDirection: "column", flexShrink: 0, paddingTop: 4, paddingBottom: 4, backgroundColor: t.warning, borderBottomWidth: 1, borderColor: t.border }}>
      <text style={{ paddingLeft: ROW_PAD, paddingTop: 2, paddingBottom: 2, fontSize: ts(11), fontWeight: 600, color: ink }}>{`Needs attention · ${loud.length}`}</text>
      {loud.map((a) => (
        <div
          key={a.tab_id}
          onClick={() => goTo(window, a.workspace_id, a.view_id, a.tab_id)}
          style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, minHeight: TREE_ROW, flexShrink: 0, paddingLeft: ROW_PAD, paddingRight: 8, cursor: "pointer", userSelect: "none", hover: { backgroundColor: "#0000001f" } }}
        >
          {/* In the ink, not the status's colour, which washes out on yellow: the shape tells them apart. */}
          <Icon name={STATUS_ICON[a.status] ?? "status_unknown"} size={14} color={ink} />
          <text style={{ fontSize: ts(12), fontWeight: 600, color: ink, whiteSpace: "nowrap", textOverflow: "ellipsis", flexGrow: 1, flexShrink: 1, minWidth: 0 }}>{tabLabel(a)}</text>
        </div>
      ))}
    </div>
  )
}

/** A side bar row's height: a session's, and each of its views'. */
const SESSION_ROW = 32
const VIEW_ROW = 34
/** How far in a row's content starts. */
const ROW_PAD = 12

/** A view's name, as the view bar and the side bar show it (`viewName`). */
function useViewName(view: string): string {
  return useApp((s) => viewName(s.snapshot, s.trees, view))
}

/**
 * A session's row, as Zed's project header: a click folds its views; its name, and on hover a
 * chevron by it (the same fold), a `+` (a new view in it; in a repository, `NewViewMenu`: in which worktree) and a `…` (its menu: `session.menu`, the
 * app's Rename, New View, Open in New Window, Close); a right click opens the menu too. Folded, it says what its views
 * say (`status`) and how many views it has. Memoized: a change to one of its views does not draw it.
 */
const SessionRow = memo(function SessionRow({ session: id, label, folded, toggle, status, views }: { session: string; label: string; folded: boolean; toggle(id: string): void; status?: string; views: number }) {
  const ts = useTextSize()
  const t = useTheme()
  const window = useWindow()
  const [hovered, setHovered] = useState(false)
  const current = id === window.session
  const { open, show, menu, shown } = useMenu("session.menu", { session: id })
  const [adding, setAdding] = useState<{ x: number; y: number } | undefined>()
  const newView = () => {
    call(via(id, "tab.open"), null, {}, { kind: "view", session: id })
      .then(() => current || window.show(id))
      .catch(swallow)
  }
  const { renaming, input } = useRename({ session: id }, label)
  const draggable = useDraggable<moves.Carried>({ kind: "pumi.session", session: id }, { disabled: renaming })
  const lit = (hovered || shown || !!adding) && !renaming
  // Read while the row is lit, so the `+` knows by its click whether there are worktrees to offer.
  const worktrees = worktreeViews.useWorktrees(id, lit)
  return (
    <>
      <div
        {...draggable.props}
        onClick={() => draggable.wasDragged() || renaming || toggle(id)}
        onAuxClick={open}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        aria-expanded={!folded}
        style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, height: SESSION_ROW, flexShrink: 0, paddingLeft: ROW_PAD, paddingRight: 8, position: "relative", cursor: "pointer", userSelect: "none" }}
      >
        {input ? (
          <RenameField input={input} />
        ) : (
          <text style={{ fontSize: ts(13), fontWeight: current ? 500 : 400, color: current || lit ? t.text : t.muted, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{label}</text>
        )}
        {lit ? <RowButton icon={folded ? "chevron_right" : "chevron_down"} title={folded ? "Expand" : "Collapse"} onClick={() => toggle(id)} /> : null}
        {renaming ? null : <div style={{ flexGrow: 1 }} />}
        {lit ? (
          <RowButtons menu={show}>
            <RowButton
              icon="plus"
              title={worktrees.length ? "New View In…" : "New view"}
              onClick={(e: any) => (worktrees.length ? setAdding({ x: e?.x ?? 0, y: e?.y ?? 0 }) : newView())}
            />
          </RowButtons>
        ) : folded && !renaming ? (
          <>
            {status && status !== "unknown" ? <StatusIcon status={status} size={12} /> : null}
            <text style={{ fontSize: ts(11), color: t.faint }}>{String(views)}</text>
          </>
        ) : null}
        <RowDrop row={{ kind: "session", session: id }} />
      </div>
      {menu}
      {adding ? <NewViewMenu session={id} at={adding} close={() => setAdding(undefined)} /> : null}
    </>
  )
})

/** A worktree's mark: the worktree glyph and its folder's name (`pilot-ivy`). */
const WorktreeBadge = memo(function WorktreeBadge({ path, size = 11, color }: { path: string; size?: number; color?: string }) {
  const ts = useTextSize()
  const t = useTheme()
  const c = color ?? t.faint
  return (
    <div title={path} style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, flexShrink: 1, minWidth: 0 }}>
      <Icon name="git_worktree" size={size + 1} color={c} />
      <text style={{ fontSize: ts(size), color: c, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{shortName(path)}</text>
    </div>
  )
})

/**
 * What a session's `+` opens when it is in a repository, as Zed's: a new view in one of its
 * worktrees (the one the window's front view is in checked), and a new worktree, on a new branch
 * or on one no worktree has checked out. Its field searches both as you type: ↑ ↓ (⌃N ⌃P) move,
 * Enter opens, → and ← go into and out of `Create New Worktree…`, Escape backs out.
 */
function NewViewMenu({ session, at, close }: { session: string; at: { x: number; y: number }; close(): void }) {
  return (
    <MenuCard at={at} close={close}>
      <NewViewRows session={session} close={close} />
    </MenuCard>
  )
}

const CREATE_WORKTREE = "Create New Worktree…"

/** `NewViewMenu`'s field and rows: inside its card, so the keys can open its submenu. */
function NewViewRows({ session, close }: { session: string; close(): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  const window = useWindow()
  const menu = useContext(MenuContext)
  const worktrees = worktreeViews.useWorktrees(session)
  const home = useApp((s) => s.snapshot?.workspaces.find((w) => w.workspace_id === session))
  const frontView = useApp((s) => (window.session === session && home?.active_view_id ? s.snapshot?.views.find((x) => x.view_id === home.active_view_id) : undefined))
  const here = frontView ? (frontView.worktree ?? home?.cwd) : home?.cwd
  const [free, setFree] = useState<string[] | undefined>()
  useEffect(() => {
    let live = true
    worktreeViews.freeBranchesFor(session).then((list) => live && setFree(list), () => live && setFree([]))
    return () => {
      live = false
    }
  }, [session])
  const [query, setQuery] = useState("")
  // The lit row: of the worktrees, then `Create New Worktree…` after them; and in its submenu.
  const [lit, setLit] = useState(0)
  const [inner, setInner] = useState(0)

  const q = query.trim().toLowerCase()
  const has = (...words: (string | undefined)[]) => !q || words.some((w) => w?.toLowerCase().includes(q))
  const shown = worktrees.filter((w) => has(nameOf(w), w.branch, w.path))
  const branches = (free ?? []).filter((b) => has(b))
  const create = shown.length
  const at = Math.min(lit, create)
  const sub = menu?.sub === CREATE_WORKTREE
  const subRows = branches.length + 1
  const inAt = Math.min(inner, subRows - 1)

  const openIn = (path: string) => {
    close()
    worktreeViews.open(window, session, path).catch(worktreeViews.report)
  }
  const createOn = (branch?: string) => {
    close()
    if (branch === undefined) worktreeViews.createAsking(window, session)
    else worktreeViews.create(window, session, branch).catch(worktreeViews.report)
  }
  const enterSub = () => {
    setLit(create)
    setInner(0)
    menu?.show(CREATE_WORKTREE)
  }
  const submit = () => {
    if (sub) createOn(inAt === 0 ? undefined : branches[inAt - 1])
    else if (at < create) openIn(shown[at]!.path)
    else enterSub()
  }

  return (
    <>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 26, flexShrink: 0, marginLeft: 4, marginRight: 4, marginBottom: 2, paddingLeft: 6, paddingRight: 6, borderRadius: m.radiusSmall, borderWidth: 1, borderColor: t.accent, backgroundColor: t.editor }}>
        <Icon name="magnifying_glass" size={12} color={t.faint} />
        <input
          autoFocus
          value={query}
          placeholder="Search worktrees…"
          aria-label="Search worktrees and branches"
          onChange={(e: any) => {
            setQuery(e.value ?? "")
            // Typing in the open submenu searches its branches and stays there.
            setLit(sub ? create : 0)
            setInner(0)
          }}
          onSubmit={submit}
          onKeyDown={(e) => {
            const ctrl = !!e.modifiers?.ctrl
            const step = e.key === "down" || (ctrl && e.key === "n") ? 1 : e.key === "up" || (ctrl && e.key === "p") ? -1 : 0
            if (step && sub) setInner((inAt + step + subRows) % subRows)
            else if (step) setLit((at + step + create + 1) % (create + 1))
            else if (e.key === "right" && !sub && at === create) enterSub()
            else if (e.key === "left" && sub) menu?.show(undefined)
            else if (e.key === "escape") sub ? menu?.show(undefined) : close()
            else return
            e.preventDefault()
            e.stopPropagation()
          }}
          style={{ flexGrow: 1, minWidth: 0, fontSize: ts(13), color: t.text, backgroundColor: "transparent" }}
        />
      </div>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", height: 24, flexShrink: 0, paddingLeft: 10, paddingRight: 10 }}>
        <text style={{ fontSize: ts(12), color: t.faint }}>New View In…</text>
      </div>
      <div style={{ display: "flex", flexDirection: "column", flexShrink: 1, minHeight: 0, maxHeight: 22 * 16, overflowY: "scroll" }}>
        {shown.length ? (
          shown.map((w, i) => (
            <MenuItem
              key={w.path}
              icon="git_worktree"
              title={nameOf(w)}
              detail={w.path}
              checked={worktreeViews.samePath(here, w.path)}
              lit={!sub && i === at}
              onEnter={() => {
                setLit(i)
                menu?.enter()
              }}
              onClick={() => openIn(w.path)}
            />
          ))
        ) : (
          <div style={{ display: "flex", flexDirection: "row", alignItems: "center", height: 22, flexShrink: 0, paddingLeft: 10, paddingRight: 10 }}>
            <text style={{ fontSize: ts(13), color: t.faint }}>No matching worktrees</text>
          </div>
        )}
      </div>
      <MenuRule />
      <SubMenu title={CREATE_WORKTREE} lit={!sub && at === create} onEnter={() => setLit(create)}>
        <NewWorktreeRows branches={branches} lit={sub ? inAt : -1} onLit={setInner} pick={createOn} />
      </SubMenu>
    </>
  )
}

/** The `Create New Worktree…` submenu: a new branch (asked for), then each branch no worktree has. */
function NewWorktreeRows({ branches, lit, onLit, pick }: { branches: string[]; lit: number; onLit(i: number): void; pick(branch?: string): void }) {
  return (
    <>
      <MenuItem icon="plus" title="New Branch…" lit={lit === 0} onEnter={() => onLit(0)} onClick={() => pick()} />
      {branches.length ? <MenuRule /> : null}
      {branches.map((branch, i) => (
        <MenuItem key={branch} icon="git_branch" title={branch} detail={`A worktree on ${branch}`} lit={lit === i + 1} onEnter={() => onLit(i + 1)} onClick={() => pick(branch)} />
      ))}
    </>
  )
}

/**
 * A view's row under its session: its tabs' icons, as the view bar has them, and its name; the
 * window's front view is lit. On hover its menu (rename, close) and its close; a tab dropped on
 * it joins it, and dragged, it moves among its session's views or to another session. `expanded`:
 * its tabs are listed under it, so it draws no icons of them. Memoized.
 */
const ViewRow = memo(function ViewRow({ view: id, session, expanded }: { view: string; session: string; expanded?: boolean }) {
  const ts = useTextSize()
  const t = useTheme()
  const window = useWindow()
  const label = useViewName(id)
  const worktree = useApp((s) => s.snapshot?.views.find((x) => x.view_id === id)?.worktree)
  const front = useApp((s) => window.session === session && s.snapshot?.workspaces.find((w) => w.workspace_id === session)?.active_view_id === id)
  // ⌘1…⌘9 go to the views of the session the window shows, by number: only its rows have them.
  const number = useApp((s) => (window.session === session ? s.snapshot?.views.find((x) => x.view_id === id)?.number : undefined))
  const keys = useHeldKeys("switch_view", number ?? -1)
  const [hovered, setHovered] = useState(false)
  const close = () => call(via(id, "view.close"), id).catch(swallow)
  const { renaming, input } = useRename({ view: id }, label)
  const { open, show, menu, shown } = useMenu(["view.menu", "view.actions"], { view: id })
  const draggable = useDraggable<moves.Carried>({ kind: "pumi.view", view: id }, { disabled: renaming })
  const row = useMemo<moves.Row>(() => ({ kind: "view", session, view: id }), [session, id])
  const background = front ? t.selected : hovered ? t.hover : t.surface
  return (
    <>
      {/* A compartment of its own: a working tab's spinner draws its row again, not the list. */}
      <div compartment>
        <div
          {...draggable.props}
          onClick={() => draggable.wasDragged() || renaming || goTo(window, session, id)}
          onAuxClick={open}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
          style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, height: VIEW_ROW, flexShrink: 0, paddingLeft: ROW_PAD, paddingRight: 8, position: "relative", cursor: "pointer", userSelect: "none", backgroundColor: background }}
        >
          {expanded ? null : <ViewIcons view={id} ring={background} />}
          {input ? (
            <RenameField input={input} />
          ) : (
            <>
              <text style={{ fontSize: ts(13), color: front || hovered ? t.text : t.muted, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{label}</text>
              <div style={{ flexGrow: 1 }} />
              {/* Its worktree at the far end, where the row's buttons take its place on hover, and its keys while ⌘ is held. */}
              {keys ? <HeldKeys keys={keys} /> : hovered || shown ? <RowButtons menu={show} close={close} /> : worktree ? <WorktreeBadge path={worktree} /> : null}
            </>
          )}
          <RowDrop row={row} />
        </div>
      </div>
      {menu}
    </>
  )
})

/**
 * A tab's row under its view, in the expanded tree: the theme's `TabRow` for a tab with a status
 * (`@status`), else a plain row. Where it is makes it a drop target (`moves.Row`). Memoized on
 * those, so a change to another tab does not draw it again.
 */
const TreeLeaf = memo(function TreeLeaf({ tab, depth, status, session, view, alone }: { tab: string; depth: number; status: boolean; session: string; view: string; alone: boolean }) {
  const C = useComponents()
  const spot = useMemo<moves.Row>(() => ({ kind: "tab", session, view, tab, alone }), [session, view, tab, alone])
  return (
    <TreeDepth.Provider value={depth}>
      <TreeSpot.Provider value={spot}>{status ? <C.TabRow tab={tab} /> : <PlainTabRow tab={tab} />}</TreeSpot.Provider>
    </TreeDepth.Provider>
  )
})

/** A tab with no status, in the expanded tree: its kind's icon and its title; a click goes to it. */
function PlainTabRow({ tab }: { tab: string }) {
  const ts = useTextSize()
  const t = useTheme()
  const window = useWindow()
  const depth = useContext(TreeDepth)
  const p = useTab(tab)
  const focused = useFocusedTab()
  const front = useView()?.view_id
  const [hovered, setHovered] = useState(false)
  const close = () => call(via(tab, "tab.close"), tab).catch(swallow)
  const { open, show, menu, shown } = useMenu("tab.menu", { tab })
  const draggable = useDraggable<moves.Carried>({ kind: "pumi.tab", tab })
  const spot = useContext(TreeSpot)
  if (!p) return null
  const selected = tab === focused && p.view_id === front
  return (
    <>
      <div compartment>
        <div
          {...draggable.props}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
          onAuxClick={open}
          onClick={() => draggable.wasDragged() || goTo(window, p.workspace_id, p.view_id, tab)}
          style={treeRow(depth, selected, t)}
        >
          <Guides depth={depth} />
          <div style={{ display: "flex", width: 14, height: 14, flexShrink: 0, alignItems: "center", justifyContent: "center" }}>
            <TabIcon tab={tab} size={12} color={t.muted} />
          </div>
          <text style={{ fontSize: ts(12), color: selected || hovered ? t.text : t.muted, whiteSpace: "nowrap", textOverflow: "ellipsis", flexGrow: 1, flexShrink: 1, minWidth: 0 }}>{tabLabel(p)}</text>
          {hovered || shown ? <RowButtons menu={show} close={close} /> : null}
          {spot ? <RowDrop row={spot} /> : null}
        </div>
      </div>
      {menu}
    </>
  )
}

/**
 * The field over the sessions tree, as Zed's: a strip the side bar's width, under the title bar's
 * edge and over its own. What it types narrows the tree; Escape (or the ×) clears it.
 */
function TreeSearch({ query, setQuery }: { query: string; setQuery(q: string): void }) {
  const ts = useTextSize()
  const t = useTheme()
  const [focused, setFocused] = useState(false)
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, height: 34, flexShrink: 0, paddingLeft: 12, paddingRight: 6, borderBottomWidth: 1, borderColor: t.border }}>
      <Icon name="magnifying_glass" size={14} color={focused ? t.muted : t.faint} />
      <input
        value={query}
        placeholder="Search sessions…"
        aria-label="Search sessions"
        onChange={(e: any) => setQuery(e.value ?? "")}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={(e) => {
          if (e.key !== "escape" || !query) return
          e.preventDefault()
          e.stopPropagation()
          setQuery("")
        }}
        style={{ flexGrow: 1, minWidth: 0, fontSize: ts(13), color: t.text, backgroundColor: "transparent" }}
      />
      {query ? <RowButton icon="close" title="Clear search" onClick={() => setQuery("")} /> : null}
    </div>
  )
}

/**
 * The `TabRow` part: a tab with a status (`@status`) in the tree, named by its title or its label,
 * bold while it wants a look. Under the name its `tab.row.footer`; on hover its
 * `tab.row.controls` (the `TabRowControls` part), its menu (`tab.row.menu`) and its close.
 */
function TabRow({ tab }: { tab: string }) {
  const ts = useTextSize()
  const t = useTheme()
  const C = useComponents()
  const window = useWindow()
  const depth = useContext(TreeDepth)
  const p = useTab(tab)
  const focused = useFocusedTab()
  const front = useView()?.view_id
  const controls = useSlot("tab.row.controls")
  const [hovered, setHovered] = useState(false)
  const close = () => call(via(tab, "tab.close"), tab).catch(swallow)
  const { open, show, menu, shown } = useMenu("tab.row.menu", { tab })
  const draggable = useDraggable<moves.Carried>({ kind: "pumi.tab", tab })
  const spot = useContext(TreeSpot)
  const footer = useMemo(() => ({ tab }), [tab])
  if (!p) return null
  const status = p.status || "unknown"
  const label = p.terminal_title || p.label || tabLabel(p)
  // What wants a look is bold, as unread mail is.
  const loud = wantsLook(status)
  const hasControls = controls.actions.length + controls.components.length > 0
  return (
    <>
      <div compartment>
        <div
          {...draggable.props}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
          onAuxClick={open}
          onClick={() => draggable.wasDragged() || goTo(window, p.workspace_id, p.view_id, tab)}
          title={p.label && p.terminal_title && p.label !== p.terminal_title ? `${p.label} · ${p.terminal_title}` : undefined}
          style={treeRow(depth, tab === focused && p.view_id === front, t)}
        >
          <Guides depth={depth} />
          <TabIcon tab={tab} />
          <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0 }}>
            <text style={{ fontSize: ts(12), fontWeight: loud ? 600 : 400, whiteSpace: "nowrap", textOverflow: "ellipsis" }}>{label}</text>
            <Slot slot="tab.row.footer" args={footer} />
          </div>
          {hovered || shown ? (
            <RowButtons menu={show} close={close}>
              {hasControls ? <C.TabRowControls tab={tab} /> : null}
            </RowButtons>
          ) : null}
          {spot ? <RowDrop row={spot} /> : null}
        </div>
      </div>
      {menu}
    </>
  )
}

function TabRowControls({ tab }: { tab: string }) {
  const C = useComponents()
  const { components, actions } = useSlot("tab.row.controls")
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2 }}>
      {actions.map((a) => (
        <C.Action key={`${a.plugin}/${a.id}`} action={a} args={{ tab }} />
      ))}
      {components.map((v) => renderComponent(v, { tab }))}
    </div>
  )
}

function Action({ action, args }: { action: ActionItem; args?: unknown }) {
  return <IconButton icon={action.icon ?? "ellipsis"} title={action.title} onClick={() => action.run(args)} />
}

function Starting({ title, detail, gaveUp, lines }: { title: string; detail: string; gaveUp: boolean; lines: string[] }) {
  const ts = useTextSize()
  const t = useTheme()
  const m = useMetrics()
  return (
    <div style={{ display: "flex", flexDirection: "column", width: "100%", height: "100%", alignItems: "center", justifyContent: "center", gap: 8, backgroundColor: t.window, color: t.text }}>
      {title ? <text style={{ fontSize: ts(15), fontWeight: 600, color: gaveUp ? t.error : t.text }}>{title}</text> : null}
      {detail ? <text style={{ fontSize: ts(12), color: t.muted, maxWidth: 520 }}>{detail}</text> : null}
      {lines.length ? (
        <div style={{ display: "flex", flexDirection: "column", marginTop: 8, padding: 8, borderRadius: m.radius, backgroundColor: t.surface, maxWidth: 620 }}>
          {lines.map((line, i) => (
            <text key={i} style={{ fontSize: ts(11), color: t.faint, fontFamily: FONT_DEFAULT.family }}>{line}</text>
          ))}
        </div>
      ) : null}
    </div>
  )
}

/** The app's own parts. */
export const DEFAULT: Components = {
  Window,
  Starting,
  ViewBar,
  View,
  Layout,
  Split,
  Pane,
  PaneTitle,
  PaneTab,
  TabBody,
  Splitter,
  Panel,
  Sessions,
  Float,
  TabRow,
  TabRowControls,
  Action,
  Menu,
  Dialog,
  IconButton: DefaultIconButton,
  TabIcon: DefaultTabIcon,
}
