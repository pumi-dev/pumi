import { describe, expect, test } from "bun:test"
import { answerNotice, updateNotices } from "./update-notices.ts"

const idle = { app: { status: "idle" }, plugins: { found: null } }

describe("update notices", () => {
  test("none while nothing is ready", () => {
    expect(updateNotices(idle, {})).toEqual([])
    expect(updateNotices({ app: { status: "downloading", version: "0.45.0" }, plugins: { found: [] } }, {})).toEqual([])
  })

  test("pumi's update once ready, and the plugins with new versions", () => {
    const state = { app: { status: "ready", version: "0.45.0" }, plugins: { found: [{ name: "agents", to: "1.2.0" }, { name: "browser", to: "0.3.0" }] } }
    const notices = updateNotices(state, {})
    expect(notices.map((n) => [n.kind, n.title])).toEqual([
      ["app", "pumi 0.45.0 is ready"],
      ["plugins", "2 plugins have updates"],
    ])
    expect(updateNotices({ ...state, app: idle.app, plugins: { found: [{ name: "agents", to: "1.2.0" }] } }, {})[0].title).toBe("agents has an update")
  })

  test("pumi's update links to its release notes when the updater knows the page", () => {
    const notes = "https://github.com/pumi-dev/pumi/releases/tag/v0.45.0"
    const [app] = updateNotices({ app: { status: "ready", version: "0.45.0", notes }, plugins: { found: null } }, {})
    expect(app.link).toEqual({ label: "What's new", url: notes })
    expect(updateNotices({ app: { status: "ready", version: "0.45.0", notes: null }, plugins: { found: null } }, {})[0].link).toBeUndefined()
  })

  test("a dismissed one stays hidden until there is something newer", () => {
    const state = { app: { status: "ready", version: "0.45.0" }, plugins: { found: [{ name: "agents", to: "1.2.0" }] } }
    const [app, plugins] = updateNotices(state, {})
    expect(updateNotices(state, { app: app.key })).toEqual([plugins])
    expect(updateNotices(state, { app: app.key, plugins: plugins.key })).toEqual([])
    expect(updateNotices({ ...state, app: { status: "ready", version: "0.46.0" } }, { app: app.key })[0].title).toBe("pumi 0.46.0 is ready")
    expect(updateNotices({ ...state, plugins: { found: [{ name: "agents", to: "1.3.0" }] } }, { app: app.key, plugins: plugins.key })[0].kind).toBe("plugins")
  })
})

describe("a check a person asked for", () => {
  const asked = (app: Record<string, unknown>) => answerNotice({ app: { status: "idle", ...app }, plugins: { found: null }, manual: true })

  test("is answered either way, up to date and all", () => {
    const upToDate = asked({ status: "upToDate", current: "0.48.0", channel: "stable" })
    expect(upToDate?.title).toBe("pumi 0.48.0 is up to date")
    expect(upToDate?.detail).toBe("That is the newest version on the stable channel.")
    expect(upToDate?.action).toBeUndefined()
    expect(asked({ status: "upToDate" })?.title).toBe("pumi is up to date")
  })

  test("says why the check could not be made", () => {
    const unavailable = asked({ status: "unavailable", error: "it is a development build" })
    expect(unavailable?.title).toBe("This copy does not update itself")
    expect(unavailable?.detail).toBe("it is a development build")
    const failed = asked({ status: "failed", error: "no network" })
    expect(failed?.title).toBe("Could not check for updates")
    expect(failed?.detail).toBe("no network")
  })

  test("says a version is out where this copy installs nothing", () => {
    const found = asked({ status: "found", version: "0.49.0", error: "it is a development build" })
    expect(found?.title).toBe("pumi 0.49.0 is out")
    expect(found?.detail).toBe("This copy does not install it: it is a development build.")
  })

  test("says nothing while a check runs, or when nobody asked", () => {
    for (const status of ["idle", "checking", "downloading", "ready"]) expect(asked({ status, version: "0.49.0" })).toBeUndefined()
    expect(answerNotice({ app: { status: "upToDate" }, plugins: { found: null } })).toBeUndefined()
    expect(answerNotice(idle)).toBeUndefined()
  })
})
