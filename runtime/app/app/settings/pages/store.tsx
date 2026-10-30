// Settings → Plugins → Browse: the store's index (`registry.list("plugin", {available: true})`,
// the `pluginindex` option) to install from. pumi.dev/plugins is the same on the web.
import { createHash } from "node:crypto"
import { existsSync, mkdirSync, renameSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { useEffect, useState } from "react"
import type { EventPayload } from "../../../native/addon"
import { Engine } from "../../../native/addon.js"
import { Button, gridColumns, Note, Page, TextField, useSettings, useTextSize } from "../../../sdk/settings/index.ts"
import { useMetrics, useTheme } from "../../../sdk/theme.ts"
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
  /** Its icon's URL, a square picture. */
  icon?: string | null
  /** Its pictures (the manifest's `images`), the first the card shows. */
  images?: { url?: string; alt?: string }[]
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

/** Where the store's pictures are kept. Their URLs name the commit they are at, so a file once
 *  fetched is never stale. */
const IMAGES = join(Engine.stdpath("cache") ?? tmpdir(), "store-images")
/** A picture larger than this is not shown. */
const MAX_IMAGE = 4 * 1024 * 1024
const fetching = new Map<string, Promise<string | undefined>>()

/** The file `url` is kept in: its hash, with its extension (an SVG is drawn by its name). */
export function imageFile(url: string): string {
  const ext = /\.(svg|png|jpe?g|gif|webp)$/i.exec(new URL(url).pathname)?.[1]?.toLowerCase() ?? "img"
  return join(IMAGES, `${createHash("sha256").update(url).digest("hex").slice(0, 32)}.${ext}`)
}

/** A store picture as a file the renderer can draw (it does not fetch URLs): fetched once. */
export function storeImage(url: string): Promise<string | undefined> {
  if (!/^https:\/\//i.test(url)) return Promise.resolve(undefined)
  const file = imageFile(url)
  if (existsSync(file)) return Promise.resolve(file)
  let found = fetching.get(url)
  if (!found) {
    found = fetch(url)
      .then(async (res) => {
        if (!res.ok) return undefined
        const bytes = new Uint8Array(await res.arrayBuffer())
        if (!bytes.length || bytes.length > MAX_IMAGE) return undefined
        mkdirSync(IMAGES, { recursive: true })
        const part = `${file}.${process.pid}.part`
        writeFileSync(part, bytes)
        renameSync(part, file)
        return file
      })
      .catch(() => undefined)
      .finally(() => fetching.delete(url))
    fetching.set(url, found)
  }
  return found
}

function useStoreImage(url: string | null | undefined): string | undefined {
  const [file, setFile] = useState<string | undefined>(() => (url && /^https:\/\//i.test(url) && existsSync(imageFile(url)) ? imageFile(url) : undefined))
  useEffect(() => {
    let live = true
    setFile(undefined)
    if (url) storeImage(url).then((f) => live && setFile(f))
    return () => {
      live = false
    }
  }, [url])
  return file
}

/** The narrowest a card is; it widens to fill its row. */
const CARD_MIN = 240
const CARD_GAP = 12
/** A preview's shape: the store's pictures are 320 by 200. */
const PREVIEW_RATIO = 200 / 320

function StoreCard({ e, width, installed, busy, onInstall }: { e: Listing; width: number; installed: boolean; busy: boolean; onInstall(): void }) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const id = e.id ?? e.name ?? ""
  const title = e.displayName || e.name || id
  const picture = e.images?.find((i) => i.url)
  const preview = useStoreImage(picture?.url)
  const icon = useStoreImage(e.icon)
  const tags = e.tags ?? []
  const what = tags.includes("plugin") && tags.includes("theme") ? "plugin + theme" : tags.includes("theme") ? "theme" : "plugin"
  const owner = e.repo?.fullName?.split("/")[0] ?? e.owner
  const stats = [`↓ ${count(e.downloads ?? 0)}`, e.repo?.stars ? `★ ${count(e.repo.stars)}` : undefined, e.updatedAt ? ago(e.updatedAt) : undefined, owner ? `by ${owner}` : undefined].filter(Boolean).join(" · ")
  const tall = Math.round(width * PREVIEW_RATIO)
  return (
    <div testId={`store-card-${id}`} aria-label={title} style={{ display: "flex", flexDirection: "column", width, flexShrink: 0, borderRadius: m.radius + 2, borderWidth: 1, borderColor: t.border, backgroundColor: t.surface, overflow: "hidden" }}>
      <div style={{ display: "flex", width: "100%", height: tall, flexShrink: 0, alignItems: "center", justifyContent: "center", backgroundColor: t.editor, borderBottomWidth: 1, borderColor: t.border, overflow: "hidden" }}>
        {preview ? (
          <img src={preview} alt={picture?.alt ?? title} objectFit="cover" style={{ width: "100%", height: tall }} />
        ) : icon ? (
          <img src={icon} alt="" objectFit="contain" style={{ width: Math.round(tall * 0.45), height: Math.round(tall * 0.45) }} />
        ) : (
          <text style={{ fontSize: ts(16), color: t.faint }}>{title}</text>
        )}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 6, padding: 12, flexGrow: 1 }}>
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          {icon ? <img src={icon} alt="" objectFit="contain" style={{ width: 22, height: 22, flexShrink: 0 }} /> : null}
          <text style={{ fontSize: ts(14), fontWeight: 600, color: t.text, flexShrink: 1, minWidth: 0, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{title}</text>
          <text style={{ fontSize: ts(12), color: t.faint, flexShrink: 0, whiteSpace: "nowrap" }}>{[what, e.version ? `v${e.version}` : undefined].filter(Boolean).join(" · ")}</text>
        </div>
        {e.description ? <text style={{ fontSize: ts(12), color: t.muted, lineClamp: 2, textOverflow: "ellipsis", overflow: "hidden" }}>{e.description}</text> : null}
        <div style={{ flexGrow: 1 }} />
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
          <text style={{ fontSize: ts(12), color: t.faint, flexGrow: 1, flexShrink: 1, minWidth: 0, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{stats}</text>
          <Button ghost icon="arrow_up_right" title="Its page on pumi.dev" onClick={() => openPath(`${WEB_STORE}${id}/`)} />
          {installed ? (
            <Note>Installed</Note>
          ) : (
            <Button primary loading={busy} onClick={onInstall} testId={`store-install-${id}`}>
              {busy ? "Installing" : "Install"}
            </Button>
          )}
        </div>
      </div>
    </div>
  )
}

/** The listings as cards, as many in a row as fit. */
function StoreGrid({ list, have, installing, install }: { list: Listing[]; have: Set<string>; installing: Set<string>; install(id: string, spec: string): void }) {
  const [width, setWidth] = useState(0)
  const layout = gridColumns(width, CARD_MIN)
  const rows: Listing[][] = []
  for (let i = 0; i < list.length; i += layout.columns) rows.push(list.slice(i, i + layout.columns))
  return (
    <div testId="store-grid" onLayout={(e: EventPayload & { width?: number }) => e.width != null && e.width !== width && setWidth(e.width)} style={{ display: "flex", flexDirection: "column", gap: CARD_GAP, width: "100%" }}>
      {rows.map((row, r) => (
        <div key={r} style={{ display: "flex", flexDirection: "row", alignItems: "stretch", gap: CARD_GAP }}>
          {row.map((e, at) => {
            const id = e.id ?? e.name ?? ""
            return <StoreCard key={`${id}-${at}`} e={e} width={layout.width} installed={have.has(id.toLowerCase())} busy={installing.has(id)} onInstall={() => install(id, e.install ?? id)} />
          })}
        </div>
      ))}
    </div>
  )
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
        shown.length ? <StoreGrid list={shown} have={have} installing={installing} install={install} /> : <Note>{index.list.length ? "Nothing in the store matches." : "Nothing is listed yet."}</Note>
      ) : null}
    </Page>
  )
}
