// The connection under the `pumi` module: one per plugin, in its worker (or its isolate's
// process), installed by the host (worker.ts) before the plugin's `main` is imported, or by
// `pumi/testing` in a plugin's own tests. Everything the module registers is kept here, so the
// host's messages reach it and a stop can undo it.
//
// host → plugin: reply {id, ok | error} · callback {fn, args} · event {subscription, event} ·
//   serve {id, name, args, caller}
// plugin → host: call {id, name, args, caller?} · provide {id, name, spec} · withdraw {names} ·
//   reply {id, ok | error} · log {stream, line} · ui {topic, data}
//
// A call made while the plugin serves one (`serve`) carries that call's `caller`: what it does
// with no id given is about the caller's tab, not the plugin host's.
import { AsyncLocalStorage } from "node:async_hooks";
import { readFileSync } from "node:fs";

/** Where the module's messages go: the host's side of the worker, or a fake app. */
export interface Port {
  post(message: any): void;
}

/** A plugin's own directories, made for it on the plugin host's machine. */
export interface Paths {
  config: string;
  state: string;
  cache: string;
}

/** An option the manifest declares (`pumi.contributes.options.<key>`). */
export interface OptionSpec {
  type: "boolean" | "number" | "string" | "table" | "secret";
  default?: unknown;
  desc?: string;
  label?: string;
  [field: string]: unknown;
}

/** What the module reads of the manifest (`package.json`'s `pumi`). */
export interface Manifest {
  name: string;
  api: number;
  contributes?: {
    options?: Record<string, OptionSpec>;
    commands?: { id: string; title: string }[];
    keys?: { key: string; command: string }[];
    blocks?: {
      id: string;
      title?: string;
      icon?: string;
      version?: number;
      strategy?: "exact" | "relaunch" | "reference";
      methods?: string[];
    }[];
    renderers?: Record<string, string | { export: string; match: string[] }>;
    slots?: Record<string, { actions?: { id: string; title: string; icon?: string; command: string; pick?: { kind?: "file" | "directory"; match?: string[]; multiple?: boolean; title?: string } }[]; component?: string; title?: string; icon?: string }>;
  };
  [field: string]: unknown;
}

export interface Init {
  /** `owner/name`. */
  pkg: string;
  /** `owner.name.`: what its options, blocks, commands and events start with. */
  prefix: string;
  dir: string;
  main?: string;
  paths?: Paths;
  /** The manifest; read from `dir`'s package.json when not given. */
  manifest?: Manifest;
  /** A script's own session and tab (`pumi do --session --tab`), and what it was called with. */
  session?: string | null;
  tab?: string | null;
  args?: Record<string, unknown>;
}

/**
 * Who called a function the plugin serves (`fn.set`, a command, an export its `ui.tsx` calls):
 * `self.caller` while it runs, `null` outside one.
 */
export interface Caller {
  /** The client (an app attached to the server) it came through; `null` for the plugin host's own app, a script or the CLI. */
  client: number | null;
  /** The tab its calls are about (`%3`): the caller's own, or the one in front where it is. */
  tab: string | null;
  /** Who it is, as provenance says it (`pumi-app`, `cli`, `owner/name`). */
  source: string | null;
}

/** The call being served, as the host sent it (`wire`) and as `self.caller` reads it. */
interface Serving {
  wire: { tab?: number; client?: number };
  caller: Caller;
}
const serving = new AsyncLocalStorage<Serving>();

/** The caller of the function being served, or `null`. */
export function currentCaller(): Caller | null {
  return serving.getStore()?.caller ?? null;
}

function servingOf(given: any): Serving {
  const number = (v: unknown) => (typeof v === "number" && Number.isInteger(v) && v > 0 ? v : undefined);
  const tab = number(given?.tab);
  const client = number(given?.client);
  const wire: Serving["wire"] = {};
  if (tab !== undefined) wire.tab = tab;
  if (client !== undefined) wire.client = client;
  const source = typeof given?.name === "string" ? given.name : null;
  return { wire, caller: { client: client ?? null, tab: tab === undefined ? null : `%${tab}`, source } };
}

/** A call the API or a provider refused: `kind` is the export's error kind (`not_found`, …). */
export class ApiError extends Error {
  constructor(
    public kind: string,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/** Undoes what registered it; `ready` settles once the app has it. `using` disposes it too. */
export type Disposer = (() => void) & { ready: Promise<void>; [Symbol.dispose](): void };

interface Pending {
  resolve(v: any): void;
  reject(e: any): void;
}

class Connection {
  port?: Port;
  init?: Init;
  manifest: Manifest = { name: "", api: 1 };
  private next = 0;
  private pending = new Map<number, Pending>();
  readonly callbacks = new Map<number, (...args: any[]) => any>();
  /** Each subscription's listener (`event.on`'s handle), and whether it hears one event only. */
  readonly listening = new Map<number, { fn: (event: any) => any; once: boolean }>();
  /**
   * Events of a subscription whose handle `event.on` has not answered yet: an event and the
   * answer reach the plugin separately. Kept for a few handles at most.
   */
  readonly early = new Map<number, any[]>();
  readonly provided = new Map<string, (...args: any[]) => any>();
  /** Everything registered and not yet disposed, so a reset undoes it all. */
  readonly live = new Set<Disposer>();
  /** Hears what a handler raised; the host may set it. */
  raised?: (err: unknown) => void;

  get ready(): boolean {
    return this.port !== undefined;
  }

  need(): { port: Port; init: Init } {
    if (!this.port || !this.init) {
      throw new Error("pumi: this code runs in pumi's Bun host; in a test, start `pumi/testing`'s fake app first");
    }
    return { port: this.port, init: this.init };
  }

  install(port: Port, init: Init) {
    this.reset();
    this.port = port;
    this.init = init;
    this.manifest = init.manifest ?? readManifest(init.dir);
  }

  /** Forgets every registration and answer it waits for: a new run starts clean. */
  reset() {
    for (const d of [...this.live]) d();
    this.live.clear();
    for (const p of this.pending.values()) p.reject(new ApiError("provider_unavailable", "the plugin stopped"));
    this.pending.clear();
    this.callbacks.clear();
    this.listening.clear();
    this.early.clear();
    this.provided.clear();
    this.port = undefined;
    this.init = undefined;
  }

  id(): number {
    return ++this.next;
  }

  request(message: any): Promise<any> {
    const { port } = this.need();
    return new Promise((resolve, reject) => {
      const id = this.id();
      this.pending.set(id, { resolve, reject });
      port.post({ ...message, id });
    });
  }

  post(message: any) {
    this.need().port.post(message);
  }

  /** Calls `name` with `args`: functions in them become callbacks it keeps (`refs`). */
  call(name: string, args: unknown[], refs: number[] = []): Promise<any> {
    let list = args.map((a) => this.encode(a, refs));
    // Trailing arguments left out are absent, not null.
    while (list.length > 0 && list.at(-1) === undefined) list = list.slice(0, -1);
    const asked = serving.getStore()?.wire;
    const caller = asked && Object.keys(asked).length > 0 ? { caller: asked } : {};
    return this.request({ t: "call", name, args: list.map((a) => (a === undefined ? null : a)), ...caller });
  }

  /** Runs `fn` with each event of subscription `handle`, and with those it heard before. */
  listen(handle: number, fn: (event: any) => any, once = false) {
    this.listening.set(handle, { fn, once });
    const missed = this.early.get(handle) ?? [];
    this.early.delete(handle);
    for (const event of missed) deliver.event(handle, event);
  }

  /** Stops hearing subscription `handle`. */
  unlisten(handle: number) {
    this.listening.delete(handle);
    this.early.delete(handle);
  }

  /** `value` as it crosses: a function is a callback ref, a handle its id, `undefined` dropped. */
  encode(value: unknown, refs: number[]): any {
    if (typeof value === "function") {
      const ref = this.id();
      this.callbacks.set(ref, value as (...args: any[]) => any);
      refs.push(ref);
      return { ref };
    }
    if (value === null || typeof value !== "object") return value;
    if (typeof (value as any).toJSON === "function") return this.encode((value as any).toJSON(), refs);
    if (Array.isArray(value)) return value.map((v) => (v === undefined ? null : this.encode(v, refs)));
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) if (v !== undefined) out[k] = this.encode(v, refs);
    return out;
  }

  /** A disposer that runs `undo` once and is forgotten by a reset after. */
  disposer(undo: () => void, ready: Promise<unknown>): Disposer {
    let done = false;
    const d = (() => {
      if (done) return;
      done = true;
      this.live.delete(d);
      undo();
    }) as Disposer;
    d.ready = ready.then(() => undefined);
    // A refusal is the registration's to report; the disposer's `ready` carries it, and an
    // unawaited one is not an unhandled rejection.
    d.ready.catch(() => {});
    d[Symbol.dispose] = d;
    this.live.add(d);
    return d;
  }

  settle(m: { id: number; ok?: any; error?: { kind: string; message: string } }) {
    const p = this.pending.get(m.id);
    if (!p) return;
    this.pending.delete(m.id);
    if (m.error) p.reject(new ApiError(m.error.kind, m.error.message));
    else p.resolve(m.ok ?? null);
  }
}

export const connection = new Connection();

/** How many subscriptions' early events are kept. */
const EARLY_LIMIT = 64;

function readManifest(dir: string): Manifest {
  try {
    const json = JSON.parse(readFileSync(`${dir}/package.json`, "utf8"));
    if (json && typeof json.pumi === "object") return json.pumi as Manifest;
  } catch {
    // No package.json (a bare test fixture): an empty manifest.
  }
  return { name: "", api: 1 };
}

const fail = (err: any) => ({
  kind: err instanceof ApiError ? err.kind : "exception",
  message: err instanceof Error ? err.message : String(err),
});

/** What the host hands the module; each returns once the handler's synchronous part has run. */
export const deliver = {
  reply(m: { id: number; ok?: any; error?: { kind: string; message: string } }) {
    connection.settle(m);
  },
  callback(fn: number, args: any) {
    const f = connection.callbacks.get(fn);
    if (!f) return;
    const threw = (err: any) => {
      console.error(`a callback of ${connection.init?.pkg} threw:`, err);
      connection.raised?.(err);
    };
    try {
      Promise.resolve(f(args)).catch(threw);
    } catch (err) {
      threw(err);
    }
  },
  event(subscription: number, event: any) {
    const held = connection.listening.get(subscription);
    if (!held) {
      const list = connection.early.get(subscription) ?? [];
      list.push(event);
      connection.early.set(subscription, list);
      while (connection.early.size > EARLY_LIMIT) connection.early.delete(connection.early.keys().next().value!);
      return;
    }
    if (held.once) connection.listening.delete(subscription);
    const threw = (err: any) => {
      console.error(`a listener of ${connection.init?.pkg} threw:`, err);
      connection.raised?.(err);
    };
    try {
      Promise.resolve(held.fn(event)).catch(threw);
    } catch (err) {
      threw(err);
    }
  },
  serve(m: { id: number; name: string; args: any; caller: any }) {
    const f = connection.provided.get(m.name);
    const reply = (r: { ok?: any; error?: any }) => connection.post({ t: "reply", id: m.id, ...r });
    if (!f) return reply({ error: { kind: "not_found", message: `${connection.init?.pkg} does not provide ${m.name}` } });
    let result: any;
    try {
      const args = Array.isArray(m.args) ? m.args : m.args == null ? [] : [m.args];
      result = serving.run(servingOf(m.caller), () => f(...args));
    } catch (err) {
      return reply({ error: fail(err) });
    }
    Promise.resolve(result).then(
      (ok) => reply({ ok: ok ?? null }),
      (err) => reply({ error: fail(err) }),
    );
  },
};
