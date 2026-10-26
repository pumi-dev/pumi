// How the default theme arranges a place's sections (`panel`): in groups, each a page of the side
// bar with a button of its own in the bar at its foot, as VS Code's view containers are. A section
// is dragged within its group to sort it, onto another group's button to join it, or onto the
// bar's end to be a group of its own; a button is dragged along the bar to sort the groups. What
// is kept is the order of every section ever seen, so one whose plugin is off comes back where it
// was. The tree of sessions is a section (`TREE`) with no header: it is never dragged, and heads
// whichever group holds it, so nothing is dropped above it. What a page's open parts are is shares
// of its height, the edge between two dragged to trade them. Nothing here draws: `default.tsx` does.

/** A page of the side bar: its sections, top to bottom, by key (`tree`, `owner/name/component`). */
export interface Group {
  id: string
  sections: string[]
}

/** A place's groups, left to right along the bar, and the one shown. */
export interface Arrangement {
  groups: Group[]
  active?: string
}

export const NO_ARRANGEMENT: Arrangement = { groups: [] }

let made = 0

/** A group's id: new each time, and not one a stored arrangement already holds. */
function newId(taken: Arrangement): string {
  for (;;) {
    const id = `g${Date.now().toString(36)}${(made++).toString(36)}`
    if (!taken.groups.some((g) => g.id === id)) return id
  }
}

/** The tree's key, as a section and among a page's shares: no plugin's (`owner/name/component`). */
export const TREE = "tree"

/** Groups that hold nothing go, the tree heads the one it is in, and the one shown is one there is. */
function tidy(arrangement: Arrangement): Arrangement {
  const groups = arrangement.groups
    .filter((g) => g.sections.length > 0)
    .map((g) => (g.sections.indexOf(TREE) > 0 ? { ...g, sections: [TREE, ...g.sections.filter((s) => s !== TREE)] } : g))
  const active = groups.some((g) => g.id === arrangement.active) ? arrangement.active : groups[0]?.id
  return { groups, active }
}

/** Whether the stored value is an arrangement (window.json is the user's to edit). */
function valid(value: unknown): value is Arrangement {
  const groups = (value as Arrangement | undefined)?.groups
  return Array.isArray(groups) && groups.every((g) => typeof g?.id === "string" && Array.isArray(g.sections) && g.sections.every((s) => typeof s === "string"))
}

/**
 * The stored arrangement with every section of `present` in it: a section not seen before joins
 * the first group (a new one when there is none). A section seen in two groups stays in the first.
 */
export function arrange(stored: unknown, present: readonly string[]): Arrangement {
  const base: Arrangement = valid(stored) ? stored : NO_ARRANGEMENT
  const seen = new Set<string>()
  const groups = base.groups.map((g) => ({
    id: g.id,
    sections: g.sections.filter((s) => (seen.has(s) ? false : (seen.add(s), true))),
  }))
  const fresh = present.filter((s) => !seen.has(s))
  if (fresh.length) {
    if (groups[0]) groups[0] = { ...groups[0], sections: [...groups[0].sections, ...fresh] }
    // The same id each time: an arrangement made from nothing is made again on every edit.
    else groups.push({ id: "main", sections: [...fresh] })
  }
  return tidy({ groups, active: base.active })
}

/** The groups as drawn: only the sections in `present`, and only groups left with any. */
export function shown(arrangement: Arrangement, present: readonly string[]): Group[] {
  const here = new Set(present)
  return arrangement.groups.map((g) => ({ id: g.id, sections: g.sections.filter((s) => here.has(s)) })).filter((g) => g.sections.length > 0)
}

function without(arrangement: Arrangement, section: string): Group[] {
  return arrangement.groups.map((g) => ({ ...g, sections: g.sections.filter((s) => s !== section) }))
}

/**
 * `section` moved into `group`, before `before` (at its end without one, or when `before` is not
 * in it). The group it left goes if it is empty; the one shown stays shown, or is `group` when the
 * one shown is left empty.
 */
export function moveSection(arrangement: Arrangement, section: string, group: string, before?: string): Arrangement {
  if (section === before || !arrangement.groups.some((g) => g.id === group)) return arrangement
  const groups = without(arrangement, section).map((g) => {
    if (g.id !== group) return g
    const at = before === undefined ? -1 : g.sections.indexOf(before)
    const sections = [...g.sections]
    sections.splice(at < 0 ? sections.length : at, 0, section)
    return { ...g, sections }
  })
  const left = groups.find((g) => g.id === arrangement.active)
  return tidy({ groups, active: left && left.sections.length ? arrangement.active : group })
}

/** `section` taken out to a group of its own, before the group `before` (last without one), and shown. */
export function splitOut(arrangement: Arrangement, section: string, before?: string): Arrangement {
  const from = arrangement.groups.find((g) => g.sections.includes(section))
  // Alone in its group already: the group is what moves.
  if (from && from.sections.length === 1) return { ...moveGroup(arrangement, from.id, before), active: from.id }
  const id = newId(arrangement)
  const groups = without(arrangement, section)
  const at = before === undefined ? -1 : groups.findIndex((g) => g.id === before)
  groups.splice(at < 0 ? groups.length : at, 0, { id, sections: [section] })
  return tidy({ groups, active: id })
}

/** `group` moved along the bar, before `before` (last without one). */
export function moveGroup(arrangement: Arrangement, group: string, before?: string): Arrangement {
  const moving = arrangement.groups.find((g) => g.id === group)
  if (!moving || group === before) return arrangement
  const groups = arrangement.groups.filter((g) => g.id !== group)
  const at = before === undefined ? -1 : groups.findIndex((g) => g.id === before)
  groups.splice(at < 0 ? groups.length : at, 0, moving)
  return tidy({ groups, active: arrangement.active })
}

/** Every section of `group` moved into `into`, after its own; `into` is shown. */
export function mergeGroup(arrangement: Arrangement, group: string, into: string): Arrangement {
  const moving = arrangement.groups.find((g) => g.id === group)
  if (!moving || group === into || !arrangement.groups.some((g) => g.id === into)) return arrangement
  const groups = arrangement.groups
    .filter((g) => g.id !== group)
    .map((g) => (g.id === into ? { ...g, sections: [...g.sections, ...moving.sections] } : g))
  return tidy({ groups, active: into })
}

/** The group shown. */
export function show(arrangement: Arrangement, group: string): Arrangement {
  return arrangement.groups.some((g) => g.id === group) ? { ...arrangement, active: group } : arrangement
}

/** A page's open parts' shares of its height, by key: what is not in it has one share. */
export type Shares = Record<string, number>

/** What `key` is in shares (window.json is the user's to edit: what is not a share is one). */
export function shareOf(shares: unknown, key: string): number {
  const n = (shares as Shares | undefined)?.[key]
  return typeof n === "number" && Number.isFinite(n) && n > 0 ? n : 1
}

/**
 * How much of the page each of `open` grows by: its share over all of theirs, so they always fill
 * it. Shares are kept per place, not per group, and flex grows that sum under 1 leave the rest of
 * the page empty (a lone section kept at 0.8 of what it once split would fill only 80%).
 */
export function grows(shares: unknown, open: string[]): Record<string, number> {
  const total = open.reduce((sum, key) => sum + shareOf(shares, key), 0)
  return Object.fromEntries(open.map((key) => [key, shareOf(shares, key) / total]))
}

/**
 * `shares` with the edge between `above` and `below` moved `by` px down, a share being `px` px:
 * the two trade what one gains, the rest stay, and neither goes under `min` px.
 */
export function resize(shares: unknown, above: string, below: string, by: number, px: number, min: number): Shares {
  const base: Shares = shares && typeof shares === "object" ? (shares as Shares) : {}
  const a = shareOf(base, above)
  const b = shareOf(base, below)
  const both = (a + b) * px
  if (!(px > 0) || both < min * 2) return base
  const to = Math.min(both - min, Math.max(min, a * px + by)) / px
  const round = (n: number) => Math.round(n * 1000) / 1000
  return { ...base, [above]: round(to), [below]: round(a + b - to) }
}

/** What the side bar's drags carry: a section, or a group's button. */
export type Carried = { kind: "pumi.section"; point: string; section: string } | { kind: "pumi.group"; point: string; group: string }

export function carried(data: unknown, point: string): Carried | undefined {
  const it = data as Carried | undefined
  if (!it || typeof it !== "object" || it.point !== point) return undefined
  return it.kind === "pumi.section" || it.kind === "pumi.group" ? it : undefined
}
