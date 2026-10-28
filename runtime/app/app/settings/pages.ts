// Settings' pages: the built-in ones and the option groups each draws, the plugins' (their own,
// or one made of their options), where a link goes, and what the search finds without drawing
// anything. Plain data, so a test can read it.
import { choicesOf, groupOf, titleOf, type Decl } from "../../sdk/settings/options.ts"

export const BUILT_IN = ["general", "appearance", "keys", "agents", "plugins"] as const
export type BuiltIn = (typeof BUILT_IN)[number]

export interface PageInfo {
  id: string
  title: string
  icon: string
  /** The plugin it is of, for a plugin's page. */
  plugin?: string
  /** A plugin's own page: the `ui.tsx` export that draws it. Without one, its options are its page. */
  component?: string
  keywords?: string[]
  /** A page reached from another and not listed in the nav: the nav marks the one it is from. */
  parent?: string
}

export const PAGES: Record<BuiltIn, PageInfo> = {
  general: { id: "general", title: "General", icon: "settings" },
  appearance: { id: "appearance", title: "Appearance", icon: "sparkle" },
  keys: { id: "keys", title: "Keybindings", icon: "keyboard" },
  agents: { id: "agents", title: "Agents", icon: "zap" },
  plugins: { id: "plugins", title: "Plugins", icon: "puzzle" },
}

/** The store: reached from Plugins, not listed in the nav. */
export const STORE: PageInfo = { id: "store", title: "Plugins and themes", icon: "puzzle", parent: "plugins" }

/** Every theme, and every colour scheme, on a page of their own: reached from Appearance. */
export const THEMES: PageInfo = { id: "themes", title: "Themes", icon: "sparkle", parent: "appearance" }
export const SCHEMES: PageInfo = { id: "color-schemes", title: "Color schemes", icon: "sparkle", parent: "appearance" }

/** The pages a button reaches, not the nav. */
export const HIDDEN: PageInfo[] = [STORE, THEMES, SCHEMES]

/** The group of pumi's own theme's settings, drawn under the theme picker (pumi-mirror's `DEFAULT_THEME_GROUP`). */
export const DEFAULT_THEME = "Default theme"

/** Whether `group` is pumi's own theme's: its own, or one of its sections (`Default theme · Panes`). */
export function isDefaultTheme(group: string): boolean {
  return group === DEFAULT_THEME || group.startsWith(`${DEFAULT_THEME} · `)
}

/** The option groups each built-in page draws itself; General draws every other built-in group after its own. */
export const GROUPS: Record<BuiltIn, string[]> = {
  general: ["General", "Updates"],
  appearance: ["Appearance", DEFAULT_THEME, "Terminal", "Font", "Cursor", "Mouse"],
  keys: ["Keys"],
  agents: [],
  plugins: ["Plugins"],
}

/** A plugin's section on a page. */
export interface SectionInfo {
  /** Its prefixed id, which a link names (`me.remote.tailscale`). */
  id: string
  plugin: string
  /** The page it is on: a built-in one, or the plugin's own page's prefixed id. */
  page: string
  title: string
  component: string
  keywords?: string[]
}

/** What a plugin's manifest says of Settings, as the app reads it. */
export interface PluginSettings {
  id: string
  /** `owner.name.`: its options, pages and sections start with it. */
  prefix: string
  /** What the nav calls a page made of its options: its display name (`Chat UI`), else its name alone (`npm-scripts`). */
  name: string
  pages?: { id: string; title: string; icon?: string; component: string; keywords?: string[] }[]
  sections?: { id: string; page: string; title: string; component: string; keywords?: string[] }[]
}

/** The agents' options' plugin id: Agents draws them, not a page of their own. */
export const AGENTS = "agents"

/** The plugins' pages (their own, and one made of the options of each that says nothing of Settings) and their sections. */
export function pluginPages(plugins: PluginSettings[], decls: Decl[], components: boolean): { pages: PageInfo[]; sections: SectionInfo[] } {
  const pages: PageInfo[] = []
  const sections: SectionInfo[] = []
  for (const p of plugins) {
    const given = !!(p.pages?.length || p.sections?.length)
    if (components) {
      for (const page of p.pages ?? []) pages.push({ id: `${p.prefix}${page.id}`, title: page.title, icon: page.icon ?? "plugin", plugin: p.id, component: page.component, keywords: page.keywords })
      for (const s of p.sections ?? []) {
        const own = (p.pages ?? []).some((page) => page.id === s.page)
        sections.push({ id: `${p.prefix}${s.id}`, plugin: p.id, page: own ? `${p.prefix}${s.page}` : s.page, title: s.title, component: s.component, keywords: s.keywords })
      }
    }
    if (!given && decls.some((d) => d.plugin === p.id)) pages.push({ id: p.id, title: p.name, icon: "puzzle", plugin: p.id })
  }
  // A plugin the server has options of but the app has no manifest for (another app's): its page too.
  for (const d of decls) {
    if (d.plugin && d.plugin !== AGENTS && !plugins.some((p) => p.id === d.plugin) && !pages.some((page) => page.id === d.plugin)) {
      pages.push({ id: d.plugin, title: d.plugin.split("/").pop() ?? d.plugin, icon: "puzzle", plugin: d.plugin })
    }
  }
  return { pages, sections }
}

/** The built-in groups General draws after its own: any no other page claims. */
export function otherGroups(decls: Decl[]): string[] {
  const claimed = new Set(Object.values(GROUPS).flat())
  const out: string[] = []
  for (const d of decls) {
    const g = groupOf(d)
    // An option without a label is the server's own, set from the command line.
    if (d.plugin || !d.label || claimed.has(g) || isDefaultTheme(g) || out.includes(g)) continue
    out.push(g)
  }
  return out
}

/** The page an option is drawn on. */
export function pageOfOption(d: Decl, pages: PageInfo[]): string {
  if (d.plugin === AGENTS) return "agents"
  if (d.plugin) {
    const own = pages.find((p) => p.id === d.plugin)
    if (own) return own.id
    // A plugin with its own pages: the first of them.
    return pages.find((p) => p.plugin === d.plugin)?.id ?? "plugins"
  }
  const group = groupOf(d)
  if (isDefaultTheme(group)) return "appearance"
  for (const page of BUILT_IN) if (GROUPS[page].includes(group)) return page
  return "general"
}

/** The rows the built-in pages draw that are no option, by the link that names each. */
export const ROWS: { id: string; page: BuiltIn; title: string; text: string }[] = [
  { id: "server", page: "general", title: "Pumi Terminals", text: "server running restart log pumid version uptime" },
  { id: "updates", page: "general", title: "Current version", text: "updates check for updates restart to update version" },
  { id: "ui_theme", page: "appearance", title: "Theme", text: "theme look ui dark light" },
  { id: "ghostty-config", page: "appearance", title: "config.ghostty", text: "ghostty config import edit file terminal settings config.ghostty" },
  { id: "agent-hooks", page: "agents", title: "Hooks", text: "agents hooks skill install update claude codex pi gemini cursor opencode" },
  { id: "safe-mode", page: "plugins", title: "Safe mode", text: "safe mode turn off plugins restart" },
  { id: "store", page: "plugins", title: "Community plugins", text: "community plugins themes browse install store" },
  { id: "plugin-updates", page: "plugins", title: "Current plugins", text: "plugin updates check update all" },
]

/** Where `link` (an option, a section, a page or a built-in row) is, when it names something there. */
export function pageOfLink(link: string, decls: Decl[], pages: PageInfo[], sections: SectionInfo[]): string | undefined {
  const d = decls.find((x) => x.name === link)
  if (d) return pageOfOption(d, pages)
  const section = sections.find((s) => s.id === link)
  if (section) return section.page
  if (pages.some((p) => p.id === link) || (BUILT_IN as readonly string[]).includes(link)) return link
  if (link.startsWith("command:")) return "keys"
  return ROWS.find((r) => r.id === link)?.page
}

/** One thing the search can find, and where it is. */
export interface Entry {
  kind: "option" | "row" | "section" | "page" | "command"
  /** The link that names it: an option's name, a section's id, a page's, `command:<name>`. */
  id: string
  page: string
  title: string
  /** Everything it is found by, lowercased. */
  text: string
}

/** A command, as Keybindings lists it. */
export interface CommandRow {
  name: string
  label: string
  group: string
  keys: string[]
}

/** What the search finds without drawing a component: the options, the built-in rows, the commands, and the plugins' pages and sections by their titles and keywords. */
export function searchIndex(decls: Decl[], pages: PageInfo[], sections: SectionInfo[], commands: CommandRow[] = []): Entry[] {
  const out: Entry[] = []
  const words = (...parts: (string | undefined | null)[]) => parts.filter(Boolean).join(" ").toLowerCase()
  for (const d of decls) {
    if (d.control === "keymap") continue
    out.push({ kind: "option", id: d.name, page: pageOfOption(d, pages), title: titleOf(d), text: words(titleOf(d), d.desc, d.name, ...choicesOf(d).map((c) => c.label)) })
  }
  for (const r of ROWS) out.push({ kind: "row", id: r.id, page: r.page, title: r.title, text: words(r.title, r.text) })
  for (const p of pages) if (p.component) out.push({ kind: "page", id: p.id, page: p.id, title: p.title, text: words(p.title, ...(p.keywords ?? [])) })
  for (const s of sections) out.push({ kind: "section", id: s.id, page: s.page, title: s.title, text: words(s.title, ...(s.keywords ?? [])) })
  for (const c of commands) out.push({ kind: "command", id: `command:${c.name}`, page: "keys", title: c.label, text: words(c.group, c.label, c.name, ...c.keys) })
  return out
}

/** The entries with every word of `query` in them, in the order they were indexed. */
export function search(entries: Entry[], query: string): Entry[] {
  const want = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (!want.length) return []
  return entries.filter((e) => want.every((w) => e.text.includes(w)))
}
