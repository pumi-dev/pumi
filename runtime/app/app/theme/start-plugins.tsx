// The start screen's first-run choices, as cards with a picture of each, four to a row: the plugins
// to install (a launcher's list, a shell, an agent's session, a page), how agents open, whether the
// window is transparent, and the colour scheme. Plugins have a box to tick, any number of them, and
// those unticked are dimmed; the other choices are one of each, with a round button. The pictures
// are drawn in the theme's colours (and the terminals' palette), so they look like what they choose.
import { memo, useState, type ReactNode } from "react"
import { FONT_DEFAULT, parseFont } from "../../sdk/font.ts"
import { Icon } from "../../sdk/elements.tsx"
import { useMetrics, usePalette, useTheme, withOpacity, type TerminalPalette } from "../../sdk/theme.ts"
import { setOption } from "../settings/store.ts"
import { openSettings } from "../settings/open.ts"
import { SCHEMES } from "../settings/pages.ts"
import { useOption } from "../state.ts"
import type { AgentsMode, Starter } from "../starters.ts"
import { schemeNames, terminalScheme } from "./terminal/index.ts"
import { SchemePreview } from "./terminal/preview.tsx"

/** Cards to a row, the gap between them, and their pictures' height. */
const COLUMNS = 4
const GAP = 12
const PREVIEW_HEIGHT = 84
/** The start screen's width while it shows them: four cards to a row. */
export const SETUP_WIDTH = 800

/** A size of text at the theme's scale: `n` at the default 13 pt. */
function useTextSize(): (n: number) => number {
  const base = useMetrics().fontSize
  return (n) => Math.round((n * base) / 13)
}

/** `cards` in a row, each a `COLUMNS`th of it however many there are. */
function CardRow({ children }: { children: ReactNode[] }) {
  const empty = Math.max(0, COLUMNS - children.length)
  return (
    <div style={{ display: "flex", flexDirection: "row", gap: GAP }}>
      {children}
      {Array.from({ length: empty }, (_, i) => (
        <div key={`empty-${i}`} style={{ flexGrow: 1, flexBasis: 0 }} />
      ))}
    </div>
  )
}

interface CardProps {
  title: string
  detail?: string
  preview: ReactNode
  on: boolean
  /** `check`: one of any number, ticked or not (dimmed). `radio`: one of a few. */
  kind: "check" | "radio"
  /** Taking no presses (an install running). */
  locked?: boolean
  onPress(): void
  testId?: string
}

const Card = memo(function Card({ title, detail, preview, on, kind, locked, onPress, testId }: CardProps) {
  const t = useTheme()
  const m = useMetrics()
  const ts = useTextSize()
  const press = () => locked || onPress()
  return (
    <div
      testId={testId}
      role={kind === "check" ? "checkbox" : "radio"}
      aria-label={title}
      aria-checked={on}
      title={detail ? `${title}: ${detail}` : title}
      tabIndex={0}
      onClick={press}
      onKeyDown={(e: any) => {
        if (e.key !== "enter" && e.key !== "space" && e.key !== " ") return
        e.preventDefault?.()
        press()
      }}
      style={{
        display: "flex",
        flexDirection: "column",
        flexGrow: 1,
        flexBasis: 0,
        minWidth: 0,
        borderRadius: m.radius + 2,
        borderWidth: on ? 2 : 1,
        // The unpicked one's border is 1 wide, its card as large as a picked one's.
        margin: on ? 0 : 1,
        borderColor: on ? t.accent : t.border,
        overflow: "hidden",
        cursor: locked ? "default" : "pointer",
        keyboardFocusDim: false,
        hover: on ? {} : { borderColor: t.muted },
      }}
    >
      <div style={{ pointerEvents: "none", display: "flex", width: "100%", height: PREVIEW_HEIGHT, backgroundColor: t.editor, overflow: "hidden", opacity: on || kind === "radio" ? 1 : 0.45 }}>
        {preview}
      </div>
      <div style={{ pointerEvents: "none", display: "flex", flexDirection: "row", alignItems: "center", gap: 8, paddingLeft: 10, paddingRight: 10, paddingTop: 7, paddingBottom: 8, borderTopWidth: 1, borderColor: t.border, backgroundColor: t.surface }}>
        <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0, gap: 1 }}>
          <text style={{ fontSize: ts(13), color: on ? t.text : t.muted, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{title}</text>
          {detail ? <text style={{ fontSize: ts(11), color: t.faint, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{detail}</text> : null}
        </div>
        {kind === "check" ? (
          <div style={{ display: "flex", width: 16, height: 16, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: 4, borderWidth: 1, borderColor: on ? t.accent : t.faint, backgroundColor: on ? t.accent : "transparent" }}>
            {on ? <Icon name="check" size={11} color={t.accentText} /> : null}
          </div>
        ) : (
          <div style={{ display: "flex", width: 16, height: 16, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: 8, borderWidth: 1, borderColor: on ? t.accent : t.faint, backgroundColor: on ? t.accent : "transparent" }}>
            {on ? <div style={{ width: 6, height: 6, borderRadius: 3, backgroundColor: t.accentText }} /> : null}
          </div>
        )}
      </div>
    </div>
  )
})

/** `offered` in a row, each ticked as `picked` says. `busy`: the one being installed, when one is. */
export function PluginCards({ offered, picked, busy, onToggle }: { offered: Starter[]; picked: Set<string>; busy?: string; onToggle(name: string): void }) {
  return (
    <CardRow>
      {offered.map((s) => (
        <Card
          key={s.name}
          testId={`start-plugin-${s.name}`}
          kind="check"
          title={busy === s.name ? "Installing…" : s.title}
          detail={s.detail}
          preview={<Preview name={s.name} />}
          on={picked.has(s.name)}
          locked={!!busy}
          onPress={() => onToggle(s.name)}
        />
      ))}
    </CardRow>
  )
}

/** One of a few, as radio cards in a row. */
function Choice<T extends string>({ cards, value, onChange, testId }: { cards: { value: T; title: string; detail?: string; preview: ReactNode }[]; value: T; onChange(value: T): void; testId: string }) {
  return (
    <CardRow>
      {cards.map((c) => (
        <Card key={c.value} testId={`${testId}-${c.value || "default"}`} kind="radio" title={c.title} detail={c.detail} preview={c.preview} on={c.value === value} onPress={() => onChange(c.value)} />
      ))}
    </CardRow>
  )
}

function Preview({ name }: { name: string }) {
  if (name === "launcher") return <LauncherPreview />
  if (name === "terminal") return <TerminalPreview />
  if (name === "agents") return <AgentsPreview />
  if (name === "browser") return <BrowserPreview />
  return <div style={{ flexGrow: 1 }} />
}

/** A bar `width` long (a share of its row, or pixels) standing for a line of text. */
function Bar({ width, color, height = 4 }: { width: number | string; color: string; height?: number }) {
  return <div style={{ width: width as any, height, borderRadius: height / 2, backgroundColor: color, flexShrink: 0 }} />
}

/** The terminals' colours, else the theme's. */
function useColors() {
  const t = useTheme()
  const p = usePalette()
  const ansi = p?.ansi ?? []
  return {
    background: p?.background ?? t.editor,
    foreground: p?.foreground ?? t.text,
    red: ansi[1] ?? t.error,
    green: ansi[2] ?? t.success,
    yellow: ansi[3] ?? t.warning,
    blue: ansi[4] ?? t.accent,
    magenta: ansi[5] ?? t.accent,
    cyan: ansi[6] ?? t.accent,
    cursor: p?.cursor ?? t.text,
  }
}

/** A line of a terminal: runs of text, each in its colour. */
function Line({ runs, font }: { runs: [string, string, string?][]; font: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "row", overflow: "hidden" }}>
      {runs.map(([text, color, background], i) => (
        <text key={i} style={{ fontFamily: font, fontSize: 10, lineHeight: 14, color, whiteSpace: "nowrap", ...(background ? { backgroundColor: background } : {}) }}>
          {text}
        </text>
      ))}
    </div>
  )
}

/** The Launcher: a search over a list of what to open, the first one picked. */
function LauncherPreview() {
  const t = useTheme()
  const c = useColors()
  const row = (color: string, width: string, picked?: boolean) => (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 13, paddingLeft: 5, paddingRight: 5, borderRadius: 3, ...(picked ? { backgroundColor: t.selected } : {}) }}>
      <div style={{ width: 7, height: 7, borderRadius: 2, backgroundColor: color, flexShrink: 0 }} />
      <Bar width={width} color={picked ? t.text : t.faint} height={3} />
    </div>
  )
  return (
    <div style={{ display: "flex", flexGrow: 1, alignItems: "center", justifyContent: "center", paddingLeft: 14, paddingRight: 14 }}>
      <div style={{ display: "flex", flexDirection: "column", width: "100%", gap: 2, padding: 5, borderRadius: 6, borderWidth: 1, borderColor: t.border, backgroundColor: t.surface }}>
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 5, height: 14, paddingLeft: 4, marginBottom: 2, borderRadius: 3, backgroundColor: t.editor }}>
          <Icon name="magnifying_glass" size={8} color={t.faint} />
          <Bar width="30%" color={t.muted} height={3} />
        </div>
        {row(c.green, "55%", true)}
        {row(c.magenta, "40%")}
        {row(c.blue, "65%")}
      </div>
    </div>
  )
}

/** The Terminal: a prompt, a command, what it printed and the cursor. */
function TerminalPreview() {
  const c = useColors()
  const font = FONT_DEFAULT.family
  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, paddingLeft: 10, paddingRight: 10, paddingTop: 9, backgroundColor: c.background }}>
      <Line font={font} runs={[["❯ ", c.green], ["~/pumi", c.blue], [" on ", c.foreground], ["main", c.magenta]]} />
      <Line font={font} runs={[["$ ", c.foreground], ["cargo", c.yellow], [" test", c.foreground]]} />
      <Line font={font} runs={[["ok", c.green], [" 42 passed", c.foreground]]} />
      <Line font={font} runs={[["$ ", c.foreground], [" ", c.foreground, c.cursor]]} />
    </div>
  )
}

/** Agents: what was asked, the agent working at it, and what it is changing. */
function AgentsPreview() {
  const t = useTheme()
  const c = useColors()
  const font = FONT_DEFAULT.family
  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, gap: 5, paddingLeft: 10, paddingRight: 10, paddingTop: 9, backgroundColor: c.background }}>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 5, paddingLeft: 5, height: 15, borderRadius: 3, borderLeftWidth: 2, borderColor: t.accent, backgroundColor: withOpacity(t.accent, 0.15) }}>
        <Line font={font} runs={[["> fix the tests", c.foreground]]} />
      </div>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 5 }}>
        <Icon name="sparkle" size={10} color={c.yellow} />
        <Line font={font} runs={[["Editing ", c.foreground], ["src/app.ts", c.cyan]]} />
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 3, paddingLeft: 15 }}>
        <Bar width="70%" color={withOpacity(c.red, 0.7)} height={3} />
        <Bar width="55%" color={withOpacity(c.green, 0.7)} height={3} />
      </div>
    </div>
  )
}

/** The Browser: an address bar over a page's heading, picture and text. */
function BrowserPreview() {
  const t = useTheme()
  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1 }}>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 18, paddingLeft: 8, paddingRight: 8, borderBottomWidth: 1, borderColor: t.border, backgroundColor: t.surface }}>
        <Icon name="arrow_left" size={8} color={t.faint} />
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, flexGrow: 1, height: 11, paddingLeft: 5, borderRadius: 5, backgroundColor: t.editor }}>
          <Icon name="lock" size={6} color={t.faint} />
          <Bar width="45%" color={t.faint} height={3} />
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "row", flexGrow: 1, gap: 8, paddingLeft: 10, paddingRight: 10, paddingTop: 9 }}>
        <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, gap: 4 }}>
          <Bar width="80%" color={t.text} height={5} />
          <Bar width="95%" color={t.faint} height={3} />
          <Bar width="70%" color={t.faint} height={3} />
          <Bar width="85%" color={t.faint} height={3} />
        </div>
        <div style={{ width: 38, height: 38, borderRadius: 4, flexShrink: 0, background: { type: "linear-gradient", angle: 135, stops: [{ color: t.accent, position: 0 }, { color: withOpacity(t.accent, 0.35), position: 1 }] } as any }} />
      </div>
    </div>
  )
}

// ── One of each ─────────────────────────────────────────────────────

/** How the Agents plugin opens agents: a chat tab over them, or their own terminal. */
export function AgentsChoice({ value, onChange }: { value: AgentsMode; onChange(mode: AgentsMode): void }) {
  return (
    <Choice
      testId="start-agents"
      value={value}
      onChange={onChange}
      cards={[
        { value: "sdk", title: "Chat UI", detail: "messages, cards, a prompt", preview: <AgentChatPreview /> },
        { value: "terminal", title: "Terminal", detail: "the agent's own screen", preview: <AgentTerminalPreview /> },
      ]}
    />
  )
}

/** `ui_background_opacity`, set as it is picked: under 100 (its default, 95) or 100. */
export function WindowChoice() {
  const opacity = Number(useOption("ui_background_opacity") ?? 95)
  // Picked at once, before the server says it is set.
  const [picked, setPicked] = useState<string>()
  const value = picked ?? (opacity >= 100 ? "opaque" : "transparent")
  const pick = (v: string) => {
    setPicked(v)
    setOption("ui_background_opacity", v === "opaque" ? 100 : null).catch((err) => console.error("pumi: setting ui_background_opacity:", err?.message ?? err))
  }
  return (
    <Choice
      testId="start-window"
      value={value}
      onChange={pick}
      cards={[
        { value: "transparent", title: "Transparent", detail: "the desktop shows through", preview: <WindowPreview transparent /> },
        { value: "opaque", title: "Opaque", detail: "a solid background", preview: <WindowPreview /> },
      ]}
    />
  )
}

/** The schemes the start screen offers after the theme's own: the best known. */
export const START_SCHEMES = ["Catppuccin Mocha", "Dracula", "TokyoNight"]

/** `terminal_colorscheme`, set as it is picked: the theme's own (the default), or one of `START_SCHEMES`; the rest are a page of Settings away. */
export function SchemeChoice() {
  const t = useTheme()
  const ts = useTextSize()
  const shown = String(useOption("terminal_colorscheme") ?? "")
  const font = parseFont(useOption("guifont"), useOption("linespace")).family
  const palette = usePalette()
  const [picked, setPicked] = useState<string>()
  const value = picked ?? shown
  // The theme's own palette is the one drawn while no scheme is: kept from then for its card.
  const [own, setOwn] = useState<TerminalPalette | undefined>(() => (shown ? undefined : palette))
  if (!shown && palette && palette !== own) setOwn(palette)
  const pick = (v: string) => {
    setPicked(v)
    setOption("terminal_colorscheme", v || null).catch((err) => console.error("pumi: setting terminal_colorscheme:", err?.message ?? err))
  }
  const cards = [
    {
      value: "",
      title: "Match the theme",
      preview: own ? (
        <SchemePreview s={own} font={font} />
      ) : (
        <div style={{ display: "flex", flexGrow: 1, alignItems: "center", justifyContent: "center" }}>
          <text style={{ fontSize: ts(12), color: t.faint }}>The theme's own colors</text>
        </div>
      ),
    },
    ...START_SCHEMES.filter((n) => terminalScheme(n)).map((name) => ({ value: name, title: name, preview: <SchemePreview s={terminalScheme(name)!} font={font} /> })),
  ]
  const all = () => openSettings(SCHEMES.id)
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <Choice testId="start-scheme" value={value} onChange={pick} cards={cards} />
      <div
        role="button"
        aria-label="Show all color schemes"
        tabIndex={0}
        onClick={all}
        onKeyDown={(e: any) => {
          if (e.key !== "enter" && e.key !== "space" && e.key !== " ") return
          e.preventDefault?.()
          all()
        }}
        style={{ display: "flex", alignSelf: "flex-start", paddingLeft: 10, cursor: "pointer", color: t.faint, hover: { color: t.text } }}
      >
        <text style={{ fontSize: ts(12) }}>{`Show all ${schemeNames().length} in Settings`}</text>
      </div>
    </div>
  )
}

/** A desktop's colours, for what shows through a transparent window. */
const DESKTOP = [
  { color: "#5b6cf0", position: 0 },
  { color: "#c35bd6", position: 0.55 },
  { color: "#f0915b", position: 1 },
]

/** A window over the desktop: `transparent`, the desktop shows through its side bar and pane. */
function WindowPreview({ transparent }: { transparent?: boolean }) {
  const t = useTheme()
  const glass = (color: string) => (transparent ? withOpacity(color, 0.55) : color)
  return (
    <div style={{ display: "flex", flexGrow: 1, alignSelf: "stretch", padding: 10, background: { type: "linear-gradient", angle: 135, stops: DESKTOP } as any }}>
      <div style={{ display: "flex", flexDirection: "row", flexGrow: 1, borderRadius: 5, overflow: "hidden", borderWidth: 1, borderColor: withOpacity(t.border, 0.8) }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 4, width: 44, paddingLeft: 6, paddingTop: 6, backgroundColor: glass(t.window) }}>
          <div style={{ display: "flex", flexDirection: "row", gap: 2, marginBottom: 2 }}>
            {["#ff5f57", "#febc2e", "#28c840"].map((c) => (
              <div key={c} style={{ width: 4, height: 4, borderRadius: 2, backgroundColor: c }} />
            ))}
          </div>
          <Bar width="70%" color={t.muted} height={3} />
          <Bar width="55%" color={t.faint} height={3} />
          <Bar width="62%" color={t.faint} height={3} />
        </div>
        <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, gap: 4, paddingLeft: 7, paddingTop: 8, borderLeftWidth: 1, borderColor: withOpacity(t.border, 0.6), backgroundColor: glass(t.editor) }}>
          <Bar width="50%" color={t.success} height={3} />
          <Bar width="75%" color={t.muted} height={3} />
          <Bar width="40%" color={t.muted} height={3} />
        </div>
      </div>
    </div>
  )
}

/** An agent in its own terminal: its banner, a prompt box, and what it does. */
function AgentTerminalPreview() {
  const t = useTheme()
  const c = useColors()
  const font = FONT_DEFAULT.family
  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, alignSelf: "stretch", gap: 4, paddingLeft: 9, paddingRight: 9, paddingTop: 8, backgroundColor: c.background }}>
      <Line font={font} runs={[["✻ ", c.yellow], ["Editing ", c.foreground], ["app.ts", c.cyan]]} />
      <div style={{ display: "flex", flexDirection: "column", gap: 3, paddingLeft: 10 }}>
        <Bar width="60%" color={withOpacity(c.red, 0.7)} height={3} />
        <Bar width="45%" color={withOpacity(c.green, 0.7)} height={3} />
      </div>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", height: 16, marginTop: 2, paddingLeft: 5, borderRadius: 3, borderWidth: 1, borderColor: t.faint }}>
        <Line font={font} runs={[["> ", c.foreground], [" ", c.foreground, c.cursor]]} />
      </div>
    </div>
  )
}

/** An agent in a chat tab: what was asked, its answer and a call it made, and the prompt. */
function AgentChatPreview() {
  const t = useTheme()
  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, alignSelf: "stretch", gap: 5, paddingLeft: 9, paddingRight: 9, paddingTop: 8, backgroundColor: t.editor }}>
      <div style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end" }}>
        <div style={{ display: "flex", width: "55%", height: 12, borderRadius: 6, backgroundColor: t.accent }} />
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        <Bar width="80%" color={t.muted} height={3} />
        <Bar width="60%" color={t.muted} height={3} />
      </div>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, height: 11, width: "65%", paddingLeft: 4, borderRadius: 3, borderWidth: 1, borderColor: t.border, backgroundColor: t.surface }}>
        <Icon name="check" size={6} color={t.success} />
        <Bar width="60%" color={t.faint} height={3} />
      </div>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "flex-end", height: 14, paddingRight: 3, borderRadius: 7, borderWidth: 1, borderColor: t.border, backgroundColor: t.surface }}>
        <div style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: t.accent }} />
      </div>
    </div>
  )
}
