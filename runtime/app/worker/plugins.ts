// The host's plugins: each one's worker (or process), the frames between it and the app, the
// crash rule and the task budget.
//
// The crash rule: a plugin that crashes (throws on start, its worker dies, a task runs past the
// budget) restarts after a backoff, at most `backoff.length` times in a row; then it is disabled
// until the app loads it again, and its status keeps the last error. A run that lasted `stable`
// starts the count again.
/** A frame between the app and the host: `{type, …}`. */
export type Frame = { type: string; [field: string]: any };
import { startProcess, startWorker, type Runner } from "./runner.ts";

export interface Limits {
  /** A task (a callback, a served call, a component's input) is logged past `warn` ms… */
  warn: number;
  /** …and stopped past `kill` ms, which crashes its plugin. */
  kill: number;
  /** The waits before each restart; there are as many restarts as waits. */
  backoff: number[];
  /** A run this long (ms) resets the restart count. */
  stable: number;
}

export const LIMITS: Limits = { warn: 100, kill: 5000, backoff: [250, 500, 1000], stable: 5 * 60_000 };

/** The app's `load` frame. */
export interface Load {
  /** The plugin's id (`owner/name`); every frame carries it as `pkg`. */
  pkg: string;
  dir: string;
  main?: string | null;
  api: number;
  prefix: string;
  /** Its own directories (`paths.config`, `state`, `cache`), on this machine. */
  paths?: { config: string; state: string; cache: string };
  isolate?: boolean;
  /** Run it in a process of its own with Bun's inspector (`pumi plugin dev --inspect`). */
  inspect?: boolean;
}

type State = "starting" | "running" | "crashed" | "disabled" | "stopped";

class Plugin {
  state: State = "starting";
  runner?: Runner;
  generation = 0;
  restarts = 0;
  error: string | null = null;
  /** What waits for it to be ready: messages and what each is, for the budget. */
  queue: [any, string][] = [];
  /** The app's serve frames it has not answered. */
  serves = new Set<number>();
  startedAt = 0;
  retry?: ReturnType<typeof setTimeout>;
  constructor(public spec: Load) {}
}

interface Task {
  pkg: Plugin;
  what: string;
  at: number;
  warned: boolean;
}

export class Host {
  private plugins = new Map<string, Plugin>();
  /** A plugin's requests to the app (call, provide), by frame id. */
  private calls = new Map<number, { pkg: Plugin; generation: number; id: number }>();
  private tasks = new Map<number, Task>();
  private nextFrame = 0;
  private nextTask = 0;
  private watchdog?: ReturnType<typeof setInterval>;

  constructor(
    private send: (f: Frame) => void,
    private limits: Limits = LIMITS,
  ) {}

  /** A frame from the app. */
  frame(f: Frame) {
    switch (f.type) {
      case "load":
        return this.load({
          pkg: f.pkg,
          dir: f.dir,
          main: f.main,
          api: f.api,
          prefix: f.prefix,
          paths: f.paths,
          isolate: !!f.isolate,
          inspect: !!f.inspect,
        });
      case "reload": {
        const p = this.plugins.get(f.pkg);
        if (p) this.restart(p);
        return;
      }
      case "stop": {
        const p = this.plugins.get(f.pkg);
        if (!p) return;
        this.halt(p, "it was stopped");
        this.plugins.delete(f.pkg);
        p.state = "stopped";
        return this.status(p);
      }
      case "reply":
        return this.replied(f);
      case "callback": {
        const p = this.plugins.get(f.pkg);
        if (p?.state === "running") this.task(p, { t: "callback", fn: f.fn, args: f.args }, "a callback");
        return;
      }
      case "event": {
        const p = this.plugins.get(f.pkg);
        if (p?.state === "running") this.task(p, { t: "event", subscription: f.subscription, event: f.event }, `an event (${f.event?.event})`);
        return;
      }
      case "serve":
        return this.serve(f);
    }
  }

  /** Every plugin ends: the app went. */
  shutdown() {
    for (const p of this.plugins.values()) this.halt(p, "the app went");
    this.plugins.clear();
  }

  private load(spec: Load) {
    const known = this.plugins.get(spec.pkg);
    if (known && JSON.stringify(known.spec) === JSON.stringify(spec) && known.state !== "disabled") {
      return this.status(known);
    }
    const p = known ?? new Plugin(spec);
    p.spec = spec;
    this.plugins.set(spec.pkg, p);
    this.restart(p);
  }

  private restart(p: Plugin) {
    this.halt(p, "it was reloaded");
    p.restarts = 0;
    p.error = null;
    this.spawn(p);
  }

  private spawn(p: Plugin) {
    const generation = ++p.generation;
    p.state = "starting";
    p.startedAt = performance.now();
    this.status(p);
    const on = {
      message: (m: any) => this.message(p, generation, m),
      exit: (why: string | null) => {
        if (why !== null && generation === p.generation) this.crash(p, why);
      },
    };
    p.runner =
      p.spec.isolate || p.spec.inspect ? startProcess(on, { inspect: !!p.spec.inspect }) : startWorker(on);
    const { pkg, prefix, dir, main, paths, api } = p.spec;
    this.task(p, { t: "init", pkg, prefix, dir, main, paths, api }, "starting");
  }

  /** Ends `p`'s run: what waited on it is answered, and nothing of it is heard again. */
  private halt(p: Plugin, why: string) {
    clearTimeout(p.retry);
    p.generation++;
    p.runner?.stop();
    p.runner = undefined;
    for (const [id, t] of this.tasks) if (t.pkg === p) this.tasks.delete(id);
    for (const [id, c] of this.calls) if (c.pkg === p) this.calls.delete(id);
    for (const id of p.serves) {
      this.send({ type: "reply", id, error: { kind: "provider_unavailable", message: `${p.spec.pkg}: ${why}` } });
    }
    p.serves.clear();
    p.queue = [];
  }

  private crash(p: Plugin, why: string) {
    this.halt(p, why);
    this.log(p, "stderr", `pumi: ${why}`);
    p.error = why;
    if (performance.now() - p.startedAt > this.limits.stable) p.restarts = 0;
    const wait = this.limits.backoff[p.restarts];
    if (wait === undefined) {
      p.state = "disabled";
      return this.status(p);
    }
    p.restarts++;
    p.state = "crashed";
    this.status(p);
    p.retry = setTimeout(() => this.spawn(p), wait);
  }

  private status(p: Plugin) {
    this.send({ type: "status", pkg: p.spec.pkg, state: p.state, error: p.error, restarts: p.restarts });
  }

  private log(p: Plugin | null, stream: string, line: string) {
    this.send({ type: "log", pkg: p?.spec.pkg ?? "", stream, line });
  }

  private serve(f: Frame) {
    const p = this.plugins.get(f.pkg);
    if (!p || p.state === "disabled") {
      const message = p ? `${f.pkg} is disabled: ${p.error}` : `${f.pkg} is not loaded`;
      return this.send({ type: "reply", id: f.id, error: { kind: "provider_unavailable", message } });
    }
    p.serves.add(f.id);
    this.deliver(p, { t: "serve", id: f.id, name: f.name, args: f.args, caller: f.caller }, `serving ${f.name}`);
  }

  private deliver(p: Plugin, m: any, what: string) {
    if (p.state === "running") this.task(p, m, what);
    else p.queue.push([m, what]);
  }

  private task(p: Plugin, m: any, what: string) {
    const task = ++this.nextTask;
    this.tasks.set(task, { pkg: p, what, at: performance.now(), warned: false });
    this.watch();
    p.runner?.post({ ...m, task });
  }

  private watch() {
    if (this.watchdog) return;
    this.watchdog = setInterval(() => {
      const now = performance.now();
      for (const [id, t] of [...this.tasks]) {
        // A crash below ends its plugin's other tasks too.
        if (!this.tasks.has(id)) continue;
        const ran = now - t.at;
        if (ran > this.limits.kill) {
          this.crash(t.pkg, `${t.what} ran past ${this.limits.kill} ms, so it was stopped`);
        } else if (ran > this.limits.warn && !t.warned) {
          t.warned = true;
          this.log(t.pkg, "stderr", `pumi: ${t.what} has run for more than ${this.limits.warn} ms`);
        }
      }
      if (this.tasks.size === 0) {
        clearInterval(this.watchdog);
        this.watchdog = undefined;
      }
    }, Math.max(5, Math.min(25, this.limits.warn / 4)));
  }

  /** A message from `p`'s worker, of run `generation`. */
  private message(p: Plugin, generation: number, m: any) {
    if (generation !== p.generation) return;
    switch (m.t) {
      case "ready": {
        for (const [id, t] of this.tasks) if (t.pkg === p && t.what === "starting") this.tasks.delete(id);
        p.state = "running";
        this.status(p);
        const queue = p.queue;
        p.queue = [];
        for (const [message, what] of queue) this.task(p, message, what);
        return;
      }
      case "failed":
        return this.crash(p, `it threw on start: ${m.error}`);
      case "done":
        this.tasks.delete(m.task);
        return;
      case "call":
      case "provide": {
        const id = ++this.nextFrame;
        this.calls.set(id, { pkg: p, generation, id: m.id });
        const { t, id: _, ...fields } = m;
        return this.send({ type: t, id, pkg: p.spec.pkg, ...fields });
      }
      case "withdraw":
        return this.send({ type: "withdraw", pkg: p.spec.pkg, names: m.names });
      case "reply":
        p.serves.delete(m.id);
        return this.send({ type: "reply", id: m.id, ...(m.error ? { error: m.error } : { ok: m.ok ?? null }) });
      case "log":
        return this.log(p, m.stream, m.line);
      case "ui":
        return this.send({ type: "ui", pkg: p.spec.pkg, topic: m.topic, data: m.data });
      case "raised":
        return this.send({ type: "raised", pkg: p.spec.pkg, error: m.error });
    }
  }

  private replied(f: Frame) {
    const c = this.calls.get(f.id);
    if (!c) return;
    this.calls.delete(f.id);
    if (c.generation !== c.pkg.generation) return;
    c.pkg.runner?.post({ t: "reply", id: c.id, ...(f.error ? { error: f.error } : { ok: f.ok ?? null }) });
  }
}

/** `LIMITS`, with what `PUMI_PLUGIN_LIMITS` (JSON, for tests) changes. */
export function limits(env = process.env.PUMI_PLUGIN_LIMITS): Limits {
  return env ? { ...LIMITS, ...JSON.parse(env) } : LIMITS;
}
