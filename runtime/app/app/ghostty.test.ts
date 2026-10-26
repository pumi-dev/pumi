// config.ghostty: Ghostty's lines read as pumi's options, and Settings' changes written back
// without touching the lines pumi doesn't know.
import { describe, expect, test } from "bun:test"
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { apply, importFromGhostty, migrate, parse, setKey, toOptions, writeOption } from "./ghostty.ts"

const options = (text: string) => toOptions(parse(text)).options

describe("reading a Ghostty config", () => {
  test("the keys pumi knows become its options; the rest are listed, unused", () => {
    const { options: got, unused } = toOptions(
      parse(`
# a comment
font-family = "JetBrains Mono"
font-family = Symbols Nerd Font
font-size = 13
font-thicken = true
font-feature = -calt
adjust-cell-height = 10%
adjust-cell-width = 5%
cursor-style = bar
cursor-style-blink = false
theme = light:Builtin Light,dark:Dracula
palette = 1=#ff0000
palette = 200=#00ff00
background-opacity = 0.9
macos-option-as-alt = left
copy-on-select = clipboard
window-padding-x = 8,12
window-padding-color = extend-always
keybind = cmd+t=new_tab
`),
    )
    expect(got).toMatchObject({
      guifont: "JetBrains_Mono:h13",
      terminal_font_fallbacks: "Symbols Nerd Font",
      terminal_font_smoothing: true,
      terminal_ligatures: false,
      linespace: 10,
      terminal_cursor_style: "bar",
      terminal_cursor_blink: "off",
      terminal_colorscheme: "Dracula",
      terminal_color_1: "#ff0000",
      ui_background_opacity: 90,
      macos_option_as_alt: true,
      terminal_copy_on_select: true,
      terminal_padding_x: 8,
      terminal_padding_color: "extend",
    })
    expect(got).not.toHaveProperty("terminal_color_200")
    expect(got).not.toHaveProperty("terminal_cell_width", "a key pumi doesn't use")
    expect(unused.sort()).toEqual(["adjust-cell-width", "keybind"])
  })

  test("a later line wins, and an empty font-family starts the list again", () => {
    expect(options("font-size = 12\nfont-size = 15")).toMatchObject({ guifont: ":h15" })
    expect(options("font-family = A\nfont-family =\nfont-family = B")).toEqual({ guifont: "B" })
  })

  test("values out of range are kept in it", () => {
    expect(options("minimum-contrast = 40\nadjust-cell-height = 999%")).toMatchObject({ terminal_minimum_contrast: 21, linespace: 100 })
  })
})

describe("writing it back", () => {
  test("a key's line is replaced where it was, comments and unknown keys kept", () => {
    const text = "# mine\ncursor-style = bar\nkeybind = x=y\n"
    expect(writeOption(text, "terminal_cursor_style", "underline", {})).toBe("# mine\ncursor-style = underline\nkeybind = x=y\n")
    expect(writeOption(text, "terminal_cursor_style", null, {})).toBe("# mine\nkeybind = x=y\n")
  })

  test("a new key goes at the end", () => {
    expect(setKey("a = 1", "b", ["2"])).toBe("a = 1\nb = 2\n")
  })

  test("the font is font-family lines (its fallbacks after it) and font-size", () => {
    const text = writeOption("font-family = Old\nfont-size = 11\n", "guifont", "Fira_Code:h14", { terminal_font_fallbacks: "Symbols Nerd Font" })
    expect(text).toBe('font-family = "Fira Code"\nfont-family = "Symbols Nerd Font"\nfont-size = 14\n')
    expect(options(text)).toMatchObject({ guifont: "Fira_Code:h14", terminal_font_fallbacks: "Symbols Nerd Font" })
  })

  test("ligatures off are font features, and on takes them out again", () => {
    const off = writeOption("font-feature = ss01\n", "terminal_ligatures", false, {})
    expect(parse(off).map((e) => e.value)).toEqual(["ss01", "-calt", "-liga", "-dlig"])
    expect(writeOption(off, "terminal_ligatures", true, {})).toBe("font-feature = ss01\n")
  })

  test("every option written reads back as it was", () => {
    const values: Record<string, string | number | boolean> = {
      terminal_font_style: "semibold",
      linespace: 15,
      terminal_minimum_contrast: 3,
      terminal_cursor_blink: "on",
      ui_background_opacity: 85,
      terminal_color_4: "#123456",
      terminal_background: "#101010",
    }
    for (const [name, value] of Object.entries(values)) expect(options(writeOption("", name, value, {}))[name]).toEqual(value)
  })
})

describe("the file", () => {
  const dir = () => mkdtempSync(join(tmpdir(), "pumi-ghostty-"))

  test("applying sets what differs, and a line taken out sets its option back", async () => {
    const file = join(dir(), "config.ghostty")
    writeFileSync(file, "cursor-style = bar\nfont-thicken = true\n")
    const sets: [string, unknown][] = []
    const set = async (name: string, value: unknown) => void sets.push([name, value])
    await apply(set, { terminal_font_smoothing: true }, file)
    expect(sets).toEqual([["terminal_cursor_style", "bar"]])
    writeFileSync(file, "font-thicken = true\n")
    sets.length = 0
    await apply(set, { terminal_cursor_style: "bar", terminal_font_smoothing: true }, file)
    expect(sets).toEqual([["terminal_cursor_style", null]])
  })

  test("import takes Ghostty's files, includes inlined, and keeps what was there", () => {
    const d = dir()
    const ghostty = join(d, "config")
    writeFileSync(join(d, "colors"), "theme = Nord\n")
    writeFileSync(ghostty, "font-size = 16\nconfig-file = colors\nconfig-file = ?missing\n")
    const file = join(d, "config.ghostty")
    writeFileSync(file, "cursor-style = bar\n")
    const result = importFromGhostty(file, [ghostty])
    expect(result.used).toBe(2)
    expect(options(readFileSync(file, "utf8"))).toEqual({ guifont: ":h16", terminal_colorscheme: "Nord" })
    expect(readFileSync(`${file}.bak`, "utf8")).toBe("cursor-style = bar\n")
  })

  test("settings.json's terminal options move into the file, an old linespace made a percent", () => {
    const d = dir()
    const settings = join(d, "settings.json")
    const file = join(d, "config.ghostty")
    writeFileSync(settings, JSON.stringify({ options: { linespace: 1.55, guifont: "Menlo:h13", ui_theme: "x" } }))
    expect(migrate(settings, {}, file).sort()).toEqual(["guifont", "linespace"])
    expect(JSON.parse(readFileSync(settings, "utf8")).options).toEqual({ ui_theme: "x" })
    expect(options(readFileSync(file, "utf8"))).toEqual({ guifont: "Menlo:h13", linespace: 30 })
  })
})
