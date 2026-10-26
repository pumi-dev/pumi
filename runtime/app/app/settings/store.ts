// What Settings reads and writes: the options there are (`registry.list("option")`), their global
// values (the window's state has them), and a value set here, which the server is told
// (`option.set`) and settings.json keeps, so it outlives the server.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { useEffect, useMemo, useState } from "react"
import { Engine } from "../../native/addon.js"
import { subscribeEvent } from "../../sdk/events.ts"
import { settable, type Decl, type OptionValue } from "../../sdk/settings/options.ts"
import { call, store } from "../engine.ts"
import { GHOSTTY_SOURCE, isGhosttyOption, keepInGhostty } from "../ghostty.ts"
import { useApp } from "../state.ts"

/** The file the server reads its options from at startup (`crates/pumi-host/src/overlay.rs`).
 *  The terminals' settings are kept in config.ghostty instead (`../ghostty.ts`). */
export const SETTINGS_FILE = join(Engine.stdpath("config") ?? ".", "settings.json")
/** Who set a value Settings sets, as the server records it. */
export const SOURCE = "settings.json"

/**
 * Keeps `value` under `options.<name>` in settings.json (`null` takes it out), written whole to a
 * file beside it and moved over it, so a reader never sees half of one.
 */
export function keep(name: string, value: OptionValue, file = SETTINGS_FILE): void {
  let json: Record<string, unknown> = {}
  try {
    const text = readFileSync(file, "utf8")
    if (text.trim()) json = JSON.parse(text)
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code !== "ENOENT") throw new Error(`${file}: ${(err as Error).message}`)
  }
  const options = typeof json.options === "object" && json.options && !Array.isArray(json.options) ? (json.options as Record<string, unknown>) : {}
  if (value === null) delete options[name]
  else options[name] = value
  json.options = options
  mkdirSync(dirname(file), { recursive: true })
  const tmp = `${file}.tmp`
  writeFileSync(tmp, `${JSON.stringify(json, null, 2)}\n`)
  renameSync(tmp, file)
}

/** What a slider's drag sets many times: written once it rests. */
const pending = new Map<string, OptionValue>()
let flushing: ReturnType<typeof setTimeout> | undefined

function keepSoon(name: string, value: OptionValue) {
  pending.set(name, value)
  clearTimeout(flushing)
  flushing = setTimeout(() => {
    for (const [n, v] of pending) {
      try {
        // The terminals' settings are config.ghostty's; settings.json keeps the rest.
        // Its options are all strings, numbers and switches.
        if (isGhosttyOption(n)) keepInGhostty(n, v as string | number | boolean | null, store.get().options)
        else keep(n, v)
      } catch (err) {
        console.error(`pumi: keeping ${n}:`, (err as Error).message)
      }
    }
    pending.clear()
  }, 250)
}

/** Sets `name` globally, as Settings does, and keeps it; `null` resets it. */
export async function setOption(name: string, value: OptionValue): Promise<void> {
  keepSoon(name, value)
  await call("option.set", name, value, { scope: "global", source: isGhosttyOption(name) ? GHOSTTY_SOURCE : SOURCE })
}

/** Every option Settings shows, read again when the registry changes. */
export function useDecls(): { decls: Decl[]; error?: string } {
  const [got, setGot] = useState<{ decls: Decl[]; error?: string }>({ decls: [] })
  useEffect(() => {
    let live = true
    const load = () =>
      call("registry.list", "option", {}).then(
        (list: Decl[]) => live && setGot({ decls: (list ?? []).filter(settable) }),
        (err: Error) => live && setGot((was) => ({ ...was, error: String(err?.message ?? err) })),
      )
    load()
    const off = subscribeEvent("registry.changed", undefined, load)
    return () => {
      live = false
      off()
    }
  }, [])
  return got
}

/**
 * The options' values as Settings shows them: the window's, and what was just set here until the
 * server says so (so a switch turns at once). `null` set is the default again.
 */
export function useValues(decls: Decl[]) {
  const options = useApp((s) => s.options)
  const [assumed, setAssumed] = useState<Record<string, OptionValue>>({})
  // What the server now says too is no longer assumed.
  useEffect(() => {
    setAssumed((was) => {
      const left = Object.entries(was).filter(([name, value]) => JSON.stringify(options[name] ?? null) !== JSON.stringify(value))
      return left.length === Object.keys(was).length ? was : Object.fromEntries(left)
    })
  }, [options])
  return useMemo(() => {
    const byName = new Map(decls.map((d) => [d.name, d]))
    const own = (name: string): OptionValue | undefined => (name in assumed ? assumed[name] : (options[name] as OptionValue | undefined))
    return {
      decl: (name: string) => byName.get(name),
      value: (name: string): OptionValue => own(name) ?? byName.get(name)?.default ?? null,
      isSet: (name: string) => own(name) !== undefined && own(name) !== null,
      set: (name: string, value: OptionValue) => {
        setAssumed((was) => ({ ...was, [name]: value }))
        setOption(name, value).catch((err) => console.error(`pumi: setting ${name}:`, err?.message ?? err))
      },
    }
  }, [decls, options, assumed])
}
