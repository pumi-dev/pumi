// Settings → Agents' plain parts: where the command line is, the PATH agents are looked for in,
// and what `pumi skill list --json` says, as the page words it.
import { describe, expect, test } from "bun:test"
import { agentPath, cliPath, hookState, parseList, type SkillAgent } from "./skills.ts"

const agent = (more: Partial<SkillAgent> = {}): SkillAgent => ({ id: "claude", label: "Claude Code", available: true, hook: "installed", reach: "prompt", accepts: ["queue"], resumes: true, ...more })

describe("skills", () => {
  test("the command line: $PUMI_BIN, then the bundle's, then this checkout's build", () => {
    const has = (paths: string[]) => (p: string) => paths.includes(p)
    expect(cliPath({ PUMI_BIN: "/x/pumi" }, "/A/pumi.app/Contents/MacOS/pumi", has(["/x/pumi"]))).toBe("/x/pumi")
    const bundled = "/A/pumi.app/Contents/Resources/bin/pumi"
    expect(cliPath({}, "/A/pumi.app/Contents/MacOS/pumi", has([bundled]))).toBe(bundled)
    expect(cliPath({ PUMI_BIN: "/gone" }, "/A/pumi.app/Contents/MacOS/pumi", has([bundled]))).toBe(bundled)
  })

  test("agents are looked for where they are installed, before an app's bare PATH, without repeats", () => {
    const path = agentPath("/usr/bin:/bin:/opt/homebrew/bin", "/Users/u").split(":")
    expect(path.slice(0, 2)).toEqual(["/Users/u/.local/bin", "/Users/u/.cargo/bin"])
    expect(path.filter((d) => d === "/opt/homebrew/bin")).toHaveLength(1)
    expect(path.slice(-3)).toEqual(["/usr/bin", "/bin", "/opt/homebrew/bin"])
  })

  test("reads the list, and nothing from what is not one", () => {
    expect(parseList(JSON.stringify({ agents: [agent(), { label: "no id" }] })).map((a) => a.id)).toEqual(["claude"])
    expect(parseList("not json")).toEqual([])
    expect(parseList("{}")).toEqual([])
  })

  test("a row's state and its one action", () => {
    expect(hookState(agent())).toEqual({ text: "Installed" })
    expect(hookState(agent({ hook: "outdated" })).action).toBe("update")
    expect(hookState(agent({ hook: "not_installed" }))).toEqual({ text: "Not installed", action: "install" })
    expect(hookState(agent({ hook: "not_installed", available: false })).text).toBe("Not found")
    expect(hookState(agent({ hook: "unsupported" })).action).toBeUndefined()
  })
})

describe("errors", () => {
  test("the command line's JSON error is its message; other text is itself", async () => {
    const { messageOf } = await import("./skills.ts")
    expect(messageOf('{"kind":"not_found","message":"no agent `nope`"}')).toBe("no agent `nope`")
    expect(messageOf("Claude Code: permission denied")).toBe("Claude Code: permission denied")
    expect(messageOf("{broken")).toBe("{broken")
  })
})
