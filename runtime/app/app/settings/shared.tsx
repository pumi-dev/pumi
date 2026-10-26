// What Settings' pages share: a plugin's component under its own boundary, a plugin's sections on a
// page, a page made of a plugin's options, the option groups, and the updater's state.
import { existsSync } from "node:fs"
import { Suspense, useEffect, useState, useSyncExternalStore, type ReactNode } from "react"
import { Engine } from "../../native/addon.js"
import { Boundary, PluginContext } from "../../sdk/contributions.ts"
import { Button, Note, OptionRow, Section, useReveal, useSettings, useTextSize } from "../../sdk/settings/index.ts"
import { groupOf, type Decl } from "../../sdk/settings/options.ts"
import { useTheme, withOpacity } from "../../sdk/theme.ts"
import { call, engine, on } from "../engine.ts"
import * as plugins from "../plugins.ts"
import { settingsComponent } from "../ui.ts"
import type { PluginSettings, SectionInfo } from "./pages.ts"

/** Opens a file or a web page in the app the system gives it. */
export function openPath(path: string) {
  call("fs.open", path).catch((err: Error) => console.error(`pumi: opening ${path}:`, err?.message ?? err))
}

/** Shows plugin `id`'s log, when it has written one. */
export function showLog(id: string): string | undefined {
  const path = Engine.logPath(id)
  if (!existsSync(path)) return `It has written no log yet (${path}).`
  openPath(path)
  return undefined
}

let pluginsVersion = 0
plugins.subscribe(() => pluginsVersion++)

/** What each plugin's manifest says of Settings, read again as the plugins change. */
export function usePluginSettings(): PluginSettings[] {
  const version = useSyncExternalStore(
    (l) => plugins.subscribe(l),
    () => pluginsVersion,
  )
  const [list, setList] = useState<{ version: number; list: PluginSettings[] }>()
  if (list?.version !== version) {
    const next = [...plugins.entries.values()].map((e) => ({
      id: e.id,
      prefix: e.prefix,
      name: e.manifest.name ?? e.id.split("/").pop() ?? e.id,
      pages: e.manifest.contributes?.settings?.pages,
      sections: e.manifest.contributes?.settings?.sections,
    }))
    setList({ version, list: next })
    return next
  }
  return list.list
}

/** What Settings draws instead of a plugin's component that threw: whose it is, why, and its log. */
function ErrorCard({ plugin, error }: { plugin: string; error: string }) {
  const t = useTheme()
  const ts = useTextSize()
  const [note, setNote] = useState<string>()
  return (
    <div testId={`settings-error-${plugin}`} style={{ display: "flex", flexDirection: "column", gap: 8, padding: 14, borderRadius: 10, borderWidth: 1, borderColor: t.error, backgroundColor: withOpacity(t.error, 0.08) }}>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 4, flexShrink: 1, minWidth: 0 }}>
          <text style={{ fontSize: ts(14), fontWeight: 600, color: t.text }}>{`${plugin} could not draw its settings`}</text>
          <text style={{ fontSize: ts(12), color: t.muted }}>{error}</text>
        </div>
        <Button onClick={() => setNote(showLog(plugin))}>Show logs</Button>
      </div>
      {note ? <Note>{note}</Note> : null}
    </div>
  )
}

/** Plugin `plugin`'s export `component`, drawn in an error boundary of its own: a throw is an error card, and Settings goes on. */
export function PluginComponent({ plugin, component }: { plugin: string; component: string }) {
  const Component = settingsComponent(plugin, component)
  if (!Component) return <ErrorCard plugin={plugin} error="Its window side (src/ui.tsx) is not loaded: it is off, or it froze the app and is left out until pumi starts again." />
  return (
    <PluginContext.Provider value={plugin}>
      <Boundary key={`${plugin}/${component}`} name={plugin} fallback={(error) => <ErrorCard plugin={plugin} error={error.message} />}>
        <Suspense fallback={<Note>Loading…</Note>}>
          <Component />
        </Suspense>
      </Boundary>
    </PluginContext.Provider>
  )
}

/** One plugin section, scrolled to and outlined while a link or search names it. */
function PluginSection({ section }: { section: SectionInfo }) {
  const t = useTheme()
  const [marked, reveal] = useReveal(section.id)
  return (
    <div ref={reveal} testId={`settings-section-${section.id}`} style={{ display: "flex", flexDirection: "column", ...(marked ? { outlineWidth: 2, outlineColor: t.accent, outlineOffset: 4, borderRadius: 10 } : {}) }}>
      <PluginComponent plugin={section.plugin} component={section.component} />
    </div>
  )
}

/** The plugins' sections on `page`, after what the page draws itself. */
export function PluginSections({ page, sections }: { page: string; sections: SectionInfo[] }) {
  const mine = sections.filter((s) => s.page === page)
  if (!mine.length) return null
  return (
    <>
      {mine.map((s) => (
        <PluginSection key={s.id} section={s} />
      ))}
    </>
  )
}

/** Options in sections by their group: a plugin's, each heading without the plugin's own name. */
export function GroupedOptions({ decls, own, description }: { decls: Decl[]; own?: string; description?: string }) {
  const groups: { title: string; decls: Decl[] }[] = []
  for (const d of decls) {
    let title = groupOf(d)
    if (own && title.startsWith(own)) title = title.slice(own.length).replace(/^ · /, "")
    if (title === d.plugin) title = ""
    const last = groups.at(-1)
    if (last && last.title === title) last.decls.push(d)
    else groups.push({ title, decls: [d] })
  }
  return (
    <>
      {groups.map((g, at) => (
        <Section key={`${g.title}-${at}`} title={g.title || undefined}>
          {g.decls.map((d) => (
            <OptionRow key={d.name} name={d.name} description={description} />
          ))}
        </Section>
      ))}
    </>
  )
}

/** The options of `group` Settings shows, but those its page draws itself (`skip`). */
export function useGroup(group: string, skip: string[] = []): Decl[] {
  const settings = useSettings()
  return (settings?.decls ?? []).filter((d) => !d.plugin && groupOf(d) === group && !skip.includes(d.name))
}

/** A section of a built-in option group, when it has any options to show. */
export function GroupSection({ group, title, skip, description, children }: { group: string; title?: string; skip?: string[]; description?: string; children?: ReactNode }) {
  const decls = useGroup(group, skip)
  if (!decls.length && !children) return null
  return (
    <Section title={title ?? group}>
      {children}
      {decls.map((d) => (
        <OptionRow key={d.name} name={d.name} description={description} />
      ))}
    </Section>
  )
}

/** The updater, as `Engine.updateState` says and each `update` event tells again. */
export interface UpdateState {
  app: { status: string; version?: string | null; error?: string | null; current: string; channel: string; text: string; canCheck: boolean; busy: boolean }
  plugins: { checking: boolean; found: { name: string; from?: string; to?: string }[] | null; error: string | null }
}

const NO_UPDATES: UpdateState = {
  app: { status: "unavailable", current: "", channel: "", text: "", canCheck: false, busy: false },
  plugins: { checking: false, found: null, error: null },
}

function readUpdates(): UpdateState {
  try {
    return (engine.updateState() as UpdateState) ?? NO_UPDATES
  } catch (err) {
    console.error("pumi: the updater's state:", (err as Error)?.message ?? err)
    return NO_UPDATES
  }
}

export function useUpdates(): UpdateState {
  const [state, setState] = useState<UpdateState>(readUpdates)
  useEffect(() => on("update", (data: UpdateState) => setState(data ?? readUpdates())), [])
  return state
}
