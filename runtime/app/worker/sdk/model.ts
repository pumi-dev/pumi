// The live cache the model classes read, and the base class they share. A model's fields are
// getters over one cache and each of its methods is one wire call. The cache fills a model with
// `<kind>.get` the first time a field is read and stays current from the objects' own public
// events; `watch(keys, fn)` hears only the keys it names. A field reads `undefined` until its
// model is filled: `await model.ready` fills it.
//
// One cache per process: the window and each worker connect it to their own wire, and read it the
// same way afterwards.
import type { Disposer } from "./runtime.ts";
import type { Wire, WireEvent } from "./transport.ts";

/** A model's fields, as the cache keeps them: the export's model data, by field name. */
export type Fields = Record<string, unknown>;

/** A model's data, as it is cached: its id and its fields. */
export type Data = { id: string } & Fields;

/** How the cache asks for a model it does not have yet: none when it comes back. */
export type Fill = (kind: string, id: string) => Promise<Fields | null>;

/** A model's class, as the generated models hand it to the cache. */
export type ModelClass = new (id: string) => Model;

/** One model's keys watched: `keys` of none means every change. */
interface Watcher {
  kind: string;
  id: string;
  keys: ReadonlySet<string> | undefined;
  fn: () => void;
}

/** What the cache hears of an object: the fields that changed, and what they changed to. */
export interface Change {
  kind: string;
  id: string;
  keys: string[];
  patch: Fields;
}

const key = (kind: string, id: string) => `${kind}/${id}`;

class Cache {
  /** The models' fields, by kind and id. */
  private readonly fields = new Map<string, Fields>();
  /** The fills in flight, so a read while filling asks once. */
  private readonly filling = new Map<string, Promise<Fields | null>>();
  private readonly watchers = new Set<Watcher>();
  private readonly listeners = new Set<() => void>();
  /** Each kind's class, as the generated models give them (`installClasses`). */
  private readonly classes = new Map<string, ModelClass>();
  /** Where calls go and where events come from; `connect` sets it. */
  wire?: Wire;
  /** How a model the cache does not have is filled; `connect` sets it. */
  fill?: Fill;
  /** Bumped on every change, for a hook that re-renders on any of them. */
  version = 0;

  /** Makes the cache speak through `wire`, fill with `fill`. */
  connect(wire: Wire, fill: Fill) {
    this.wire = wire;
    this.fill = fill;
  }

  /** Gives the cache each kind's class, which the generated models do as they load. */
  useClasses(classes: Record<string, ModelClass>) {
    for (const [kind, cls] of Object.entries(classes)) this.classes.set(kind, cls);
  }

  /** The fields of `kind`'s model `id`, as the cache has them now; reads what it lacks. */
  get(kind: string, id: string): Fields | undefined {
    const at = key(kind, id);
    const had = this.fields.get(at);
    if (had !== undefined) return had;
    if (this.fill) this.ensure(kind, id).catch(() => {});
    return undefined;
  }

  /** The fields of `kind`'s model `id`, filling them first if need be. */
  async load(kind: string, id: string): Promise<Fields | null> {
    const had = this.get(kind, id);
    if (had !== undefined) return had;
    return this.ensure(kind, id);
  }

  /** Fills `kind`'s model `id` once, and answers what it holds. */
  ensure(kind: string, id: string): Promise<Fields | null> {
    const at = key(kind, id);
    const had = this.fields.get(at);
    if (had !== undefined) return Promise.resolve(had);
    let inflight = this.filling.get(at);
    if (!inflight) {
      if (!this.fill) return Promise.resolve(null);
      inflight = this.fill(kind, id)
        .then((fields) => {
          this.filling.delete(at);
          if (!fields) return null;
          const merged = { ...(this.fields.get(at) ?? {}), ...fields, id };
          this.fields.set(at, merged);
          this.notify({ kind, id, keys: Object.keys(fields), patch: fields });
          return merged;
        })
        .catch((err) => {
          this.filling.delete(at);
          throw err;
        });
      this.filling.set(at, inflight);
    }
    return inflight;
  }

  /** Sets `kind`'s model `id`'s fields, or the ones given, as `fields` says. */
  prime(kind: string, id: string, fields: Fields) {
    const at = key(kind, id);
    this.fields.set(at, { ...(this.fields.get(at) ?? {}), ...fields, id });
    this.notify({ kind, id, keys: Object.keys(fields), patch: fields });
  }

  /** Forgets `kind`'s model `id`: it closed. */
  drop(kind: string, id: string) {
    const at = key(kind, id);
    if (!this.fields.delete(at)) return;
    this.filling.delete(at);
    this.version++;
    for (const listener of [...this.listeners]) listener();
    for (const w of [...this.watchers]) if (w.kind === kind && w.id === id) w.fn();
  }

  /** `change` happened: merges its patch, filling what it did not carry, and tells who watches. */
  changed(change: Change) {
    const at = key(change.kind, change.id);
    const patch: Fields = {};
    const unknown: string[] = [];
    for (const name of change.keys) {
      if (name in change.patch) patch[name] = change.patch[name];
      else unknown.push(name);
    }
    if (Object.keys(patch).length > 0) {
      this.fields.set(at, { ...(this.fields.get(at) ?? {}), ...patch, id: change.id });
    }
    if (unknown.length > 0 && this.fill) {
      // The event says what changed, not what to: ask the server for the model again.
      const at2 = key(change.kind, change.id);
      this.fields.delete(at2);
      this.ensure(change.kind, change.id).catch(() => {});
    }
    this.notify({ kind: change.kind, id: change.id, keys: change.keys, patch });
  }

  /** The models of `kind` through their class, made once per id: `a === b` for the same id. */
  model<K extends Model = Model>(kind: string, id: string | null | undefined): K | null {
    if (!id) return null;
    const at = key(kind, id);
    let made = this.made.get(at);
    if (!made) {
      const cls = this.classes.get(kind);
      if (!cls) return null;
      made = new cls(id);
      this.made.set(at, made);
    }
    return made as K;
  }

  /** The models of `kind` for `ids`, in order. */
  models<K extends Model = Model>(kind: string, ids: readonly string[] | null | undefined): K[] {
    return (ids ?? []).map((id) => this.model<K>(kind, id)).filter((m): m is K => m !== null);
  }
  private readonly made = new Map<string, Model>();

  /** Hears `keys` of `kind`'s model `id` (every change without) until the disposer is called. */
  watch(kind: string, id: string, keys: readonly string[] | undefined, fn: () => void): Disposer {
    const watcher: Watcher = { kind, id, keys: keys && new Set(keys), fn };
    this.watchers.add(watcher);
    const off = (() => {
      this.watchers.delete(watcher);
    }) as Disposer;
    off.ready = Promise.resolve();
    off[Symbol.dispose] = off;
    return off;
  }

  /** Runs `fn` on every change to a model, for a hook that re-renders on any of them. */
  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  /** Calls `name` through the wire, as a model's method does. */
  call(name: string, args: unknown[]): Promise<any> {
    if (!this.wire) return Promise.reject(new Error("pumi: the SDK is not connected"));
    return this.wire.call(name, args);
  }

  /** Hears what the wire says of the models, and keeps the cache current. */
  follow(): Promise<Disposer[]> {
    if (!this.wire) return Promise.resolve([]);
    const wire = this.wire;
    const events = [...this.classes.keys()].flatMap((kind) => [`${kind}.opened`, `${kind}.closed`, `${kind}.changed`]);
    const disposes: Disposer[] = [];
    const one = (event: string, fn: (e: WireEvent) => void) => {
      const refs: Disposer[] = [];
      const handle = wire.on([event], {}, (e) => fn(e));
      const off = (() => {
        refs.forEach((r) => r());
        handle.then((id) => id >= 0 && wire.off(id)).catch(() => {});
      }) as Disposer;
      off.ready = handle.then(() => undefined);
      off[Symbol.dispose] = off;
      disposes.push(off);
      return off;
    };
    for (const event of events) {
      const [kind, what] = event.split(".") as [string, "opened" | "closed" | "changed"];
      one(event, (e) => this.heard(kind, what, e));
    }
    return Promise.all(disposes.map((d) => d.ready)).then(() => disposes);
  }

  /** What one of the models' events says, applied. */
  private heard(kind: string, what: "opened" | "closed" | "changed", e: WireEvent) {
    const data = (e.data ?? {}) as Fields;
    const id = typeof data[kind] === "string" ? (data[kind] as string) : undefined;
    if (!id) return;
    if (what === "closed") return this.drop(kind, id);
    if (what === "changed") {
      const keys = Array.isArray(data.keys) ? (data.keys as string[]) : [];
      const patch = (data.patch ?? {}) as Fields;
      return this.changed({ kind, id, keys, patch });
    }
    // Opened: it has fields the event may not carry, so ask for the model again.
    const { restored: _restored, ...fields } = data;
    this.fields.set(key(kind, id), { ...(this.fields.get(key(kind, id)) ?? {}), ...fields, id });
    if (this.fill) {
      this.fields.delete(key(kind, id));
      this.ensure(kind, id).catch(() => {});
    } else {
      this.notify({ kind, id, keys: Object.keys(fields), patch: fields });
    }
  }

  private notify(change: Change) {
    this.version++;
    for (const listener of [...this.listeners]) listener();
    const keys = new Set(change.keys);
    for (const w of [...this.watchers]) {
      if (w.kind !== change.kind || w.id !== change.id) continue;
      if (w.keys && ![...keys].some((k) => w.keys!.has(k))) continue;
      w.fn();
    }
  }
}

/** The one cache: the window's and every worker's. */
export const cache = new Cache();

/** Gives the cache each kind's class, which the generated models do as they load. */
export function installClasses(classes: Record<string, ModelClass>) {
  cache.useClasses(classes);
}

/**
 * A model of the export: `Session`, `View`, `Tab`, `Request` or `Plugin`. Its id is a prefixed
 * string (`%3`); every field is a getter over the live cache, and `watch` hears the keys it names.
 */
export abstract class Model<D extends { id: string } = any, K extends string = string> {
  /** Its kind (`tab`), which the generated classes set. */
  static kind = "";
  /**
   * Its kind, kept out of the subclass's names: a model's own fields may well be called `kind`
   * (a request's is), and a field getter must not answer for the cache's.
   */
  readonly #kind: string;

  constructor(readonly id: string) {
    this.#kind = (this.constructor as typeof Model).kind;
  }

  /**
   * It, once the cache holds its fields: until then each reads `undefined`, whatever its type
   * says. A model from `workspace.openTab`, `tabs()` or the caller's own after `workspace.ready`
   * is filled already; one reached through another's field (`session.views`, `pane.tabs`) or by
   * id (`workspace.getTab`) may not be. Rejects when there is no such object (it closed).
   */
  get ready(): Promise<this> {
    return cache.load(this.#kind, this.id).then((fields) => {
      if (!fields) throw new Error(`pumi: no ${this.#kind} ${this.id}`);
      return this;
    });
  }

  /** One field, as the cache has it; `undefined` until it is filled (`await this.ready`). */
  protected field<P extends keyof D & string>(name: P): D[P] {
    return cache.get(this.#kind, this.id)?.[name] as D[P];
  }

  /** Its whole data, as the cache has it. */
  get data(): D | undefined {
    return cache.get(this.#kind, this.id) as D | undefined;
  }

  /** A model of `kind` with `id`, or `null`. */
  protected model<M extends Model = Model>(kind: string, id: string | null | undefined): M | null {
    return cache.model<M>(kind, id);
  }

  /** The models of `kind` with `ids`, in order. */
  protected models<M extends Model = Model>(kind: string, ids: readonly string[] | null | undefined): M[] {
    return cache.models<M>(kind, ids);
  }

  /** One wire call, as each method makes exactly one. */
  protected invoke(name: string, args: unknown[]): Promise<any> {
    return cache.call(name, args);
  }

  /** Runs `fn` when one of `keys` changes (every one without keys), until the disposer is called. */
  watch(keys: readonly K[] = [], fn: () => void): Disposer {
    return cache.watch(this.#kind, this.id, keys, fn);
  }

  /** It crosses calls as its id. */
  toJSON(): string {
    return this.id;
  }

  valueOf(): string {
    return this.id;
  }
}
