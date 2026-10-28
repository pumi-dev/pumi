// Settings → Plugins → Browse: the store's index (`registry.list("plugin", {available: true})`,
// the `pluginindex` option) to install from. pumi.dev/plugins is the same on the web.
import { useEffect, useState } from "react"
import { Button, Note, Page, Row, Section, TextField, useSettings, useTextSize } from "../../../sdk/settings/index.ts"
import { useTheme } from "../../../sdk/theme.ts"
import { call } from "../../engine.ts"
import { openPath } from "../shared.tsx"
import { useInstalled } from "./plugins.tsx"

/** The store on the web, for what this page does not show (screenshots, the core plugins). */
export const WEB_STORE = "https://pumi.dev/plugins/"

export interface Listing {
  id?: string
  name?: string
  /** Its name as people read it (package.json's `displayName`), when it has one. */
  displayName?: string
  description?: string
  install?: string
  version?: string
  owner?: string
  tags?: string[]
  keywords?: string[]
  downloads?: number
  downloads7d?: number
  starsDelta7d?: number
  updatedAt?: string
  createdAt?: string
  repo?: { fullName?: string; stars?: number }
}

export const KINDS = [
  ["all", "All"],
  ["plugin", "Plugins"],
  ["theme", "Themes"],
] as const
export type Kind = (typeof KINDS)[number][0]

export const SORTS = [
  ["downloads", "Popular"],
  ["trending", "Trending"],
  ["updated", "Updated"],
  ["new", "New"],
] as const
export type Sort = (typeof SORTS)[number][0]

/** Whether a listing is of `kind`: the store tags each `plugin` and/or `theme`. */
export function holds(kind: Kind, e: Listing): boolean {
  return kind === "all" || (e.tags ?? []).includes(kind)
}

/** Whether a listing has every word of `query` in its id, name, display name, description, tags or keywords. */
export function matches(e: Listing, query: string): boolean {
  const text = [e.id, e.name, e.displayName, e.description, ...(e.tags ?? []), ...(e.keywords ?? [])].filter(Boolean).join(" ").toLowerCase()
  return query.toLowerCase().split(/\s+/).filter(Boolean).every((w) => text.includes(w))
}

/** The listings in the order `sort` puts them, as the store's API sorts. */
export function order(list: Listing[], sort: Sort): Listing[] {
  const n = (e: Listing, k: "downloads" | "downloads7d" | "starsDelta7d") => e[k] ?? 0
  const stars = (e: Listing) => e.repo?.stars ?? 0
  const by = {
    downloads: (a: Listing, b: Listing) => n(b, "downloads") - n(a, "downloads") || stars(b) - stars(a),
    trending: (a: Listing, b: Listing) => n(b, "downloads7d") - n(a, "downloads7d") || n(b, "starsDelta7d") - n(a, "starsDelta7d") || n(b, "downloads") - n(a, "downloads"),
    updated: (a: Listing, b: Listing) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? ""),
    new: (a: Listing, b: Listing) => (b.createdAt ?? "").localeCompare(a.createdAt ?? ""),
  }[sort]
  return [...list].sort(by)
}

/** 1234 as `1.2k`. */
export function count(n: number): string {
  if (n < 1000) return String(n)
  if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, "")}k`
  if (n < 1_000_000) return `${Math.floor(n / 1000)}k`
  return `${(n / 1e6).toFixed(1).replace(/\.0$/, "")}M`
}

/** An ISO time as `today`, `3 days ago`, `2 months ago`, against `now` (ms). */
export function ago(iso: string, now = Date.now()): string | undefined {
  const day = (t: number) => Math.floor(t / 86_400_000)
  const at = Date.parse(iso.slice(0, 10))
  if (!Number.isFinite(at)) return undefined
  const days = day(now) - day(at)
  if (days < 1) return "today"
  if (days === 1) return "yesterday"
  const [n, unit] = days < 14 ? [days, "day"] : days < 60 ? [Math.floor(days / 7), "week"] : days < 730 ? [Math.floor(days / 30), "month"] : [Math.floor(days / 365), "year"]
  return `${n} ${unit}s ago`
}

function Pill({ label, on, onClick }: { label: string; on: boolean; onClick(): void }) {
  const t = useTheme()
  const ts = useTextSize()
  return (
    <div role="tab" aria-selected={on} tabIndex={0} onClick={onClick} style={{ display: "flex", alignItems: "center", height: 24, paddingLeft: 10, paddingRight: 10, borderRadius: 12, backgroundColor: on ? t.selected : "transparent", cursor: "pointer", hover: on ? {} : { backgroundColor: t.hover } }}>
      <text style={{ fontSize: ts(12), color: on ? t.text : t.muted }}>{label}</text>
    </div>
  )
}

export function StorePage() {
  const settings = useSettings()!
  const { list: installed, reload } = useInstalled()
  const [index, setIndex] = useState<{ list?: Listing[]; error?: string }>({})
  const [query, setQuery] = useState("")
  const [kind, setKind] = useState<Kind>("all")
  const [sort, setSort] = useState<Sort>("downloads")
  const [installing, setInstalling] = useState<Set<string>>(new Set())
  const [status, setStatus] = useState<string>()
  const ask = () => {
    setIndex({})
    call("registry.list", "plugin", { available: true }).then(
      (list: Listing[]) => setIndex({ list: list ?? [] }),
      (err: Error) => setIndex({ error: String(err?.message ?? err) }),
    )
  }
  useEffect(ask, [])
  const have = new Set((installed ?? []).map((p) => p.id.toLowerCase()))
  const install = (id: string, spec: string) => {
    setInstalling((s) => new Set(s).add(id))
    setStatus(undefined)
    call("plugin.add", spec, {}).then(
      () => setStatus(`Installed ${id}. Its settings are in Settings → Plugins.`),
      (err: Error) => setStatus(`Could not install ${id}: ${err?.message ?? err}`),
    ).finally(() => {
      setInstalling((s) => {
        const next = new Set(s)
        next.delete(id)
        return next
      })
      reload()
    })
  }
  const shown = order((index.list ?? []).filter((e) => holds(kind, e) && matches(e, query)), sort)
  return (
    <Page title="Plugins and themes" description="Listings are not reviewed, and a plugin has full access to pumi and your computer: install only ones you trust. Any public GitHub repository can be installed: `pumi plugin add owner/repo`.">
      <div style={{ display: "flex", flexDirection: "row", gap: 8 }}>
        <Button ghost icon="arrow_left" onClick={() => settings.open("plugins")}>
          Plugins
        </Button>
        <div style={{ flexGrow: 1 }} />
        <Button ghost icon="arrow_up_right" onClick={() => openPath(WEB_STORE)}>
          Open pumi.dev
        </Button>
      </div>
      <TextField search autoFocus value={query} width="100%" placeholder="Search the store…" onChange={setQuery} onCommit={setQuery} testId="store-search" />
      <div style={{ display: "flex", flexDirection: "row", justifyContent: "space-between" }}>
        <div style={{ display: "flex", flexDirection: "row", gap: 6 }}>
          {KINDS.map(([id, label]) => (
            <Pill key={id} label={label} on={kind === id} onClick={() => setKind(id)} />
          ))}
        </div>
        <div style={{ display: "flex", flexDirection: "row", gap: 4 }}>
          {SORTS.map(([id, label]) => (
            <Pill key={id} label={label} on={sort === id} onClick={() => setSort(id)} />
          ))}
        </div>
      </div>
      {status ? <Note>{status}</Note> : null}
      {!index.list && !index.error ? <Note>Reading the store…</Note> : null}
      {index.error ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <Note tone="error">{`The store could not be read: ${index.error}`}</Note>
          <Button onClick={ask}>Try again</Button>
        </div>
      ) : null}
      {index.list ? (
        <Section>
          {!shown.length ? <Row title={index.list.length ? "Nothing in the store matches." : "Nothing is listed yet."} /> : null}
          {shown.map((e, at) => {
            const id = e.id ?? e.name ?? ""
            const tags = e.tags ?? []
            const what = tags.includes("plugin") && tags.includes("theme") ? "plugin + theme" : tags.includes("theme") ? "theme" : "plugin"
            const owner = e.repo?.fullName?.split("/")[0] ?? e.owner
            const stats = [`↓ ${count(e.downloads ?? 0)}`, e.repo?.stars ? `★ ${count(e.repo.stars)}` : undefined, e.updatedAt ? `updated ${ago(e.updatedAt)}` : undefined, owner ? `by ${owner}` : undefined].filter(Boolean).join(" · ")
            const busy = installing.has(id)
            return (
              <Row
                key={`${id}-${at}`}
                title={e.displayName || e.name || id}
                aside={[what, e.version ? `v${e.version}` : undefined].filter(Boolean).join(" · ")}
                description={e.description ? `${e.description}\n${stats}` : stats}
                control={
                  <>
                    <Button ghost icon="arrow_up_right" title="Its page on pumi.dev" onClick={() => openPath(`${WEB_STORE}${id}/`)} />
                    {have.has(id.toLowerCase()) ? (
                      <Note>Installed</Note>
                    ) : (
                      <Button primary loading={busy} onClick={() => install(id, e.install ?? id)}>
                        {busy ? "Installing" : "Install"}
                      </Button>
                    )}
                  </>
                }
              />
            )
          })}
        </Section>
      ) : null}
    </Page>
  )
}
