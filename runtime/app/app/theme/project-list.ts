// The projects list under the project button: the projects this window has open, the app's recent
// ones, and under them the git projects found on this Mac (`git-projects.ts`) — a project the user
// has not opened here yet but was working in. The search field takes all of them at once (a
// project by its label or its folder, a found one by its name or its path), so what the user is
// looking for is most likely a keystroke away, not a folder they have to find.
import type { GitProject } from "./git-projects.ts"

/** A project pumi has: its id, what it is called, and its folder. */
export interface Listed {
  id: string
  label?: string | null
  /** Its folder: what a found git project at it is the same as. */
  path?: string
}

export interface Listing {
  /** The projects this window has open, in the order it opened them. */
  here: string[]
  /** The projects the app showed last, most recent first. */
  recent: string[]
  /** Every project there is: only those are listed. */
  projects: Listed[]
  /** The project the window shows now. */
  current?: string
  /** The git projects found on this Mac, under the ones pumi remembers. */
  found?: GitProject[]
  /** What the search field holds. */
  query: string
  /** A path as the list writes it (`~/src/app`). */
  short(path: string): string
}

/** A project of pumi's: one this window has open, or a recent one. */
export type ProjectRow = { kind: "here" | "recent"; id: string; label: string; path?: string; current: boolean }
/** A git project found on this Mac, not pumi's yet: its name, and its path to tell it from others. */
export type FoundRow = { kind: "found"; id: string; label: string; detail: string; path: string }
export type Row = ProjectRow | FoundRow

export interface Rows {
  here: ProjectRow[]
  recent: ProjectRow[]
  found: FoundRow[]
}

/** Whether `path` is `under` or inside it: the same project, or one in its folder. */
export function within(path: string, under: string): boolean {
  const self = path.replace(/\/+$/, "")
  const parent = under.replace(/\/+$/, "")
  return self === parent || self.startsWith(`${parent}/`)
}

/** The git projects none of `paths` is (a project, or a folder of one): what the list has to add. */
export function beyond(found: GitProject[], paths: (string | undefined)[]): GitProject[] {
  const known = paths.filter((p): p is string => !!p)
  return found.filter((g) => !known.some((path) => within(g.path, path)))
}

/** The list's rows, the remembered projects first, `query` applied to every one of them. */
export function projectRows({ here, recent, projects, current, found, query, short }: Listing): Rows {
  const byId = new Map(projects.map((p) => [p.id, p]))
  const q = query.trim().toLowerCase()
  const matches = (label: string, detail = "") => !q || label.toLowerCase().includes(q) || detail.toLowerCase().includes(q)
  const open = [...here, ...(current && !here.includes(current) ? [current] : [])].filter((id) => byId.has(id))
  const rest = [...recent, ...projects.map((p) => p.id)].filter((id, i, all) => byId.has(id) && !open.includes(id) && all.indexOf(id) === i)
  const entry = (kind: "here" | "recent") => (id: string): ProjectRow => {
    const p = byId.get(id)!
    return { kind, id, label: p.label || id, path: p.path, current: id === current }
  }
  // A project the list already shows (or a project inside one it shows) is not added again.
  const listed = (rows: ProjectRow[]) => rows.filter((r) => matches(r.label, r.path ? short(r.path) : ""))
  const hereRows = listed(open.map(entry("here")))
  const recentRows = listed(rest.map(entry("recent")))
  const foundRows: FoundRow[] = beyond(found ?? [], [...hereRows, ...recentRows].map((r) => r.path))
    .map((g): FoundRow => ({ kind: "found", id: g.path, label: g.name, detail: short(g.path), path: g.path }))
    .filter((r) => matches(r.label, r.detail))
  return { here: hereRows, recent: recentRows, found: foundRows }
}
