import { describe, expect, test } from "bun:test"
import { installedOf, listingOf, parseInstallLink, questionOf } from "./install-link.ts"

describe("an install link", () => {
  test("names a GitHub repository", () => {
    expect(parseInstallLink("pumi://plugins/install/someone/pumi-clock")).toEqual({
      owner: "someone",
      repo: "pumi-clock",
      dir: undefined,
      url: "https://github.com/someone/pumi-clock",
      source: "https://github.com/someone/pumi-clock",
      name: "pumi-clock",
    })
    expect(parseInstallLink("pumi://plugins/install/someone/pumi-clock.git/?from=web")?.repo).toBe("pumi-clock")
  })

  test("names one of a repository's plugins by its folder", () => {
    const link = parseInstallLink("pumi://plugins/install/pumi-dev/pumi/plugins/git")!
    expect(link.dir).toBe("plugins/git")
    expect(link.source).toBe("pumi-dev/pumi/plugins/git")
    expect(link.name).toBe("git")
  })

  test("names nothing but GitHub", () => {
    for (const bad of [
      "pumi://settings/general",
      "pumi://plugins/install/someone",
      "pumi://plugins/install/a/../b",
      "pumi://plugins/install/a/b/./c",
      "pumi://plugins/install/a/b%2F..",
      "pumi://plugins/install/-a/b",
      "pumi://plugins/install/a/b//c",
      "pumi://plugins/install/https:/evil.dev/x",
      "file:///Users/me/x",
    ]) {
      expect(parseInstallLink(bad)).toBeUndefined()
    }
  })
})

const link = parseInstallLink("pumi://plugins/install/Pumi-Dev/pumi/plugins/git")!
const listing = { id: "pumi_dev/git", name: "git", owner: "pumi_dev", description: "Git in the sidebar.", src: "https://github.com/pumi-dev/pumi", path: "plugins/git" }

test("its listing is the one at its repository and folder", () => {
  expect(listingOf(link, [{ ...listing, path: "plugins/other" }, listing])).toBe(listing)
  expect(listingOf(parseInstallLink("pumi://plugins/install/pumi-dev/pumi")!, [listing])).toBeUndefined()
})

test("it is installed by its listing's id, or by its name and repository", () => {
  expect(installedOf(link, [{ id: "pumi_dev/git" }], listing)).toBe(true)
  expect(installedOf(link, [{ spec: { name: "git", src: "https://github.com/pumi-dev/pumi" } }])).toBe(true)
  expect(installedOf(link, [{ spec: { name: "git", src: "https://github.com/other/pumi" } }])).toBe(false)
})

test("an unlisted one is asked about as a warning", () => {
  expect(questionOf(link, listing)).toMatchObject({ title: "Install git by pumi_dev?", confirm: "Install" })
  expect(questionOf(link, listing).detail).toContain("github.com/Pumi-Dev/pumi/plugins/git")
  expect(questionOf(link)).toMatchObject({ title: "Install git from GitHub?", danger: true })
})
