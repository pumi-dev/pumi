import { describe, expect, test } from "bun:test"
import { appearanceOf, schemeLook, terminalScheme } from "./index.ts"

describe("schemeLook", () => {
  test("a dark scheme gives a dark look on its own background and foreground", () => {
    const scheme = terminalScheme("Dracula")!
    const { appearance, tokens } = schemeLook(scheme)
    expect(appearance).toBe("dark")
    expect(tokens.editor).toBe(scheme.background)
    expect(tokens.text).toBe(scheme.foreground)
    expect(tokens.accent).toBe(scheme.ansi[4])
  })

  test("a light scheme gives a light look", () => {
    expect(schemeLook(terminalScheme("GitHub Light Default")!).appearance).toBe("light")
  })

  test("the surfaces step from the background toward the foreground", () => {
    const { tokens } = schemeLook({ background: "#000000", foreground: "#ffffff", cursor: "#ffffff", selection: "#888888", ansi: Array(16).fill("#0000ff") })
    expect(tokens.window).toBe("#0d0d0d")
    expect(tokens.border).toBe("#1f1f1f")
    expect(tokens.accentText).toBe("#ffffff")
  })
})

describe("appearanceOf", () => {
  test("a background is dark or light by its brightness, whatever it is called", () => {
    expect(appearanceOf("#1e1f22")).toBe("dark")
    expect(appearanceOf("#ffffff")).toBe("light")
    expect(appearanceOf("#e1e2e7")).toBe("light")
  })
})
