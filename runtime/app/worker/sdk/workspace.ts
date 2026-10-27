// `workspace`: the root object the SDK's models and calls hang on, like Airtable's `base`. Its
// current objects are the caller's own (its tab, the view it is in, that view's session); ids are
// prefixed strings everywhere, and a call for an id on another server goes to that server.
import { cache, type Fill } from "./model.ts";
import type { Pane, Plugin, Request, Session, Tab, View } from "./models.gen.ts";
import type { Wire } from "./transport.ts";

/** Who the caller is: the objects a function means when its id is left out. */
export interface CallerIds {
  session?: string | null;
  view?: string | null;
  tab?: string | null;
}

/** Where the caller's own ids come from as they change: a window's focus, a script's environment. */
export type Caller = () => CallerIds;

/** What `tabs` and `requests` filter by, as their `list` functions do. */
export interface TabFilter {
  session?: string;
  view?: string;
  block?: string;
  trait?: string;
}

export interface RequestFilter {
  tab?: string;
  kind?: string;
}

/**
 * The root: the caller's current session, view and tab, every session, and the calls that make
 * more of them. Reads come from the live cache (fill it once with `await workspace.ready`, and a
 * model it does not hold with `await model.ready`); the `open…` calls are one wire call each.
 */
class Workspace {
  private caller: Caller = () => ({});
  private ids: string[] = [];
  private priming?: Promise<void>;
  private started = false;

  /** Says which objects are the caller's own. */
  useCaller(caller: Caller) {
    this.caller = caller;
  }

  /** Resolves once the cache holds the caller's own objects and the session list; a failure asks again next time. */
  get ready(): Promise<void> {
    return (this.priming ??= this.prime().catch((err) => {
      this.priming = undefined;
      throw err;
    }));
  }

  private async prime() {
    if (cache.wire) await this.start();
    const ids: string[] = await cache.call("session.list", []);
    this.ids = ids;
    await Promise.all(ids.map((id) => cache.load("session", id)));
    const own = this.caller();
    const loads: Promise<unknown>[] = [];
    if (own.session) loads.push(cache.load("session", own.session));
    if (own.view) loads.push(cache.load("view", own.view));
    if (own.tab) loads.push(cache.load("tab", own.tab));
    await Promise.all(loads);
  }

  /** Hears the models' events, so the cache stays current, and keeps the session list. */
  private async start() {
    if (this.started) return;
    this.started = true;
    await cache.follow();
    const wire = cache.wire;
    if (!wire) return;
    await wire.on(["session.opened", "session.closed"], {}, (e) => {
      const id = (e.data as { session?: string }).session;
      if (!id) return;
      if (e.event === "session.closed") this.ids = this.ids.filter((known) => known !== id);
      else if (!this.ids.includes(id)) this.ids = [...this.ids, id];
    });
  }

  /** The caller's session. */
  get currentSession(): Session | null {
    this.ready.catch(() => {});
    return cache.model<Session>("session", this.caller().session);
  }

  /** The caller's view. */
  get currentView(): View | null {
    this.ready.catch(() => {});
    return cache.model<View>("view", this.caller().view);
  }

  /** The caller's tab. */
  get currentTab(): Tab | null {
    this.ready.catch(() => {});
    return cache.model<Tab>("tab", this.caller().tab);
  }

  /** Every session, in creation order. */
  get sessions(): Session[] {
    this.ready.catch(() => {});
    return cache.models<Session>("session", this.ids);
  }

  /** A session by its id; its fields read once the cache has it (`await session.ready`). */
  getSession(id: string): Session | null {
    return cache.model<Session>("session", id);
  }

  /** A view by its id; its fields read once the cache has it (`await view.ready`). */
  getView(id: string): View | null {
    return cache.model<View>("view", id);
  }

  /** A pane by its id (`:4`); its fields read once the cache has it (`await pane.ready`). */
  getPane(id: string): Pane | null {
    return cache.model<Pane>("pane", id);
  }

  /** A tab by its id; its fields read once the cache has it (`await tab.ready`). */
  getTab(id: string): Tab | null {
    return cache.model<Tab>("tab", id);
  }

  /** A view's panes (every view's without one), in layout order. */
  async panes(view?: string): Promise<Pane[]> {
    const ids: string[] = await cache.call("pane.list", [view ? { view } : {}]);
    return Promise.all(ids.map((id) => cache.load("pane", id))).then(() => cache.models<Pane>("pane", ids));
  }

  /** A plugin by its id (`owner/name`), from the registry. */
  async getPlugin(id: string): Promise<Plugin | null> {
    const found = await cache.call("registry.list", ["plugin", { name: id }]);
    const one = Array.isArray(found) ? found[0] : found;
    if (!one) return null;
    cache.prime("plugin", id, one);
    return cache.model<Plugin>("plugin", id);
  }

  /** The tabs matching `filter`, in layout order. */
  async tabs(filter: TabFilter = {}): Promise<Tab[]> {
    const ids: string[] = await cache.call("tab.list", [filter]);
    return Promise.all(ids.map((id) => cache.load("tab", id))).then(() => cache.models<Tab>("tab", ids));
  }

  /** Opens a tab with `block` and `args` where `place` says, and answers it. */
  async openTab(block: string, args: unknown = {}, place?: unknown): Promise<Tab> {
    const id: string = await cache.call("tab.open", [block, args, place ?? {}]);
    await cache.load("tab", id);
    return cache.model<Tab>("tab", id)!;
  }

  /** The open requests matching `filter`. */
  async requests(filter: RequestFilter = {}): Promise<Request[]> {
    const ids: string[] = await cache.call("request.list", [filter]);
    return Promise.all(ids.map((id) => cache.load("request", id))).then(() => cache.models<Request>("request", ids));
  }

  /** Asks the user something, and answers the request it opened. */
  async openRequest(request: Record<string, unknown>, opts: Record<string, unknown> = {}): Promise<Request> {
    const id: string = await cache.call("request.open", [request, opts]);
    await cache.load("request", id);
    return cache.model<Request>("request", id)!;
  }
}

/** The root object, one per process. */
export const workspace = new Workspace();

/** Connects the cache to `wire`, filled with `fill`, for the caller `caller` names. */
export function connect(wire: Wire, fill: Fill, caller: Caller = () => ({})) {
  cache.connect(wire, fill);
  workspace.useCaller(caller);
}

/** Fills a model over the wire: `<kind>.get`, the request table's, or the plugin registry's. */
export function wireFill(wire: Wire): Fill {
  return async (kind, id) => {
    switch (kind) {
      case "session":
        return wire.call("session.get", [id]);
      case "view":
        return wire.call("view.get", [id]);
      case "pane":
        return wire.call("pane.get", [id]);
      case "tab":
        return wire.call("tab.get", [id]);
      case "request":
        return wire.call("request.get", [id]);
      case "plugin": {
        const found = await wire.call("registry.list", ["plugin", { name: id }]);
        return (Array.isArray(found) ? found[0] : found) ?? null;
      }
      default:
        return null;
    }
  };
}
