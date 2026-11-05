import { describe, expect, test } from "bun:test"
import { TOKENS } from "../../../sdk/theme.ts"
import { DEFAULT_SCHEME, schemeLook, terminalScheme } from "./index.ts"

describe("schemeLook", () => {
  test("a scheme gives a look on its own background and foreground", () => {
    const scheme = terminalScheme("Dracula")!
    const tokens = schemeLook(scheme)
    expect(tokens.editor).toBe(scheme.background)
    expect(tokens.text).toBe(scheme.foreground)
    expect(tokens.accent).toBe(scheme.ansi[4])
  })

  test("a light scheme gives a light look", () => {
    const tokens = schemeLook(terminalScheme("GitHub Light Default")!)
    expect(tokens.editor).toBe("#ffffff")
    expect(tokens.text).toBe("#1f2328")
  })

  test("the surfaces step from the background toward the foreground", () => {
    const tokens = schemeLook({ background: "#000000", foreground: "#ffffff", cursor: "#ffffff", selection: "#888888", ansi: Array(16).fill("#0000ff") })
    expect(tokens.window).toBe("#0d0d0d")
    expect(tokens.border).toBe("#1f1f1f")
    expect(tokens.accentText).toBe("#ffffff")
  })

  test("the default scheme is a bundled one", () => {
    expect(terminalScheme(DEFAULT_SCHEME)).toBeDefined()
  })

  test("the default theme's colours are GitHub Dark Default's", () => {
    expect({ ...TOKENS, ...schemeLook(terminalScheme(DEFAULT_SCHEME)!) }).toEqual(TOKENS)
    expect(Object.keys(schemeLook(terminalScheme(DEFAULT_SCHEME)!)).sort()).toEqual(Object.keys(TOKENS).sort())
  })
})
