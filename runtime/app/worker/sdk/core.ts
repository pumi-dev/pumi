// What a plugin's `main.ts` imports as `pumi`, in its worker: `workspace` (the models and the
// caller's own objects), `call`, `on` and `emit`, `fn`, `option`, `registry`, `request`, `menu`,
// `plugin`, `fs`, `clipboard`, `process`, the `agent` transport helpers, `self`, `layout`, `stop`,
// `paths` and `ui.post` (what its `ui.tsx` hears with `useChannel`).
//
// It speaks the server's export (api/pumi-api.json), and so does the wire under it.
import { mkdirSync } from "node:fs";
import { FUNCTIONS } from "./api.gen.ts";
import type { EventName, Events, FunctionName, Functions, Layout, Nouns, TabId, ViewId } from "./api.types.gen.ts";
import { cache } from "./model.ts";
import type { Caller, Disposer, Manifest, OptionSpec, Paths } from "./runtime.ts";
import { connection, currentCaller } from "./runtime.ts";
import type { WireEvent, WireOnOptions } from "./transport.ts";
import { connect, wireFill, workspace } from "./workspace.ts";
import { connectionWire } from "./wire.ts";

/** The wire this worker's SDK speaks: the export's names, over its connection. */
const wire = connectionWire();

/** Bun's `process`, under a name of its own: the SDK exports the API's `process.*` noun. */
const runtime = globalThis.process;

/**
 * The caller's own objects: while the plugin serves a call, its caller's tab and where it is;
 * else its session and tab (`pumi do --session --tab`, else its environment's).
 */
function caller() {
  const asked = currentCaller()?.tab;
  if (asked) {
    const tab = cache.get("tab", asked) as { session?: string; view?: string | null } | undefined;
    return { session: tab?.session ?? null, view: tab?.view ?? null, tab: asked };
  }
  const init = connection.init;
  return {
    session: init?.session ?? runtime.env.PUMI_SESSION ?? null,
    view: runtime.env.PUMI_VIEW ?? null,
    tab: init?.tab ?? runtime.env.PUMI_TAB ?? null,
  };
}
connect(wire, wireFill(wire), caller);

/** Calls API function `name` (or one a plugin provides) with its arguments in order. */
export interface Call {
  <N extends FunctionName>(name: N, ...args: Functions[N]["args"]): Promise<Functions[N]["returns"]>;
  (name: string, ...args: unknown[]): Promise<any>;
}

/** Calls `name`: one wire call, with the export's arguments and answer. */
export const call: Call = ((name: string, ...args: unknown[]) => cache.call(name, args)) as Call;

/** One function of each noun, as the export has them: `tab.open(…)` is one wire call. */
function nouns(): Nouns {
  const root: any = {};
  for (const name of FUNCTIONS) {
    const parts = name.split(".");
    if (parts.length < 2) continue;
    let at = root;
    for (const part of parts.slice(0, -1)) at = at[part] ??= {};
    at[parts.at(-1)!] = (...args: unknown[]) => cache.call(name, args);
  }
  return root as Nouns;
}
const raw = nouns();

// Events.

/** An event as a handler gets it: what fired, what it matched, the objects it is about, its data. */
export interface PumiEvent<D = any> {
  /** The event's name (`tab.closed`, `pumi_dev.queue.done`). */
  event: string;
  /** What a `match` is tested against: per event, as the export says (an object's id, …). */
  match: string;
  /** The session it is about, when it is about one. */
  session?: string | null;
  view?: string | null;
  tab?: string | null;
  data: D;
  /** The subscription's handle. */
  id: number;
  seq?: number;
}
export interface OnOptions {
  /** Globs tested against the event's `match`: every one without. */
  match?: string | string[];
  /** Runs once, then it is gone. */
  once?: boolean;
  desc?: string;
}
type DataOf<E> = E extends EventName ? Events[E] : any;
export type EventHandler<E> = (e: PumiEvent<DataOf<E>>) => unknown;

/** What a subscription is, once the server has it. */
function subscribe(events: string[], opts: OnOptions, fn: (e: PumiEvent) => void): Disposer {
  let handle = 0;
  const onWire = (e: WireEvent) => fn({ ...e, id: handle } as PumiEvent);
  const ready = wire.on(events, opts as WireOnOptions, onWire).then((id) => {
    handle = id;
  });
  const dispose = (() => {
    wire.off(handle).catch(() => {});
  }) as Disposer;
  dispose.ready = ready.then(() => undefined);
  dispose.ready.catch(() => {});
  dispose[Symbol.dispose] = dispose;
  return dispose;
}

/**
 * Runs `fn` with each `event` (the export's, or a plugin's own, `owner.name.done`) that `opts`
 * matches. Returns its disposer.
 */
export function on<E extends EventName | (string & {})>(event: E | E[], fn: EventHandler<E>): Disposer;
export function on<E extends EventName | (string & {})>(event: E | E[], opts: OnOptions, fn: EventHandler<E>): Disposer;
export function on(event: string | string[], a: OnOptions | EventHandler<string>, b?: EventHandler<string>): Disposer {
  const [opts, fn] = typeof a === "function" ? [{} as OnOptions, a] : [a ?? {}, b!];
  return subscribe([event].flat(), opts, fn as (e: PumiEvent) => void);
}

export interface EmitOptions {
  /** What its listeners' `match` is tested against; the event's own without. */
  match?: string;
  /** The tab it is about. */
  tab?: TabId;
}
/** Fires the plugin's own event `name` (`done` is `owner.name.done`) with `data`. */
export function emit(name: string, data?: unknown, opts: EmitOptions = {}): Promise<null> {
  return cache.call("event.emit", [self.plugin.prefixed(name), { data, match: opts.match, tab: opts.tab }]);
}

/** The API's `event.*`, with `event.on` as `on`. */
export const event = { ...raw.event, on };

// Functions others call.

/** A function's spec, as `fn.set` takes it: only `desc` need say anything. */
export interface FunctionSpec {
  desc?: string;
  effect?: "read" | "write";
  parameters?: [string, string][];
  params_schema?: unknown;
  returns?: string;
  returns_schema?: unknown;
  [field: string]: unknown;
}
/** What `fn.set` takes: a name, its spec, and what runs it. */
export interface FnSpec extends FunctionSpec {
  name: string;
}

export const fn = {
  ...raw.fn,
  /**
   * Sets function `name` (`greet`, `queue.toggle`), under the plugin's prefix, run by `impl` with
   * the call's arguments in order; any caller (another plugin, a script, `pumi call`) reaches it.
   * A manifest command's name reaches it too. `null` withdraws it. Returns its disposer.
   */
  set(name: string, spec: FunctionSpec | null, impl?: (...args: any[]) => unknown): Disposer {
    const full = self.plugin.prefixed(name);
    if (spec === null || !impl) {
      // Withdrawn now: nothing left to undo.
      connection.post({ t: "withdraw", names: [full] });
      return connection.disposer(() => {}, Promise.resolve());
    }
    // The app provides it for the plugin, and serves each call in this worker (`serve`).
    connection.provided.set(full, impl);
    const ready = connection.request({ t: "provide", name: full, spec });
    ready.catch(() => {
      if (connection.provided.get(full) === impl) connection.provided.delete(full);
    });
    return connection.disposer(() => {
      if (connection.provided.get(full) !== impl) return;
      connection.provided.delete(full);
      connection.post({ t: "withdraw", names: [full] });
    }, ready);
  },
};

// Options.

/** The plugin's own option `name` by its full name, when the manifest declares it. */
function own(name: string): { full: string; spec: OptionSpec } | undefined {
  const options = connection.manifest.contributes?.options ?? {};
  const prefix = connection.init?.prefix ?? "";
  const key = name.startsWith(prefix) ? name.slice(prefix.length) : name;
  const spec = options[key];
  return spec ? { full: `${prefix}${key}`, spec } : undefined;
}

type OptionOpts = Functions["option.get"]["args"][1];

/**
 * Options: the plugin's own by their key (`side_by_side`) in the plugin scope, everything else as
 * given. A `secret` option the manifest declares is kept in the keychain by the server.
 */
export const option = {
  ...raw.option,
  get(name: string, opts?: OptionOpts): Promise<any> {
    const mine = own(name);
    return cache.call("option.get", [mine?.full ?? name, { scope: "plugin", plugin: self.plugin.id, ...opts }]);
  },
  set(name: string, value: unknown, opts?: Functions["option.set"]["args"][2]): Promise<null> {
    const mine = own(name);
    return cache.call("option.set", [mine?.full ?? name, value ?? null, { scope: "plugin", plugin: self.plugin.id, ...opts }]);
  },
};

// What registers something returns its disposer.

export const menu = {
  ...raw.menu,
  /**
   * Sets menu item `path`, running `rhs` (a function, or `{call, args}`) when it is chosen;
   * `null` takes it out. Returns its disposer.
   */
  set(path: string, item: Record<string, unknown> | null, impl?: (...args: any[]) => unknown): Disposer {
    const done = cache.call("menu.set", [path, item === null ? null : impl ? { ...item, callback: impl } : item]);
    const dispose = (() => {
      cache.call("menu.set", [path, null]).catch(() => {});
    }) as Disposer;
    dispose.ready = done.then(() => undefined);
    dispose.ready.catch(() => {});
    dispose[Symbol.dispose] = dispose;
    return dispose;
  },
};

// The plugin itself.

/** What the plugin is: its id (`owner/name`), its prefix, its directory and manifest. */
export interface Self {
  readonly plugin: {
    /** `owner/name`. */
    id: string;
    /** `owner.name.`: what its functions, options and events start with. */
    prefix: string;
    dir: string;
    manifest: Manifest;
    /** `name` with the prefix: `bar` is `owner.name.bar`; a name that has it already is kept. */
    prefixed(name: string): string;
  };
  /** Its own session: the caller's while it serves a call, or `pumi do --session`, or the one it was opened in. */
  readonly session: string | null;
  /** Its own tab: the caller's while it serves a call, or `PUMI_TAB`, or the one it was opened in. */
  readonly tab: string | null;
  /**
   * Who called the function it is serving (`fn.set`, a command, an export its `ui.tsx` calls),
   * while it runs: their client and tab. `null` outside one.
   */
  readonly caller: Caller | null;
  /** What it was called with (`pumi do FILE k=v`). */
  readonly args: Record<string, unknown>;
}
export const self: Self = {
  plugin: {
    get id(): string {
      return connection.need().init.pkg;
    },
    get prefix(): string {
      return connection.need().init.prefix;
    },
    get dir(): string {
      return connection.need().init.dir;
    },
    get manifest(): Manifest {
      return connection.manifest;
    },
    prefixed(name: string): string {
      const prefix = self.plugin.prefix;
      return name.startsWith(prefix) ? name : `${prefix}${name}`;
    },
  },
  get session(): string | null {
    return caller().session;
  },
  get tab(): string | null {
    return caller().tab;
  },
  get caller(): Caller | null {
    return currentCaller();
  },
  get args(): Record<string, unknown> {
    return connection.init?.args ?? {};
  },
};

/** The API's `plugin.*`: installing, removing and updating plugins. */
export const plugin = raw.plugin;

// Layouts, composed from `tab.open` and `view.update`.

/** A view's layout: a split's two sides, or a leaf: a pane, and the tabs it holds. */
export type LayoutTree = Layout;

export const layout = {
  /**
   * Opens `block` with `args` in a pane of its own in `view` (the caller's), and answers that
   * pane's leaf, to compose with `split` and `set`. A leaf names a pane: the server keeps its
   * tabs.
   */
  async block(block: string, args: unknown = {}, view?: ViewId): Promise<LayoutTree> {
    const target = view ?? workspace.currentView?.id;
    const tab: TabId = await cache.call("tab.open", [
      block,
      args,
      { kind: "tiled", split: "right", ...(target ? { view: target } : {}) },
    ]);
    const placed = await cache.call("tab.get", [tab]);
    return { pane: placed.place.pane } as LayoutTree;
  },
  /** Splits a view's layout into two, `ratio` the first's share, and sets it. */
  async split(ratio: number, first: LayoutTree, second: LayoutTree, view?: ViewId): Promise<LayoutTree> {
    const tree = { axis: "row", ratio, first, second } as LayoutTree;
    const id = view ?? workspace.currentView?.id;
    await cache.call("view.update", [id, { layout: tree }]);
    return tree;
  },
  /** Sets `view`'s whole layout (the caller's view without one). */
  async set(tree: LayoutTree, view?: ViewId): Promise<null> {
    return cache.call("view.update", [view ?? workspace.currentView?.id, { layout: tree }]);
  },
};

/** Ends a `pumi do` script with `code`, dropping what it registered. */
export function stop(code = 0): void {
  runtime.exitCode = code;
  for (const d of [...connection.live]) d();
}

// The plugin's window side.

/**
 * What the plugin's `main.ts` tells its own `ui.tsx`, in process: its components hear it with
 * `useChannel(topic, …)` at once, without the server between them (token by token, if need be).
 */
export const ui = {
  /** Sends `data` (anything structured-cloneable) to the plugin's components listening on `topic`. */
  post(topic: string, data?: unknown): void {
    connection.post({ t: "ui", topic, data });
  },
};

// Agents.

type AgentTakeArgs = Functions["agent.take"]["args"];
type QueuedItem = NonNullable<Functions["agent.take"]["returns"]>;
/** What `agent.onSend` is asked: the turn (`agent.sending`'s data). */
export type AgentSending = Events["agent.sending"];
/** What `agent.onTurn` is told (`agent.turned`'s data). */
export type AgentTurned = Events["agent.turned"];
/** `agent.take`'s options in a plugin: `wait` waits here, and `signal` stops the wait. */
export type AgentTakeOptions = AgentTakeArgs[1] & { signal?: AbortSignal };

export const agent = {
  ...raw.agent,
  /**
   * Says what an agent should know when it is sent something: `handler` gets each turn
   * (`agent.sending`) and returns the text to hand it unseen, or `null` for nothing. Its answer
   * goes back with `agent.contribute` before the deadline (a second); a throw answers `null`, and
   * the error and a late answer go to the plugin's log. Returns its disposer.
   */
  onSend(handler: (turn: AgentSending) => string | null | undefined | Promise<string | null | undefined>): Disposer {
    return on("agent.sending", async (e) => {
      const turn = e.data;
      let text: string | null = null;
      try {
        text = (await handler(turn)) ?? null;
      } catch (err: any) {
        console.error(`pumi: agent.onSend for ${turn.tab} threw: ${err?.stack ?? err}`);
      }
      try {
        const counted = await cache.call("agent.contribute", [turn.id, text]);
        if (!counted) console.error(`pumi: agent.onSend answered ${turn.tab}'s ${turn.event} after its deadline: the agent was not told it`);
      } catch (err: any) {
        console.error(`pumi: agent.contribute: ${err?.message ?? err}`);
      }
    });
  },
  /** Runs `handler` when an agent's turn ends or it compacts its context (`agent.turned`). */
  onTurn(handler: (turn: AgentTurned) => unknown): Disposer {
    return on("agent.turned", (e) => handler(e.data));
  },
  /**
   * The oldest message waiting for the agent in `tab`'s `session`, taken out of the queue, or
   * `null` (`agent.take`). With `wait` it waits for one here, on the tab's changes, rather than in
   * one call: until an item comes, the tab closes or its session changes (`null`), or `signal`
   * aborts.
   */
  async take(tab: TabId, opts: AgentTakeOptions): Promise<QueuedItem | null> {
    const { signal, wait, session } = opts;
    if (!wait) return cache.call("agent.take", [tab, { session }]);
    let wake: (why: "again" | "gone") => void = () => {};
    const about = (e: PumiEvent) => e.tab === tab || e.data?.tab === tab;
    // Its queue growing, or its agent's session changing, is a change to the tab.
    const queueChanged = (e: PumiEvent) => {
      if (!about(e)) return;
      const keys: string[] = e.data?.keys ?? [];
      if (!keys.some((k) => k === "traits.agent" || k === "agentQueue" || k === "options.@agent.session")) return;
      const moved = keys.some((k) => k.includes("@agent.session")) && e.data?.patch?.["options.@agent.session"] !== session;
      wake(moved ? "gone" : "again");
    };
    // A tab's own options are its changes (`tab.changed`), not the global `option.changed`'s.
    const offs = [
      on("tab.changed", queueChanged),
      on("tab.closed", (e) => {
        if (about(e)) wake("gone");
      }),
    ];
    const aborted = new Promise<"gone">((resolve) => {
      if (signal?.aborted) resolve("gone");
      signal?.addEventListener("abort", () => resolve("gone"), { once: true });
    });
    try {
      await Promise.all(offs.map((off) => off.ready));
      for (;;) {
        const woken = new Promise<"again" | "gone">((resolve) => (wake = resolve));
        if (signal?.aborted) return null;
        const item = await cache.call("agent.take", [tab, { session }]);
        if (item) return item;
        if ((await Promise.race([woken, aborted])) === "gone") return null;
      }
    } finally {
      offs.forEach((off) => off());
    }
  },
};

// The rest of the nouns, as the export has them.

export const { clipboard, fs, process, registry, request } = raw;

function made(kind: keyof Paths): string {
  const dir = connection.need().init.paths?.[kind];
  if (!dir) throw new Error(`pumi: no ${kind} directory was given to ${connection.init?.pkg}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

/** The plugin's own directories on the primary app's machine, made when first asked for. */
export const paths = {
  /** What the user may edit (`<config>/plugins/<owner.name>`). */
  get config(): string {
    return made("config");
  },
  /** What it keeps between runs. */
  get state(): string {
    return made("state");
  },
  /** What it can make again. */
  get cache(): string {
    return made("cache");
  },
};

export { workspace };
