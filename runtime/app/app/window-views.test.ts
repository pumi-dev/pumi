// A window's view is its own: the project's current view moving does not move it, and a view the
// server closes only moves the windows that showed it.
import { describe, expect, test } from "bun:test"
import { settle, type Placement, type PlacementState } from "./window-views.ts"

const project = (project_id: string, active_view_id?: string) => ({ project_id, active_view_id })
const view = (view_id: string, project_id: string) => ({ view_id, project_id })
const remembered = (entries: [string, string][] = []) => new Map(entries)
const NO_REMEMBERED = remembered()

const s1 = project("$1", "@2")
const s2 = project("$2", "@4")
const v1 = view("@1", "$1")
const v2 = view("@2", "$1")
const v3 = view("@3", "$1")
const v4 = view("@4", "$2")
const STATE: PlacementState = { projects: [s1, s2], views: [v1, v2, v3, v4] }

describe("settle", () => {
  test("a window keeps its own view while the project's in front moves", () => {
    const window: Placement = { project: "$1", view: "@1", listed: true }
    expect(settle(window, NO_REMEMBERED, STATE)).toEqual({ project: "$1", view: "@1", listed: true, pending: undefined })
    // The project's active view is @2, and another window moved it: this one stays on @1.
    expect(settle(window, NO_REMEMBERED, { ...STATE, projects: [project("$1", "@3"), s2] }).view).toBe("@1")
  })

  test("a window with no view takes the project's in front, else its first", () => {
    const window: Placement = { project: "$1", listed: true }
    expect(settle(window, NO_REMEMBERED, STATE).view).toBe("@2")
    expect(settle(window, NO_REMEMBERED, { ...STATE, projects: [project("$1"), s2] }).view).toBe("@1")
  })

  test("a view the server closed gives way to the one the window showed there last, then the project's", () => {
    const window: Placement = { project: "$1", view: "@1", listed: true }
    // @1 is gone: of what is left, the view the window showed there last, else the project's.
    const closed: PlacementState = { ...STATE, views: [v2, v3, v4] }
    expect(settle(window, remembered([["$1", "@3"]]), closed).view).toBe("@3")
    expect(settle(window, remembered([["$1", "@9"]]), closed).view).toBe("@2")
    const alone: PlacementState = { projects: [project("$1"), s2], views: [v3, v4] }
    expect(settle(window, NO_REMEMBERED, alone).view).toBe("@3")
  })

  test("a view the server just made is left alone until the snapshot lists it, and taken then", () => {
    const window: Placement = { project: "$1", view: "@1", listed: true, pending: "@9" }
    expect(settle(window, NO_REMEMBERED, STATE)).toEqual({ project: "$1", view: "@1", listed: true, pending: "@9" })
    const listed: PlacementState = { ...STATE, views: [...STATE.views, view("@9", "$1")] }
    expect(settle(window, NO_REMEMBERED, listed)).toEqual({ project: "$1", view: "@9", listed: true, pending: undefined })
    // A view the window showed and the server does not have (it closed while the app was off) is
    // gone, not on its way: it takes the project's.
    expect(settle({ project: "$1", view: "@9", listed: true }, NO_REMEMBERED, STATE).view).toBe("@2")
    // A view made in another project: the window shows nothing of that project's until it arrives.
    const moved: Placement = { project: "$2", view: "@9", listed: true, pending: "@9" }
    expect(settle(moved, NO_REMEMBERED, STATE)).toEqual({ project: "$2", view: "@9", listed: true, pending: "@9" })
    const arrived: PlacementState = { ...STATE, views: [...STATE.views, view("@9", "$2")] }
    expect(settle(moved, NO_REMEMBERED, arrived)).toEqual({ project: "$2", view: "@9", listed: true, pending: undefined })
  })

  test("a project the server closed gives way to the one in front, with a view of it", () => {
    const window: Placement = { project: "$1", view: "@1", listed: true }
    const closed: PlacementState = { projects: [s2], views: [v4], focused_project_id: "$2" }
    expect(settle(window, NO_REMEMBERED, closed)).toEqual({ project: "$2", view: "@4", listed: true, pending: undefined })
    // A window saved on a project that is gone moves to the one in front, or the first there is.
    expect(settle({ project: "$1", listed: true }, NO_REMEMBERED, { projects: [s2], views: [v4] })).toEqual({
      project: "$2",
      view: "@4",
      listed: true,
      pending: undefined,
    })
    expect(settle({ listed: false }, NO_REMEMBERED, closed)).toEqual({ project: "$2", view: "@4", listed: true, pending: undefined })
  })

  test("a project the snapshot has yet to list is left alone, and so is a window with nothing to show", () => {
    const made: Placement = { project: "$9", listed: false }
    expect(settle(made, NO_REMEMBERED, STATE)).toEqual({ project: "$9", view: undefined, listed: false, pending: undefined })
    expect(settle({ listed: false }, NO_REMEMBERED, { projects: [], views: [] })).toEqual({
      project: undefined,
      view: undefined,
      listed: false,
      pending: undefined,
    })
  })
})
