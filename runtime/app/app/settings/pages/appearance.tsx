// Settings → Appearance: the theme (the theme picker, the colour scheme everything takes its
// colours from, and the picked theme's own settings), then the terminals' font, spacing, cursor
// and mouse.
import { existsSync } from "node:fs"
import { homedir } from "node:os"
import { Fragment, useEffect, useMemo, useState } from "react"
import { guifont, parseFont, FONT_SIZE_RANGE } from "../../../sdk/font.ts"
import { Button, ImageCards, Note, OptionRow, Page, Row, Section, SelectField, Stepper, TextField, useSettings, useTextSize, type ImageCard } from "../../../sdk/settings/index.ts"
import { useTheme, type TerminalPalette } from "../../../sdk/theme.ts"
import { assets } from "../../assets.ts"
import { engine } from "../../engine.ts"
import { schemeNames, terminalScheme } from "../../theme/terminal/index.ts"
import { useThemes } from "../../themes.ts"
import { GHOSTTY_FILE, ensureFile, importFromGhostty, inspect } from "../../ghostty.ts"
import { DEFAULT_THEME, isDefaultTheme, SCHEMES, THEMES } from "../pages.ts"
import { GroupedOptions, GroupSection, openPath, useGroup } from "../shared.tsx"

const MATCH_THEME = "Match the theme"
/** The most cards a picker shows without a way to a page of them all. */
const SHOWN_INLINE = 12
/** A card's narrowest on a page of them all (the strip's size), and its preview's height at that width. */
const GRID_CARD = { width: 168, height: 80 }

/** A theme's `preview.svg` (or `.png`), when it has one. */
function previewOf(dir: string): string | undefined {
  return ["preview.svg", "preview.png"].map((f) => `${dir}/${f}`).find((f) => existsSync(f))
}

/** The cards whose title has `query` in it. */
function useFiltered(all: ImageCard[], query: string): ImageCard[] {
  return useMemo(() => {
    const q = query.trim().toLowerCase()
    return q ? all.filter((c) => c.title.toLowerCase().includes(q)) : all
  }, [all, query])
}

interface Picker {
  cards: ImageCard[]
  value: string
  onChange(value: string): void
}

/** The UI themes: pumi's own, then each installed one with its preview. */
function useThemePicker(): Picker {
  const settings = useSettings()!
  const themes = useThemes()
  const value = String(settings.value("ui_theme") ?? "").trim() || "default"
  const cards = useMemo<ImageCard[]>(() => [{ value: "default", title: "Default", src: assets.themePreview("default") }, ...themes.map((t) => ({ value: t.id, title: t.id, src: previewOf(t.dir) }))], [themes])
  return { cards, value, onChange: (v) => settings.set("ui_theme", v) }
}

/** The colour schemes: the theme's own, then every bundled one as a terminal draws it. */
function useSchemePicker(): Picker {
  const settings = useSettings()!
  const value = String(settings.value("terminal_colorscheme") ?? "")
  const font = parseFont(settings.value("guifont"), settings.value("linespace")).family
  // Every card once per font and scheme, so a keystroke in the search only filters them: built
  // anew, each of the hundreds of previews was a fresh element, and the strip drew them all again.
  const cards = useMemo<ImageCard[]>(() => {
    const names = schemeNames()
    const out: ImageCard[] = [{ value: "", title: MATCH_THEME, preview: <Blank caption="The theme's own colors" /> }]
    if (value && !names.includes(value)) out.push({ value, title: value, preview: <Blank caption="Not a bundled scheme" /> })
    for (const name of names) out.push({ value: name, title: name, preview: <SchemePreview s={terminalScheme(name)!} font={font} /> })
    return out
  }, [value, font])
  return { cards, value, onChange: (v) => settings.set("terminal_colorscheme", v || null) }
}

/** A picker's row: a search, a strip of its cards, and past a dozen of them a way to a page of them all. */
function PickerRow({ picker, link, title, noun, page, testId }: { picker: Picker; link: string; title: string; noun: string; page: string; testId: string }) {
  const settings = useSettings()!
  const [query, setQuery] = useState("")
  const cards = useFiltered(picker.cards, query)
  const many = picker.cards.length > SHOWN_INLINE
  return (
    <Row
      link={link}
      title={title}
      control={
        <>
          <TextField search value={query} placeholder={`Search ${noun}…`} onChange={setQuery} onCommit={setQuery} testId={`${testId}-search`} />
          {many ? (
            <Button onClick={() => settings.open(page)} title={`Every ${noun.replace(/s$/, "")} on one page`} testId={`${testId}-all`}>
              {`Show all ${picker.cards.length}`}
            </Button>
          ) : null}
        </>
      }
    >
      {cards.length ? <ImageCards strip testId={`${testId}-cards`} cards={cards} value={picker.value} onChange={picker.onChange} /> : <Note>{`No ${noun} match.`}</Note>}
    </Row>
  )
}

/** Every card of a picker on a page of its own: a search over a grid that fills the page. */
function AllCardsPage({ picker, title, noun, testId }: { picker: Picker; title: string; noun: string; testId: string }) {
  const settings = useSettings()!
  const t = useTheme()
  const ts = useTextSize()
  const [query, setQuery] = useState("")
  const cards = useFiltered(picker.cards, query)
  return (
    <Page title={title} fill>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 12 }}>
        <Button ghost icon="arrow_left" onClick={() => settings.open("appearance")} testId={`${testId}-back`}>
          Appearance
        </Button>
        <div style={{ flexGrow: 1 }} />
        <text style={{ fontSize: ts(12), color: t.faint }}>{query.trim() ? `${cards.length} of ${picker.cards.length}` : `${picker.cards.length} ${noun}`}</text>
      </div>
      <TextField search autoFocus value={query} width="100%" placeholder={`Search ${noun}…`} onChange={setQuery} onCommit={setQuery} testId={`${testId}-search`} />
      {cards.length ? <ImageCards grid testId={`${testId}-cards`} cards={cards} value={picker.value} onChange={picker.onChange} width={GRID_CARD.width} height={GRID_CARD.height} /> : <Note>{`No ${noun} match.`}</Note>}
    </Page>
  )
}

/** Settings → Appearance → Show all themes. */
export function ThemesPage() {
  return <AllCardsPage picker={useThemePicker()} title={THEMES.title} noun="themes" testId="ui-theme-all" />
}

/** Settings → Appearance → Show all color schemes. */
export function SchemesPage() {
  return <AllCardsPage picker={useSchemePicker()} title={SCHEMES.title} noun="schemes" testId="terminal-scheme-all" />
}

function ThemeSection() {
  const settings = useSettings()!
  const themes = useThemes()
  const theme = useThemePicker()
  const scheme = useSchemePicker()
  const picked = theme.value
  // The picked theme's own settings: the default's groups (its look under the picker, then how
  // its panes, side bar and motion behave), or what its plugin declares.
  const plugin = themes.find((t) => t.id === picked)?.plugin
  const defaults = useGroup(DEFAULT_THEME)
  const sections = settings.decls.filter((d) => !d.plugin && d.group !== DEFAULT_THEME && isDefaultTheme(d.group ?? ""))
  const own = plugin ? settings.decls.filter((d) => d.plugin === plugin) : []
  return (
    <>
      <Section id="ui_theme">
        <PickerRow picker={theme} link="ui_theme" title="Theme" noun="themes" page={THEMES.id} testId="ui-theme" />
        <PickerRow picker={scheme} link="terminal_colorscheme" title="Color scheme" noun="schemes" page={SCHEMES.id} testId="terminal-scheme" />
      </Section>
      {picked === "default" && defaults.length ? (
        <Section title="Look">
          {defaults.map((d) => (
            <OptionRow key={d.name} name={d.name} description="" />
          ))}
        </Section>
      ) : null}
      {picked === "default" && sections.length ? <GroupedOptions decls={sections} own={DEFAULT_THEME} description="" /> : null}
      {picked !== "default" && own.length ? <GroupedOptions decls={own} own={plugin} description="" /> : null}
    </>
  )
}

/** A scheme as a terminal draws it: a prompt, a command with the cursor after it, an error beside a selection, and its sixteen colours. */
function SchemePreview({ s, font }: { s: TerminalPalette; font: string }) {
  const [, red, green, yellow, blue, magenta, cyan] = s.ansi
  const run = (text: string, color: string, background?: string) => (
    <text style={{ fontFamily: font, fontSize: 11, lineHeight: 15, color, whiteSpace: "nowrap", ...(background ? { backgroundColor: background } : {}) }}>{text}</text>
  )
  const line = (...parts: ReturnType<typeof run>[]) => (
    <div style={{ display: "flex", flexDirection: "row", overflow: "hidden" }}>
      {parts.map((p, i) => (
        <Fragment key={i}>{p}</Fragment>
      ))}
    </div>
  )
  // The sixteen colours as one box with hard stops rather than sixteen boxes: a strip shows a
  // dozen previews, and every box is drawn again each frame it scrolls.
  const swatches = s.ansi.slice(0, 16).flatMap((color, i, all) => [
    { color, position: i / all.length },
    { color, position: (i + 1) / all.length },
  ])
  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", width: "100%", height: "100%", paddingLeft: 8, paddingRight: 8, paddingTop: 7, paddingBottom: 7, backgroundColor: s.background }}>
      <div style={{ display: "flex", flexDirection: "column" }}>
        {line(run("❯ ", green!), run("~/pumi", blue!), run(" on ", s.foreground), run("main", magenta!))}
        {line(run("$ ", s.foreground), run("ls", yellow!), run(" -a ", s.foreground), run("src/", cyan!), run(" ", s.foreground, s.cursor))}
        {line(run("error", red!), run(": ", s.foreground), run("selected", s.foreground, s.selection))}
      </div>
      <div style={{ height: 6, borderRadius: 2, background: { type: "linear-gradient", angle: 90, stops: swatches } }} />
    </div>
  )
}

function Blank({ caption }: { caption: string }) {
  const t = useTheme()
  const ts = useTextSize()
  return <text style={{ fontSize: ts(12), color: t.faint }}>{caption}</text>
}

let fonts: Promise<string[]> | undefined
let known: string[] | undefined
/** The monospace families, asked of the text system once and off the main thread: telling
 *  them loads every installed font, which would hold the page back from drawing. */
function monospaceFonts(): Promise<string[]> {
  return (fonts ??= Promise.resolve()
    .then(() => engine.fonts({ monospace: true }))
    .then((list) => (known = list))
    .catch((err) => {
      console.error("pumi: the fonts:", (err as Error)?.message ?? err)
      fonts = undefined
      return []
    }))
}

function FontSection() {
  const settings = useSettings()!
  const font = parseFont(settings.value("guifont"), settings.value("linespace"))
  // The page draws with the font in use, and the list comes in after.
  const [list, setList] = useState(() => known ?? [])
  useEffect(() => {
    let live = true
    if (!known) void monospaceFonts().then((l) => live && setList(l))
    return () => {
      live = false
    }
  }, [])
  const families = list.includes(font.family) ? list : [font.family, ...list]
  // Shape, then spacing, then what fills in: the order the rows read best in.
  const rest = orderBy(useGroup("Font", ["guifont"]), FONT_ORDER)
  return (
    <Section title="Font">
      <Row
        link="guifont"
        title="Font"
        control={<SelectField testId="font-family" width={240} options={families.map((f) => ({ value: f }))} value={font.family} onChange={(family) => settings.set("guifont", guifont(family, font.size))} />}
      />
      <Row
        title="Font size"
        control={<Stepper testId="font-size" value={font.size} min={FONT_SIZE_RANGE[0]} max={FONT_SIZE_RANGE[1]} format={(n) => `${n} pt`} onChange={(size) => settings.set("guifont", guifont(font.family, size))} />}
      />
      {rest.map((d) => (
        <OptionRow key={d.name} name={d.name} description="" />
      ))}
    </Section>
  )
}

const FONT_ORDER = ["terminal_font_style", "terminal_font_smoothing", "terminal_ligatures", "linespace", "terminal_font_fallbacks"]
function orderBy<T extends { name: string }>(list: T[], order: string[]): T[] {
  const at = (name: string) => (order.includes(name) ? order.indexOf(name) : order.length)
  return [...list].sort((a, b) => at(a.name) - at(b.name))
}

/** Opens config.ghostty in the text editor the system gives plain text (made first when it isn't there). */
function editGhostty() {
  const file = ensureFile()
  if (process.platform === "darwin") Bun.spawn(["open", "-t", file], { stdio: ["ignore", "ignore", "ignore"] })
  else openPath(file)
}

/** The terminals' settings file: where it is, importing Ghostty's, and editing it by hand. */
function GhosttyRow() {
  const settings = useSettings()!
  const [note, setNote] = useState<{ text: string; tone?: "error" }>()
  // Read again with the options, which change as the file does.
  const { unused } = useMemo(() => {
    try {
      return inspect()
    } catch {
      return { unused: [] as string[] }
    }
  }, [settings])
  const run = () => {
    try {
      const { from, used, unused } = importFromGhostty()
      const skipped = unused.length ? ` ${unused.length} more pumi doesn't use yet stay in the file.` : ""
      setNote({ text: `Imported ${used} settings from ${from.join(" and ")}.${skipped} What was there before is in config.ghostty.bak.` })
    } catch (err) {
      setNote({ text: (err as Error).message, tone: "error" })
    }
  }
  return (
    <Row
      link="ghostty-config"
      testId="ghostty-config"
      title="config.ghostty"
      aside={`${GHOSTTY_FILE.replace(homedir(), "~")}${unused.length ? ` · ${unused.length} keys it has that pumi doesn't use` : ""}`}
      control={
        <>
          <Button icon="download" onClick={run} testId="ghostty-import">
            Import from Ghostty
          </Button>
          <Button icon="arrow_up_right" onClick={editGhostty} testId="ghostty-edit">
            Edit file
          </Button>
        </>
      }
    >
      {note ? <Note tone={note.tone}>{note.text}</Note> : null}
    </Row>
  )
}

export function AppearancePage() {
  const rest = useGroup("Appearance", ["ui_theme", "terminal_colorscheme"])
  return (
    <Page title="Appearance">
      <ThemeSection />
      {rest.length ? (
        <Section>
          {rest.map((d) => (
            <OptionRow key={d.name} name={d.name} description="" />
          ))}
        </Section>
      ) : null}
      <FontSection />
      <GroupSection group="Terminal" description="" />
      <GroupSection group="Cursor" description="" />
      <GroupSection group="Mouse" description="" />
      <Section title="Ghostty">
        <GhosttyRow />
      </Section>
    </Page>
  )
}
