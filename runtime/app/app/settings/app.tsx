// The Settings window's tree: the nav (the built-in pages, then the plugins'), the search, and the
// page shown, all drawn with the settings kit over `SettingsContext`.
import { useEffect, useMemo, useState } from "react"
import { Button, Note, OptionRow, Page, Row, Section, TextField, useTextSize, type SettingsKit } from "../../sdk/settings/index.ts"
import { SettingsContext } from "../../sdk/settings/context.ts"
import { Icon } from "../../sdk/elements.tsx"
import { useMetrics, useTheme, withOpacity } from "../../sdk/theme.ts"
import { BUILT_IN, HIDDEN, otherGroups, pageOfLink, PAGES, pluginPages, SCHEMES, search, searchIndex, STORE, THEMES, type Entry, type PageInfo } from "./pages.ts"
import { AgentsPage } from "./pages/agents.tsx"
import { AppearancePage, SchemesPage, ThemesPage } from "./pages/appearance.tsx"
import { GeneralPage } from "./pages/general.tsx"
import { KeysPage, useKeyRows } from "./pages/keys.tsx"
import { PluginsPage } from "./pages/plugins.tsx"
import { StorePage } from "./pages/store.tsx"
import { GroupedOptions, PluginComponent, PluginSections, usePluginSettings } from "./shared.tsx"
import { SETTINGS_FILE, useDecls, useValues } from "./store.ts"
import { restartApp } from "../engine.ts"

const NAV_WIDTH = 220
/** Room for the traffic lights over the nav. */
const TITLEBAR = 36

/** Where Settings is asked to go: a page, the thing a link names on it, and a count so the same ask again is heard. */
export interface Route {
  page?: string
  link?: string
  asked: number
}

/** A page name as a link may write it (`keybindings` is `keys`). */
export function pageName(page: string | undefined): string | undefined {
  const p = page?.trim().toLowerCase()
  return p === "keybindings" ? "keys" : p || undefined
}

function NavItem({ page, selected, onClick }: { page: PageInfo; selected: boolean; onClick(): void }) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  return (
    <div
      testId={`settings-nav-${page.id}`}
      role="tab"
      aria-selected={selected}
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key !== "enter" && e.key !== "space") return
        e.preventDefault()
        onClick()
      }}
      style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, height: 28, paddingLeft: 8, paddingRight: 8, borderRadius: m.radiusSmall, backgroundColor: selected ? t.selected : "transparent", cursor: "pointer", keyboardFocusDim: false, hover: selected ? {} : { backgroundColor: t.hover } }}
    >
      <Icon name={page.icon} size={14} color={selected ? t.text : t.muted} />
      <text style={{ fontSize: ts(13), color: selected ? t.text : t.muted, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{page.title}</text>
    </div>
  )
}

function Nav({ groups, page, searching, query, setQuery, go }: { groups: [string, PageInfo[]][]; page: string; searching: boolean; query: string; setQuery(q: string): void; go(id: string): void }) {
  const t = useTheme()
  const ts = useTextSize()
  const all = groups.flatMap(([, pages]) => pages)
  return (
    <div
      style={{ display: "flex", flexDirection: "column", width: NAV_WIDTH, flexShrink: 0, height: "100%", borderRightWidth: 1, borderColor: t.border, backgroundColor: t.surface }}
      onKeyDown={(e) => {
        if (searching || (e.key !== "up" && e.key !== "down")) return
        const at = Math.max(0, all.findIndex((p) => p.id === page))
        const next = all[Math.min(all.length - 1, Math.max(0, at + (e.key === "up" ? -1 : 1)))]
        if (next) {
          e.preventDefault()
          go(next.id)
        }
      }}
    >
      <div style={{ height: TITLEBAR, flexShrink: 0, appRegion: "drag" }} />
      <div style={{ display: "flex", padding: 10, paddingTop: 0 }}>
        <TextField search value={query} width={NAV_WIDTH - 20} placeholder="Search settings…" onChange={setQuery} onCommit={setQuery} testId="settings-search" />
      </div>
      <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0, overflowY: "scroll", paddingLeft: 10, paddingRight: 10, paddingBottom: 10 }}>
        {groups.map(([label, pages]) => (
          <div key={label} style={{ display: "flex", flexDirection: "column", gap: 1, paddingBottom: 12 }}>
            <text style={{ fontSize: ts(12), color: t.faint, paddingLeft: 8, paddingTop: 4, paddingBottom: 4 }}>{label}</text>
            {pages.map((p) => (
              <NavItem key={p.id} page={p} selected={!searching && p.id === page} onClick={() => go(p.id)} />
            ))}
          </div>
        ))}
      </div>
    </div>
  )
}

function SafeBanner() {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12, paddingLeft: 12, paddingRight: 12, paddingTop: 8, paddingBottom: 8, borderRadius: m.radius, borderWidth: 1, borderColor: t.warning, backgroundColor: withOpacity(t.warning, 0.08) }}>
      <text style={{ fontSize: ts(13), color: t.warning }}>Safe mode: plugins are off</text>
      <Button onClick={() => restartApp({ safe: false })}>Restart normally</Button>
    </div>
  )
}

/** The search's hits, by page: an option's own row (its control works here), else a row that goes to it. */
function Results({ hits, titles, open }: { hits: Entry[]; titles: Map<string, string>; open(page: string, link: string): void }) {
  const byPage: [string, Entry[]][] = []
  for (const h of hits) {
    const group = byPage.find(([p]) => p === h.page)
    if (group) group[1].push(h)
    else byPage.push([h.page, [h]])
  }
  return (
    <Page title="Search">
      {!hits.length ? <Note>No settings match.</Note> : null}
      {byPage.map(([page, list]) => (
        <Section key={page} title={titles.get(page) ?? page}>
          {list.map((h) =>
            h.kind === "option" ? (
              <OptionRow key={h.id} name={h.id} />
            ) : (
              <Row key={h.id} title={h.title} description={h.kind === "section" ? "A section" : h.kind === "command" ? "A command's keys" : undefined} control={<Button onClick={() => open(h.page, h.id)}>Show</Button>} />
            ),
          )}
        </Section>
      ))}
    </Page>
  )
}

export function SettingsApp({ route, safe }: { route: Route; safe: boolean }) {
  const t = useTheme()
  const ts = useTextSize()
  const { decls, error } = useDecls()
  const values = useValues(decls)
  const manifests = usePluginSettings()
  const { pages: own, sections } = useMemo(() => pluginPages(manifests, decls, !safe), [manifests, decls, safe])
  const pages = useMemo(() => [...BUILT_IN.map((id) => PAGES[id]), ...own], [own])
  const keyRows = useKeyRows()
  const index = useMemo(() => searchIndex(decls, pages, sections, keyRows), [decls, pages, sections, keyRows])
  const [page, setPage] = useState("general")
  const [highlight, setHighlight] = useState<string>()
  const [query, setQuery] = useState("")
  const [problem, setProblem] = useState<string>()
  const open = (to: string, link?: string) => {
    setPage(to)
    setHighlight(link)
    setQuery("")
    setProblem(undefined)
  }
  // A route asked for (`openSettings`): its page, or the page of what its link names.
  useEffect(() => {
    if (!route.asked) return
    const link = route.link?.replace(/^\/+|\/+$/g, "") || undefined
    const named = pageName(route.page)
    const to = named ?? (link ? pageOfLink(link, decls, pages, sections) : undefined) ?? page
    open(to, link)
  }, [route.asked])
  // A link named before the options were read goes to its page once they are.
  useEffect(() => {
    if (!highlight || route.page) return
    const to = pageOfLink(highlight, decls, pages, sections)
    if (to && to !== page) setPage(to)
  }, [decls.length])
  const kit: SettingsKit = { decls, ...values, highlight, query, open, report: setProblem }
  const groups: [string, PageInfo[]][] = [["Options", BUILT_IN.map((id) => PAGES[id])]]
  if (own.length) groups.push(["Plugins", own])
  const shown = pages.find((p) => p.id === page) ?? HIDDEN.find((p) => p.id === page) ?? PAGES.general
  const titles = new Map(pages.map((p) => [p.id, p.title]))
  const searching = query.trim() !== ""
  // A page of every theme or scheme scrolls its own grid, which fills the page: the page itself stays put.
  const fills = !searching && (shown.id === THEMES.id || shown.id === SCHEMES.id)
  const body = searching ? (
    <Results hits={search(index, query)} titles={titles} open={open} />
  ) : shown.id === "general" ? (
    <GeneralPage others={otherGroups(decls)} />
  ) : shown.id === "appearance" ? (
    <AppearancePage />
  ) : shown.id === "keys" ? (
    <KeysPage />
  ) : shown.id === "agents" ? (
    <AgentsPage />
  ) : shown.id === "plugins" ? (
    <PluginsPage pages={own} />
  ) : shown.id === STORE.id ? (
    <StorePage />
  ) : shown.id === THEMES.id ? (
    <ThemesPage />
  ) : shown.id === SCHEMES.id ? (
    <SchemesPage />
  ) : shown.component && shown.plugin ? (
    <PluginComponent plugin={shown.plugin} component={shown.component} />
  ) : (
    <Page title={shown.title}>
      <GroupedOptions decls={decls.filter((d) => d.plugin === shown.plugin)} own={shown.plugin} />
    </Page>
  )
  return (
    <SettingsContext.Provider value={kit}>
      <div testId="settings-window" style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%", backgroundColor: t.editor }}>
        <Nav groups={groups} page={shown.parent ?? shown.id} searching={searching} query={query} setQuery={setQuery} go={(id) => open(id)} />
        <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0, height: "100%" }}>
          <div style={{ height: TITLEBAR, flexShrink: 0, appRegion: "drag" }} />
          <div key={searching ? "search" : shown.id} testId={`settings-page-${searching ? "search" : shown.id}`} style={{ display: "flex", flexDirection: "column", gap: 20, flexGrow: 1, minHeight: 0, overflowY: fills ? "hidden" : "scroll", scrollbarColor: t.faint, paddingLeft: 32, paddingRight: 32, paddingBottom: 32 }}>
            {safe && shown.id !== "plugins" ? <SafeBanner /> : null}
            {error ? <Note tone="error">{`The server did not answer: ${error}`}</Note> : null}
            {body}
            {searching ? null : <PluginSections page={shown.id} sections={sections} />}
          </div>
          <div style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12, height: 30, flexShrink: 0, paddingLeft: 32, paddingRight: 32, borderTopWidth: 1, borderColor: t.border }}>
            <text style={{ fontSize: ts(12), color: t.faint }}>{`Saved to ${SETTINGS_FILE.replace(process.env.HOME ?? "\u0000", "~")}`}</text>
            {problem ? <text style={{ fontSize: ts(12), color: t.error, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{problem}</text> : null}
          </div>
        </div>
      </div>
    </SettingsContext.Provider>
  )
}
