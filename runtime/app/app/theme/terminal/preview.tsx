// A terminal colour scheme drawn small, as a terminal shows it: what Settings → Appearance's cards
// and the start screen's show of each scheme.
import { Fragment } from "react"
import type { TerminalPalette } from "../../../sdk/theme.ts"

/** A scheme as a terminal draws it: a prompt, a command with the cursor after it, an error beside a selection, and its sixteen colours. */
export function SchemePreview({ s, font }: { s: TerminalPalette; font: string }) {
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
