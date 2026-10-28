// Settings → Plugins, as Obsidian's community plugins page has it: what to do (safe mode, the
// store, checking for updates), the plugins' own options, then the installed plugins, each with
// its switch, its settings, and a menu of the rest.
import { dirname, join } from "node:path"
import { createElement, useCallback, useEffect, useRef, useState } from "react"
import { Engine } from "../../../native/addon.js"
import type { Action } from "../../../sdk/actions.ts"
import { confirm } from "../../../sdk/dialogs.ts"
import { Button, Note, Page, Row, Section, Spinner, Switch, TextField, useSettings } from "../../../sdk/settings/index.ts"
import { useComponents } from "../../../sdk/theme.ts"
import { subscribeEvent } from "../../../sdk/events.ts"
import { call, engine, restartApp } from "../../engine.ts"
import { safe } from "../../mode.ts"
import type { PageInfo } from "../pages.ts"
import { GroupSection, openPath, showLog, useUpdates } from "../shared.tsx"

/** One installed plugin, as `registry.list("plugin")` describes it. */
export interface Installed {
  name: string
  /** What people read it as: its package.json's `displayName`, else `name`. */
  title: string
  /** `owner/name`, or the name while it has no manifest. */
  id: string
  owner?: string
  version?: string
  description: string
  on: boolean
  /** Where it is: a linked one's own directory, else its copy in the plugins folder. */
  dir?: string
  src?: string
  /** It ships with pumi: it turns off, and is never uninstalled. */
  bundled: boolean
  path?: string
}

export function installedOf(p: any): Installed {
  const name: string = p?.spec?.name ?? p?.name ?? ""
  const id: string = p?.id ?? name
  const owner = id.includes("/") ? id.split("/")[0] : undefined
  const version = p?.meta?.version ? `v${p.meta.version}` : typeof p?.rev === "string" ? p.rev.slice(0, 8) : undefined
  const description = p?.error ? `Cannot be read: ${p.error}` : (p?.meta?.description ?? "")
  const src: string | undefined = p?.spec?.src
  return {
    name,
    title: (typeof p?.meta?.displayName === "string" && p.meta.displayName) || name,
    id,
    owner: owner === "local" ? undefined : owner,
    version,
    description,
    on: p?.disabled !== true,
    dir: p?.kind === "link" ? src : p?.path,
    src,
    bundled: p?.bundled === true,
    path: p?.path,
  }
}

/** Its version and who made it, as its row says after its name. */
export function asideOf(p: Installed): string {
  return [p.version, p.bundled ? "Bundled with pumi" : p.owner ? `By ${p.owner}` : undefined].filter(Boolean).join(" ")
}

/** Whether every word of `query` is in its name, title, id or description. */
export function matchesInstalled(p: Installed, query: string): boolean {
  return query.toLowerCase().split(/\s+/).filter(Boolean).every((w) => [p.name, p.title, p.id, p.description].some((t) => t.toLowerCase().includes(w)))
}

/** The installed plugins, read again when one is installed, removed or reloaded. */
export function useInstalled(): { list?: Installed[]; error?: string; reload(): void } {
  const [got, setGot] = useState<{ list?: Installed[]; error?: string }>({})
  const reload = useCallback(() => {
    call("registry.list", "plugin", {}).then(
      (list: unknown[]) => setGot({ list: (list ?? []).map(installedOf) }),
      (err: Error) => setGot((was) => ({ ...was, error: String(err?.message ?? err) })),
    )
  }, [])
  useEffect(() => {
    reload()
    return subscribeEvent("registry.changed", undefined, reload)
  }, [reload])
  return { ...got, reload }
}

/** Where the server keeps the plugins: beside an installed one, else the data directory's. */
function pluginsFolder(list: Installed[] | undefined): string {
  const beside = list?.find((p) => p.path)?.path
  return beside ? dirname(beside) : join(Engine.stdpath("data") ?? ".", "installed")
}

/** Runs `calls` in order. `doing` is shown above the list while they run; `id` is the plugin they
 * are about, whose row turns a spinner meanwhile. */
type Run = (doing: string | undefined, calls: [string, unknown[]][], updated?: string[], id?: string) => void

function PluginMenu({ p, at, close, busy, run, uninstall }: { p: Installed; at: { x: number; y: number }; close(): void; busy: boolean; run: Run; uninstall(): void }) {
  const C = useComponents()
  const settings = useSettings()
  const item = (id: string, title: string, icon: string, go: () => void, more: Partial<Action> = {}): Action => ({ id, title, icon, plugin: "pumi", run: () => (close(), go()), ...more })
  const actions: Action[] = [
    item("reload", "Reload", "rotate_cw", () => run(`Reloading ${p.title}…`, [["plugin.update", [p.id, {}]]]), { disabled: busy }),
    item("log", "Show log", "terminal", () => {
      settings?.report(showLog(p.id))
    }),
    item("folder", "Open folder", "folder_open", () => p.dir && openPath(p.dir), { disabled: !p.dir }),
    item("source", "Open its source", "arrow_up_right", () => p.src && openPath(p.src), { disabled: !p.src?.startsWith("http") }),
    item("uninstall", "Uninstall…", "close", uninstall, { danger: true, disabled: p.bundled, group: "end" }),
  ]
  return createElement(C.Menu, { at, actions, close })
}

function InstalledRow({ p, pages, busy, pending, run }: { p: Installed; pages: PageInfo[]; busy: boolean; pending: boolean; run: Run }) {
  const settings = useSettings()!
  const { plugins } = useUpdates()
  const [menu, setMenu] = useState<{ x: number; y: number }>()
  const pressedAt = useRef({ x: 0, y: 0 })
  const update = plugins.found?.find((u) => u.name === p.name || u.name === p.id)
  const page = pages.find((x) => x.plugin === p.id)
  const uninstall = async () => {
    const yes = await confirm({ title: `Uninstall ${p.title}?`, detail: "Its settings and data stay.", confirm: "Uninstall", danger: true })
    if (yes) run(`Uninstalling ${p.title}…`, [["plugin.remove", [p.id, {}]]])
  }
  return (
    <Row
      link={p.name}
      testId={`plugin-${p.id}`}
      title={p.title}
      aside={asideOf(p)}
      description={p.description}
      control={
        <>
          {update ? (
            <Button primary title={update.to ? `Update to ${update.to}` : "Update"} onClick={() => run(`Updating ${p.title}…`, [["plugin.update", [p.id, {}]]], [p.name])}>
              Update
            </Button>
          ) : null}
          {page ? <Button ghost icon="settings" title="Settings" onClick={() => settings.open(page.id)} /> : null}
          {/* Where the press was: the menu opens there. */}
          <div onMouseDown={(e) => (pressedAt.current = { x: e.x ?? 0, y: e.y ?? 0 })}>
            <Button ghost icon="ellipsis" title="More" onClick={() => setMenu(menu ? undefined : pressedAt.current)} testId={`plugin-menu-${p.id}`} />
          </div>
          {pending ? <Spinner testId={`plugin-pending-${p.id}`} /> : null}
          <Switch checked={p.on} disabled={pending} aria-label={`${p.title} on`} testId={`plugin-on-${p.id}`} onChange={(on) => run(undefined, [["plugin.update", [p.id, { enabled: on }]]], [], p.id)} />
          {menu ? <PluginMenu p={p} at={menu} busy={busy} close={() => setMenu(undefined)} run={run} uninstall={uninstall} /> : null}
        </>
      }
    />
  )
}

export function PluginsPage({ pages }: { pages: PageInfo[] }) {
  const settings = useSettings()!
  const { list, error, reload } = useInstalled()
  const { plugins } = useUpdates()
  const [query, setQuery] = useState("")
  const [status, setStatus] = useState<string>()
  const [busy, setBusy] = useState(false)
  const [pending, setPending] = useState<string>()
  const run: Run = (doing, calls, updated = [], id) => {
    setStatus(doing)
    setBusy(true)
    setPending(id)
    ;(async () => {
      for (const [method, args] of calls) await call(method, ...args)
    })().then(
      () => {
        if (updated.length) engine.pluginUpdatesApplied(updated)
        setStatus(updated.length ? (updated.length === 1 ? "Updated." : `${updated.length} plugins updated.`) : undefined)
      },
      (err: Error) => setStatus(String(err?.message ?? err)),
    ).finally(() => {
      setBusy(false)
      setPending(undefined)
      reload()
    })
  }
  const waiting = plugins.found ?? []
  const count = list ? (list.length === 1 ? "You have 1 plugin installed." : `You have ${list.length} plugins installed.`) : ""
  const found = plugins.checking
    ? "Checking for updates…"
    : plugins.error
      ? `The check failed: ${plugins.error}`
      : plugins.found
        ? waiting.length === 0
          ? "Every one is up to date."
          : waiting.length === 1
            ? "1 has an update."
            : `${waiting.length} have updates.`
        : ""
  const shown = (list ?? []).filter((p) => matchesInstalled(p, query))
  return (
    <Page title="Plugins">
      <Section>
        <Row
          link="safe-mode"
          title={safe ? "Safe mode is on" : "Safe mode"}
          description={safe ? "No plugin runs, and the window has the default theme." : "Use the app without any plugin and with the default theme, to tell whether one causes a problem."}
          control={<Button onClick={() => restartApp({ safe: !safe })}>{safe ? "Turn off and restart" : "Turn on and restart"}</Button>}
        />
        <Row
          link="store"
          title="Community plugins"
          description="Find and install themes, panels and tools made by the community. A plugin has full access to pumi and your computer, so install only ones you trust."
          control={<Button onClick={() => settings.open("store")}>Browse</Button>}
        />
        <Row
          link="plugin-updates"
          title="Current plugins"
          description={`${count} ${found}`.trim()}
          control={
            <>
              <Button loading={plugins.checking} onClick={() => engine.checkForUpdates(true)}>
                Check for updates
              </Button>
              {waiting.length ? (
                <Button primary loading={busy} onClick={() => run("Updating…", waiting.map((u) => ["plugin.update", [u.name, {}]] as [string, unknown[]]), waiting.map((u) => u.name))}>
                  Update all
                </Button>
              ) : null}
            </>
          }
        />
      </Section>
      <GroupSection group="Plugins" title="Updates" />
      {status ? <Note>{status}</Note> : null}
      <Section
        title="Installed plugins"
        actions={
          <>
            <Button ghost icon="rotate_cw" title="Read the installed plugins again" onClick={() => (setStatus(undefined), reload())} />
            <Button ghost icon="folder_open" title="Open the plugins folder" onClick={() => openPath(pluginsFolder(list))} />
          </>
        }
      >
        {list?.length ? (
          <div style={{ display: "flex", flexDirection: "column", paddingTop: 12, paddingBottom: 4 }}>
            <TextField search value={query} width="100%" placeholder="Search installed plugins…" onChange={setQuery} onCommit={setQuery} />
          </div>
        ) : null}
        {!list ? <Row title={error ? `The server did not answer: ${error}` : "Asking the server…"} /> : null}
        {list && !list.length ? <Row title="No plugins installed" description="Browse the community's, or install one with `pumi plugin add <url>`." /> : null}
        {list?.length && !shown.length ? <Row title="No installed plugin matches." /> : null}
        {shown.map((p) => (
          <InstalledRow key={p.id} p={p} pages={pages} busy={busy} pending={pending === p.id} run={run} />
        ))}
      </Section>
    </Page>
  )
}
