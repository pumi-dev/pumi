// `pumi://plugins/install/OWNER/REPO[/DIR…]`: a link that asks to install the plugin in a GitHub
// repository, or in a folder of one (a repository may hold several). pumi.dev's plugin pages have
// one, from the listing's `install`. Only GitHub: a link never names a git URL or a directory
// here. `opened.ts` asks before it installs; this part is apart from the engine, so it is tested
// without the addon.

/** What an install link names. */
export interface InstallLink {
  owner: string
  repo: string
  /** The plugin's folder in the repository. */
  dir?: string
  /** The repository's URL, as `plugin.add` records it. */
  url: string
  /** What `plugin.add` is given: the URL alone, so the store's ids never stand in for it. */
  source: string
  /** The name it is installed under: its folder's, else the repository's. */
  name: string
}

/** A store listing, as much of one as the question uses. */
export interface Listing {
  id?: string
  name?: string
  owner?: string
  description?: string
  src?: string
  path?: string | null
}

const OWNER = /^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?$/
const PART = /^[A-Za-z0-9._-]+$/

/** The plugin `link` names, or none when it is not an install link or names something else. */
export function parseInstallLink(link: string): InstallLink | undefined {
  const m = /^pumi:\/\/plugins\/install\/([^?#]*)(?:[?#].*)?$/i.exec(link.trim())
  if (!m) return undefined
  const parts = m[1]!.replace(/\/+$/, "").split("/")
  if (parts.length < 2) return undefined
  const [owner, rawRepo, ...rest] = parts as [string, string, ...string[]]
  const repo = rawRepo.replace(/\.git$/, "")
  if (!OWNER.test(owner) || !PART.test(repo)) return undefined
  if (rest.some((p) => !PART.test(p) || p === "." || p === "..")) return undefined
  if (repo === "." || repo === "..") return undefined
  const dir = rest.length ? rest.join("/") : undefined
  const url = `https://github.com/${owner}/${repo}`
  // A folder goes as GitHub's shorthand (`plugin.add` has no other way to name one); with a
  // folder it never matches a store id, which has one `/`.
  const source = dir ? `${owner}/${repo}/${dir}` : url
  return { owner, repo, dir, url, source, name: rest.at(-1) ?? repo }
}

/** A repository URL as two are compared: GitHub's names are not case-sensitive. */
const same = (a: string | undefined, b: string) => (a ?? "").replace(/\.git$/, "").replace(/\/+$/, "").toLowerCase() === b.toLowerCase()

/** The store's listing of what `link` names, when it has one. */
export function listingOf(link: InstallLink, listings: Listing[]): Listing | undefined {
  return listings.find((l) => same(l.src, link.url) && (l.path ?? "") === (link.dir ?? ""))
}

/** Whether what `link` names is installed (`registry.list("plugin")`'s entries). */
export function installedOf(link: InstallLink, installed: any[], listing?: Listing): boolean {
  return installed.some(
    (p) => (listing?.id && p?.id === listing.id) || (p?.spec?.name === link.name && same(p?.spec?.src, link.url)),
  )
}

/** Where it is, as the question says it: `github.com/owner/repo/dir`. */
export const whereOf = (link: InstallLink) => `github.com/${link.owner}/${link.repo}${link.dir ? `/${link.dir}` : ""}`

/** The question asked before it installs: a listed one by its listing, else by its repository. */
export function questionOf(link: InstallLink, listing?: Listing) {
  const trust = "Plugins are not sandboxed: one can do anything you can on this computer."
  if (listing) {
    const by = listing.owner ? ` by ${listing.owner}` : ""
    return {
      title: `Install ${listing.name ?? link.name}${by}?`,
      detail: [listing.description, `From ${whereOf(link)}.`, trust].filter(Boolean).join(" "),
      confirm: "Install",
    }
  }
  return {
    title: `Install ${link.name} from GitHub?`,
    detail: `${whereOf(link)} is not listed in the pumi store. ${trust} Install it only if you trust it.`,
    confirm: "Install",
    danger: true,
  }
}
