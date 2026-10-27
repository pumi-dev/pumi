// The plugins: which run, when, and everything between them and the server. It runs only in the
// app pumid names plugin host (the first window app to connect); another app's plugins draw their
// `ui.tsx` in its own windows, and their logic runs once, here.
//
// A plugin's `main.ts` runs in a worker (worker/), supervised by `Host` (the crash rule and the
// budgets). What it calls goes to the server with its identity; what it provides is registered
// for it, and the server's requests and callbacks come back to it.
import { existsSync, mkdirSync, appendFileSync, renameSync, statSync, readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { Engine } from "../native/addon.js"
import { Host, limits, type Frame, type Load } from "../worker/plugins.ts"
import { call, engine, on } from "./engine.ts"
import { deliver, forget } from "../sdk/channels.ts"
import { listen, unlisten } from "../sdk/events.ts"
import { APP_FUNCTIONS } from "./updates.ts"

const API_LEVEL = 1
const LOG_LIMIT = 1 << 20

/** A plugin's manifest: `package.json`'s `pumi`. */
export interface Manifest {
  name: string
  owner?: string
  api?: number
  isolate?: boolean
  activation?: string[]
  themes?: string[]
  contributes?: {
    commands?: { id: string; title: string }[]
    keys?: { key: string; command: string }[]
    options?: Record<string, unknown>
    blocks?: { id: string; title?: string; icon?: string }[]
    /** The blocks it draws but did not define: block → `ui.tsx` export; for the core's `file`,
     *  optionally with the files it draws (`{ export: "Editor", match: ["*.md"] }`). */
    renderers?: Record<string, string | { export: string; match: string[] }>
    slots?: Record<string, Slot>
    /** Its pages in Settings, and its sections on one (a built-in page or its own). */
    settings?: {
      pages?: { id: string; title: string; icon?: string; component: string; keywords?: string[] }[]
      sections?: { id: string; page: string; title: string; component: string; keywords?: string[] }[]
    }
  }
  [field: string]: unknown
}

/** What a plugin adds to one of the window's slots. */
export interface Slot {
  actions?: { id: string; title: string; icon?: string; command: string }[]
  /** An export of its `src/ui.tsx`. */
  component?: string
  title?: string
  icon?: string
}

export interface Entry {
  /** `owner/name`; `local/<name>` while linked from disk. */
  id: string
  prefix: string
  dir: string
  manifest: Manifest
  /** The worker's file, when it has one. */
  main?: string
  /** The window's file, when it has one. */
  ui?: string
  state: "idle" | "starting" | "running" | "crashed" | "disabled" | "stopped"
  error?: string | null
  active: boolean
  sent: boolean
}

/** What a run made on the server, undone when it stops. */
class Made {
  private undo: [string, unknown[]][] = []
  record(name: string, args: any[], result: any) {
    switch (name) {
      case "event.on": if (typeof result === "number") this.undo.push(["event.off", [result]]); break
      case "fn.set": if (typeof args[0] === "string" && args[1]) this.undo.push(["fn.set", [args[0], null]]); break
      case "menu.set": if (args[1]) this.undo.push(["menu.set", [args[0], null]]); break
    }
  }
  take(): [string, unknown[]][] {
    return this.undo.splice(0).reverse()
  }
}

export const entries = new Map<string, Entry>()
const made = new Map<string, Made>()
/** The functions each plugin provides, by name. */
const provided = new Map<string, string>()
const listeners = new Set<() => void>()
let host: Host | undefined
let isHost = false
/** The plugins `pumi plugin dev --inspect` runs under Bun's inspector. */
const inspected = new Set<string>()

function changed() {
  for (const listener of listeners) listener()
  publish()
}

export function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** A plugin's option and command prefix, from its id (`owner/name` → `owner.name.`). */
export const prefixOf = (id: string) => `${id.replace("/", ".").replaceAll("-", "_")}.`

function readManifest(dir: string): Manifest | string {
  try {
    const json = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"))
    if (!json?.pumi || typeof json.pumi !== "object") return "its package.json has no `pumi`"
    return json.pumi as Manifest
  } catch (err: any) {
    return `its package.json: ${err?.message ?? err}`
  }
}

/** `package.json`'s `main`: what its worker runs. */
function packageMain(dir: string): string | undefined {
  try {
    const main = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"))?.main
    return typeof main === "string" ? main : undefined
  } catch {
    return undefined
  }
}

function file(dir: string, given: string | undefined, fallback: string): string | undefined {
  const path = join(dir, given ?? fallback)
  return existsSync(path) ? path : undefined
}

/** Reads what is installed and enabled, and starts what should run. */
async function catalog() {
  const seen = new Set<string>()
  const packs: any[] = (await call("registry.list", "plugin", {}).catch(() => [])) ?? []
  for (const p of packs) {
    if (p.active !== true) continue
    const dir: string = p.path
    const manifest: Manifest | string = p.error ? String(p.error) : (p.manifest ?? readManifest(dir))
    const id: string = p.id ?? p.spec?.name ?? dir
    seen.add(id)
    if (typeof manifest === "string") {
      entries.set(id, { id, prefix: prefixOf(id), dir, manifest: { name: id }, state: "disabled", error: manifest, active: false, sent: false })
      continue
    }
    if ((manifest.api ?? 0) !== API_LEVEL) {
      const error = `it is written for API level ${manifest.api ?? "(none)"}; this pumi runs ${API_LEVEL}`
      entries.set(id, { id, prefix: prefixOf(id), dir, manifest, state: "disabled", error, active: false, sent: false })
      continue
    }
    const old = entries.get(id)
    const entry: Entry = {
      id,
      prefix: prefixOf(id),
      dir,
      manifest,
      main: file(dir, packageMain(dir), "src/main.ts"),
      ui: file(dir, undefined, "src/ui.tsx"),
      state: old?.state ?? "idle",
      error: old?.error,
      active: old?.active ?? false,
      sent: old?.sent ?? false,
    }
    entries.set(id, entry)
    route(entry)
    if (entry.manifest.activation?.includes("onStartup")) activate(id)
    for (const a of entry.manifest.activation ?? []) {
      if (a.startsWith("onEvent:") && !entry.active) {
        listen(a.slice(8), { desc: `activates ${id}`, once: true }, () => activate(id)).catch(() => {})
      }
    }
  }
  for (const id of [...entries.keys()]) if (!seen.has(id)) stop(id)
  changed()
}

/**
 * Registers what the manifest says it answers before its worker starts: its commands, and (when
 * it has a window side) the function another app's windows call its `main.ts` exports through.
 * What it answered before and no longer declares goes: a reload reads the manifest again.
 */
function route(entry: Entry) {
  if (!isHost) return
  const answers = new Set<string>()
  const answer = (name: string, spec: Record<string, unknown>) => {
    answers.add(name)
    provide(entry.id, name, spec)
  }
  for (const command of entry.manifest.contributes?.commands ?? []) answer(`${entry.prefix}${command.id}`, { desc: command.title })
  if (entry.ui && entry.main) answer(`${entry.prefix}rpc`, { desc: `Calls an export of ${entry.id}'s main.ts, for its ui.tsx in another app` })
  withdraw(
    entry.id,
    [...provided].filter(([name, owner]) => owner === entry.id && !answers.has(name)).map(([name]) => name),
  )
}

function provide(pkg: string, name: string, spec: Record<string, unknown>): Promise<null> {
  const owner = provided.get(name)
  if (owner && owner !== pkg) return Promise.reject(new Error(`${name} is provided by ${owner}`))
  provided.set(name, pkg)
  engine.provide({
    name,
    since: 1,
    provider: pkg,
    runs_on: "client",
    effect: "write",
    agent: false,
    desc: "",
    parameters: [],
    params_schema: { type: "object" },
    returns: "Object",
    returns_schema: {},
    examples: [{ args: [] }],
    notes: [],
    source: pkg,
    ...spec,
  })
  return Promise.resolve(null)
}

function withdraw(pkg: string, names: string[]) {
  const mine = names.filter((n) => provided.get(n) === pkg)
  for (const n of mine) provided.delete(n)
  for (const n of mine) call("fn.set", n, null).catch(() => {})
}

/** Starts `id`'s worker (once) when something wants it. */
export function activate(id: string) {
  const entry = entries.get(id)
  if (!entry || entry.state === "disabled" && entry.error) return
  entry.active = true
  if (!entry.main || !host || entry.sent) return
  entry.sent = true
  entry.state = "starting"
  host.frame({ type: "load", ...loadOf(entry) })
}

function loadOf(entry: Entry): Load {
  const paths = { config: "", state: "", cache: "" }
  for (const kind of ["config", "state", "cache"] as const) {
    paths[kind] = join(Engine.stdpath(kind) ?? "", "plugins", entry.prefix.slice(0, -1))
  }
  return {
    pkg: entry.id,
    dir: entry.dir,
    main: entry.main,
    api: API_LEVEL,
    prefix: entry.prefix,
    paths,
    isolate: entry.manifest.isolate === true,
    inspect: inspected.has(entry.id),
  }
}

function stop(id: string) {
  host?.frame({ type: "stop", pkg: id })
  undo(id)
  forget(id)
  entries.delete(id)
}

function undo(id: string) {
  for (const [name, args] of made.get(id)?.take() ?? []) {
    ;(name === "event.off" ? unlisten(args[0] as number) : call(name, ...args)).catch(() => {})
  }
  withdraw(id, [...provided].filter(([, p]) => p === id).map(([n]) => n))
}

/** `args` as the server takes them from `pkg`: callbacks bound to it, storage its own. */
function bind(pkg: string, value: any): any {
  if (Array.isArray(value)) return value.map((v) => bind(pkg, v))
  if (value && typeof value === "object") {
    const keys = Object.keys(value)
    if (keys.length === 1 && keys[0] === "ref" && typeof value.ref === "number") return { chan: 0, ref: `${pkg}/${value.ref}` }
    return Object.fromEntries(keys.map((k) => [k, bind(pkg, value[k])]))
  }
  return value
}

async function callFor(pkg: string, name: string, args: any[], caller?: unknown): Promise<any> {
  // A function another plugin provides is served here, not by the server.
  const provider = provided.get(name)
  if (provider) return serve(provider, name, args, { name: pkg })
  const params = bind(pkg, args)
  // A plugin's subscription is the app channel's, made for the plugin: its events go to its worker.
  const result =
    name === "event.on"
      ? await listen(params[0], params[1] ?? {}, (event, subscription) => host?.frame({ type: "event", pkg, subscription, event }), caller ?? { name: pkg })
      : name === "event.off"
        ? await unlisten(params[0])
        : await engine.call(name, params, caller ?? { name: pkg })
  if (!made.has(pkg)) made.set(pkg, new Made())
  made.get(pkg)!.record(name, params, result)
  return result
}

let nextServe = 0
const serving = new Map<number, { resolve: (v: any) => void; reject: (e: any) => void }>()

/** Calls `name`, which `pkg` provides, in its worker (starting it). */
function serve(pkg: string, name: string, args: any, caller: unknown): Promise<any> {
  activate(pkg)
  return new Promise((resolve, reject) => {
    const id = ++nextServe
    serving.set(id, { resolve, reject })
    if (!host) return reject(new Error("plugins are not running in this app"))
    host.frame({ type: "serve", id, pkg, name, args, caller })
  })
}

function frame(f: Frame) {
  const entry = entries.get(f.pkg)
  switch (f.type) {
    case "call":
      return callFor(f.pkg, f.name, f.args ?? [], f.caller).then(
        (ok) => host?.frame({ type: "reply", id: f.id, ok }),
        (err) => host?.frame({ type: "reply", id: f.id, error: { kind: "exception", message: String(err?.message ?? err) } }),
      )
    case "provide":
      return provide(f.pkg, f.name, f.spec ?? {}).then(
        () => host?.frame({ type: "reply", id: f.id, ok: null }),
        (err) => host?.frame({ type: "reply", id: f.id, error: { kind: "invalid_params", message: String(err?.message ?? err) } }),
      )
    case "withdraw":
      return withdraw(f.pkg, f.names ?? [])
    case "reply": {
      const s = serving.get(f.id)
      if (!s) return
      serving.delete(f.id)
      return f.error ? s.reject(Object.assign(new Error(f.error.message), { kind: f.error.kind })) : s.resolve(f.ok ?? null)
    }
    case "status":
      if (!entry) return
      entry.state = f.state
      entry.error = f.error
      if (f.state === "disabled") entry.active = false
      if (["crashed", "disabled", "stopped"].includes(f.state)) {
        entry.sent = f.state === "crashed"
        undo(entry.id)
        route(entry)
      }
      return changed()
    case "raised":
      return log(f.pkg, "stderr", String(f.error))
    case "log":
      return log(f.pkg || "host", f.stream, f.line)
    case "ui":
      return deliver(f.pkg, String(f.topic), f.data)
  }
}

/** Writes `line` to `id`'s log (`pumi plugin logs`), as its stderr. */
export function note(id: string, line: string) {
  log(id, "stderr", line)
}

function log(id: string, stream: string, line: string) {
  const path = Engine.logPath(id)
  try {
    mkdirSync(dirname(path), { recursive: true })
    if (existsSync(path) && statSync(path).size > LOG_LIMIT) renameSync(path, `${path}.1`)
    appendFileSync(path, `${Math.floor(Date.now() / 1000)}${stream === "stderr" ? "!" : " "} ${line}\n`)
  } catch {}
}

/** What `pumi plugin list` reads: `id=state=detail` lines. */
function publish() {
  if (!isHost) return
  const lines = [...entries.values()].map((e) => `${e.id}=${e.state}=${(e.error ?? e.dir).replace(/\n/g, " ")}`)
  call("option.set", "@ui_plugins", lines.join("\n"), { scope: "global" }).catch(() => {})
}

const reloads = new Set<(id: string) => void>()

/** Hears each plugin reloaded (its window side loads again). */
export function onReload(listener: (id: string) => void) {
  reloads.add(listener)
  return () => reloads.delete(listener)
}

/** A plugin installed, removed or changed (`registry.changed`): its worker starts again. */
async function reload(event: any) {
  await catalog()
  const names: string[] = Array.isArray(event?.names) ? event.names.map(String) : []
  for (const entry of entries.values()) {
    if (names.length && !names.some((name) => entry.id === name || entry.manifest.name === name)) continue
    for (const listener of reloads) listener(entry.id)
    if (!entry.active && names.length === 0) continue
    entry.error = null
    entry.active = true
    if (entry.sent) host?.frame({ type: "reload", pkg: entry.id, spec: loadOf(entry) })
    else activate(entry.id)
  }
  changed()
}

/** `pumi plugin dev` asks for a plugin again (every one without a `name`); `inspect` runs it under
 *  Bun's inspector. */
function reloadRequested(data: any) {
  const name = typeof data?.name === "string" ? data.name : ""
  if (name && data?.inspect === true) inspected.add(name)
  else if (name) inspected.delete(name)
  return reload({ names: name ? [name] : [] })
}

/** The callbacks the server makes to a plugin (a function or menu item it set): `<pkg>/<n>`. */
on("callback", ({ ref, args }: { ref: string; args: any }) => {
  const at = ref.lastIndexOf("/")
  const [owner, what] = [ref.slice(0, at), ref.slice(at + 1)]
  const data = Array.isArray(args) && args.length === 1 ? args[0] : args
  const fn = Number(what)
  if (Number.isFinite(fn) && entries.has(owner)) host?.frame({ type: "callback", pkg: owner, fn, args: data })
})

/** A call to a function a plugin provides: the app serves it, and answers the server with it. */
on("request", ({ id, name, args, caller }: { id: number; name: string; args: any; caller: unknown }) => {
  // The app's own (`update.*`).
  const own = APP_FUNCTIONS.get(name)
  if (own) {
    Promise.resolve()
      .then(() => own(Array.isArray(args) ? args : [args]))
      .then(
        (ok) => engine.answer(id, ok ?? null),
        (err) => engine.answer(id, null, String(err?.message ?? err)),
      )
    return
  }
  const pkg = provided.get(name)
  if (!pkg) return engine.answer(id, null, `nothing in this app provides ${name}`)
  const rpc = name === `${entries.get(pkg)?.prefix}rpc`
  const asked = rpc ? (Array.isArray(args) ? args[0] : args) : undefined
  const served = rpc ? serve(pkg, `export:${asked?.name}`, asked?.args ?? [], caller) : serve(pkg, name, args, caller)
  served.then(
    (ok) => engine.answer(id, ok),
    (err) => engine.answer(id, null, String(err?.message ?? err)),
  )
})

/** Whether this app runs the plugins; it starts them when it does, and stops them when not. */
async function elect() {
  const was = isHost
  isHost = (await call("channel.getInfo", 0).catch(() => undefined))?.primary === true
  if (isHost && !was) {
    host = new Host(frame, limits())
    for (const entry of entries.values()) entry.sent = false
    await catalog()
  } else if (!isHost && was) {
    host?.shutdown()
    host = undefined
  }
  changed()
}

let started = false
/** Starts the plugins, once the server is reachable. `safe` (`--safe`) runs none. */
export async function start(safe: boolean) {
  if (started) return
  started = true
  if (safe) return
  await listen("client.detached", { desc: "the plugin host may change" }, () => elect()).catch(() => {})
  // A plugin installed, removed or reloaded: `registry.changed` of the `plugin` kind.
  await listen("registry.changed", { match: "plugin", desc: "reloads plugins" }, (e) => reload(e.data)).catch(() => {})
  await listen("plugin.reloadRequested", { desc: "reloads a plugin (`pumi plugin dev`)" }, (e) => reloadRequested(e.data)).catch(() => {})
  await elect()
  // Every app draws its plugins' window side; only the host runs their workers.
  if (!isHost) await catalog()
}

/** Ends every plugin: the app quits. */
export function shutdown() {
  host?.shutdown()
}

/** The plugins with a window side, for the window to load. */
export function uiEntries(): Entry[] {
  return [...entries.values()].filter((e) => e.ui && e.state !== "disabled")
}

/** Calls the export `name` of `pkg`'s `main.ts`, from its `ui.tsx`: here, or through the host. */
export function invoke(pkg: string, name: string, args: unknown[]): Promise<any> {
  const entry = entries.get(pkg)
  if (!entry) return Promise.reject(new Error(`${pkg} is not installed`))
  if (isHost) return serve(pkg, `export:${name}`, args, { name: pkg })
  // The app that runs the plugins provides the call, and may still be joining (it just started,
  // or the plugin was added while it ran): until it has, ask again rather than fail.
  const until = Date.now() + 3000
  const again = async (wait: number) => {
    if (Date.now() > until) throw new Error(`${pkg} is not running yet`)
    await new Promise((done) => setTimeout(done, wait))
    return invoke(pkg, name, args)
  }
  return engine.call(`${entry.prefix}rpc`, [{ name, args }], { name: pkg }).catch((err: any) => {
    const message = String(err?.message ?? err)
    return message.startsWith("no function ") ? again(50) : Promise.reject(err)
  })
}

/** Runs `pkg`'s command `id` (an action of one of its points), given what the point is of. */
export function command(pkg: string, id: string, args: unknown): Promise<any> {
  const entry = entries.get(pkg)
  if (!entry) return Promise.reject(new Error(`${pkg} is not installed`))
  const name = `${entry.prefix}${id}`
  const given = args === undefined ? [] : [args]
  return provided.get(name) === pkg ? serve(pkg, name, given, { name: pkg }) : engine.call(name, given, { name: pkg })
}
