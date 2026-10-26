import { describe, expect, mock, test } from "bun:test"

// `mock.module` is process-wide: the app's other tests hear this stub too, so it carries what
// they import from the engine (`call`, `on`) beside what `state.ts` needs.
mock.module("./engine.ts", () => ({
  store: { get: () => ({}), subscribe: () => () => {} },
  call: () => Promise.resolve(null),
  on: () => () => {},
}))
const { agentsOf, HeldModifiers } = await import("./state.ts")

const tab = (id: string, extra: Record<string, unknown> = {}) => ({
  tab_id: id,
  block: "terminal",
  view_id: "@1",
  workspace_id: "$1",
  agent_status: "idle",
  agent: "claude",
  terminal_title: "zsh",
  ...extra,
})
const snapshot = (tabs: unknown[]) => ({ workspaces: [], views: [], layouts: [], tabs }) as any

describe("agentsOf", () => {
  test("a snapshot whose agents did not change gives the same list", () => {
    const first = agentsOf(snapshot([tab("%1"), tab("%2"), tab("%3", { agent: undefined })]))
    const second = agentsOf(snapshot([tab("%1"), tab("%2"), tab("%3", { agent: undefined, terminal_title: "vim" })]))
    expect(second).toBe(first)
    expect(second.map((a) => a.tab_id)).toEqual(["%1", "%2"])
  })

  test("only the agent that changed is a new object", () => {
    const first = agentsOf(snapshot([tab("%1"), tab("%2")]))
    const second = agentsOf(snapshot([tab("%1"), tab("%2", { agent_status: "working" })]))
    expect(second).not.toBe(first)
    expect(second[0]).toBe(first[0]!)
    expect(second[1]).not.toBe(first[1]!)
    expect(second[1]!.agent_status).toBe("working")
  })

  test("a plugin's tab is an agent, with what it says of itself", () => {
    const inView = tab("%4", { block: "pumi_dev.acp.chat", agent: "codex", agent_model: "gpt-5", agent_session: "s1", agent_transcript: "/t.jsonl", agent_conversation: "pumi_dev/acp" })
    const [agent] = agentsOf(snapshot([inView]))
    expect(agent).toMatchObject({ tab_id: "%4", block: "pumi_dev.acp.chat", agent: "codex", model: "gpt-5", session: "s1", transcript: "/t.jsonl", conversation: "pumi_dev/acp" })
  })

  test("what an agent accepts is its words, and the same words keep the same object", () => {
    const accepting = (words: unknown) => tab("%5", { options: { "@agent.accepts": words } })
    const [first] = agentsOf(snapshot([accepting(["queue", "wake"])]))
    expect(first!.accepts).toEqual(["queue", "wake"])
    const [again] = agentsOf(snapshot([accepting(["queue", "wake"])]))
    expect(again).toBe(first!)
    const [none] = agentsOf(snapshot([accepting("queue")]))
    expect(none!.accepts).toEqual([])
  })
})

describe("HeldModifiers", () => {
  const mods = (held: Partial<Record<"ctrl" | "alt" | "shift" | "cmd", boolean>>) => ({ ctrl: false, alt: false, shift: false, cmd: false, ...held })

  test("what is held, as a chord writes it before its key, and a word only when it changes", () => {
    const held = new HeldModifiers()
    let heard = 0
    held.subscribe(() => heard++)
    held.set(mods({ cmd: true }))
    expect(held.get()).toBe("cmd")
    held.set(mods({ cmd: true }))
    expect(heard).toBe(1)
    held.set(mods({ cmd: true, shift: true, ctrl: true }))
    expect(held.get()).toBe("ctrl+shift+cmd")
    held.clear()
    expect(held.get()).toBe("")
    expect(heard).toBe(3)
  })
})
