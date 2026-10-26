// Generated from api/pumi-api.json by `bun runtime/app/gen/index.ts` (`cargo xtask api` runs it); do not edit.

import { Model, installClasses } from "./model.ts";
import type { Layout, Neighbors, Notification, ObjectId, PaneData, PaneId, PaneKey, PluginData, PluginId, PluginKey, Process, QueuedMessage, RequestData, RequestId, RequestKey, Scroll, SessionData, SessionId, SessionKey, TabData, TabId, TabKey, TabPlace, TabTraits, ViewData, ViewId, ViewKey } from "./api.types.gen.ts";

/** A session containing views. Its fields read the live cache; each method is one call. */
export class Session extends Model<SessionData, SessionKey> {
  static readonly kind = "session";
  /** Its name. */
  get name(): string {
    return this.field("name") as string;
  }
  /** Its directory. */
  get cwd(): string | null {
    return this.field("cwd") as string | null;
  }
  /** Its views in order. */
  get views(): View[] {
    return this.models("view", this.field("views")) as View[];
  }
  /** Its current view. */
  get activeView(): View | null {
    return this.model("view", this.field("activeView")) as View | null;
  }
  /**
   * Change a session's name or directory. Fires `session.changed` with what changed. No id is the
   * caller's session.
   *
   * @example
   * await session.update({ name: "work" })
   */
  update(patch: {
    /** Its name; `null` gives it a free one of its own (its folder's, else a number). */
    name?: string | null;
    /** Its directory, where its terminals start without a `cwd` of their own; `null` has none. */
    cwd?: string | null;
  }): Promise<null> {
    return this.invoke("session.update", [this.id, patch]);
  }
  /**
   * Show a session in the caller's client; or move from it to another by offset, or to the last
   * one (`-n`, `-p`, `-l`). No id is the caller's session.
   *
   * @example
   * await session.focus()
   */
  focus(opts?: {
    /** To so many sessions on (or back), in the order they were made, wrapping. */
    offset?: number | null;
    /** To the session used most recently before it. */
    last?: boolean;
  } | null): Promise<null> {
    return this.invoke("session.focus", [this.id, opts]);
  }
  /**
   * Close a session and its views and tabs. No id is the caller's.
   *
   * @example
   * await session.close()
   */
  close(): Promise<null> {
    return this.invoke("session.close", [this.id]);
  }
}

/** A view: a layout of panes, each holding tabs. Its fields read the live cache; each method is one call. */
export class View extends Model<ViewData, ViewKey> {
  static readonly kind = "view";
  /** Its session. */
  get session(): Session {
    return this.model("session", this.field("session")) as Session;
  }
  /** Its given name. */
  get name(): string | null {
    return this.field("name") as string | null;
  }
  /** Its index in the session. */
  get index(): number {
    return this.field("index") as number;
  }
  /** Its panes as they split, each with its tabs and the one in front. */
  get layout(): Layout {
    return this.field("layout") as Layout;
  }
  /** Its active tab: the one with the focus, in front of its pane. */
  get activeTab(): Tab | null {
    return this.model("tab", this.field("activeTab")) as Tab | null;
  }
  /** Its pane with the focus. */
  get activePane(): Pane | null {
    return this.model("pane", this.field("activePane")) as Pane | null;
  }
  /** Its zoomed pane. */
  get zoom(): Pane | null {
    return this.model("pane", this.field("zoom")) as Pane | null;
  }
  /**
   * Change a view: its name, index or session, layout, and zoom. Each change fires `view.changed`
   * with what changed. No id is the caller's view.
   *
   * @example
   * await view.update({ name: "tests", zoom: ":3" })
   */
  update(patch: {
    /** Its name; `null` gives it back its automatic name (its active tab's title). */
    name?: string | null;
    /** Its index in its session, which must be free. */
    index?: number | null;
    /** The session it moves to, at `index` or its first free one. */
    session?: SessionId | null;
    layout?: "even-horizontal" | "even-vertical" | "main-horizontal" | "main-horizontal-mirrored" | "main-vertical" | "main-vertical-mirrored" | "tiled" | {
      offset: number;
    } | {
      pane: PaneId;
      /** Its tabs, in its strip's order. */
      tabs?: TabId[];
      /** The tab in front. */
      active?: TabId | null;
    } | {
      /** A `row` lays its halves out left to right, a `column` top to bottom. */
      axis: "row" | "column";
      /** The first half's share, between 0 and 1. */
      ratio: number;
      first: unknown;
      second: unknown;
    } | null;
    /** The pane zoomed to fill it, which gets its focus; `null` unzooms it. */
    zoom?: PaneId | null;
  }): Promise<null> {
    return this.invoke("view.update", [this.id, patch]);
  }
  /**
   * Make a view its session's active view; or move from it to another of its session by offset
   * (`next-window` is `{"offset": 1}`), by index, or the last one. No id is the caller's view.
   *
   * @example
   * await view.focus()
   */
  focus(opts?: {
    /** To so many views on (or back, negative), wrapping. */
    offset?: number | null;
    /** To the session's view at this index. */
    index?: number | null;
    /** To the view used before the session's active one. */
    last?: boolean;
  } | null): Promise<null> {
    return this.invoke("view.focus", [this.id, opts]);
  }
  /**
   * Close a view and every tab in it. No id is the caller's.
   *
   * @example
   * await view.close()
   */
  close(): Promise<null> {
    return this.invoke("view.close", [this.id]);
  }
}

/** A pane: an area of a view's layout, holding tabs with one in front. Its fields read the live cache; each method is one call. */
export class Pane extends Model<PaneData, PaneKey> {
  static readonly kind = "pane";
  /** Its view. */
  get view(): View {
    return this.model("view", this.field("view")) as View;
  }
  /** Its tabs, in the order its strip shows them. */
  get tabs(): Tab[] {
    return this.models("tab", this.field("tabs")) as Tab[];
  }
  /** The tab in front. */
  get activeTab(): Tab {
    return this.model("tab", this.field("activeTab")) as Tab;
  }
  /** It has its view's focus. */
  get isFocused(): boolean {
    return this.field("isFocused") as boolean;
  }
  /** It fills its view. */
  get isZoomed(): boolean {
    return this.field("isZoomed") as boolean;
  }
  /**
   * Zoom a pane, or move its borders. No id is the caller's tab's pane.
   *
   * @example
   * await pane.update({ zoom: true })
   */
  update(patch: {
    /**
     * Fill its view with it, its tab strip on top (`true`), give the view its layout back
     * (`false`), or flip it (`"toggle"`).
     */
    zoom?: boolean | "toggle" | null;
    /** Grow (or, negative, shrink) it by so many cells from each side given. */
    resize?: {
      left?: number | null;
      right?: number | null;
      up?: number | null;
      down?: number | null;
    } | null;
  }): Promise<null> {
    return this.invoke("pane.update", [this.id, patch]);
  }
  /**
   * Give a pane its view's focus (its front tab becomes the view's active tab), and make its view
   * and session current; or move from it to another: the one beside it (`direction`), so many
   * on (`offset`), or the one used last (`last`). No id is the caller's tab's pane. Moving out of
   * a zoomed pane unzooms the view; nothing there is not an error.
   *
   * @example
   * await pane.focus()
   */
  focus(opts?: {
    /** To the pane beside it on this side. */
    direction?: "left" | "right" | "up" | "down" | null;
    /** To so many panes on (or back, negative), in layout order, wrapping. */
    offset?: number | null;
    /** To the pane of its view's tab used last. */
    last?: boolean;
  } | null): Promise<null> {
    return this.invoke("pane.focus", [this.id, opts]);
  }
  /**
   * Close a pane and every tab in it. No id is the caller's tab's pane.
   *
   * @example
   * await pane.close()
   */
  close(): Promise<null> {
    return this.invoke("pane.close", [this.id]);
  }
}

/** A tab holding one block. Its fields read the live cache; each method is one call. */
export class Tab extends Model<TabData, TabKey> {
  static readonly kind = "tab";
  /** Its session. */
  get session(): Session {
    return this.model("session", this.field("session")) as Session;
  }
  /** Its view. */
  get view(): View | null {
    return this.model("view", this.field("view")) as View | null;
  }
  /** The block it holds. */
  get block(): string {
    return this.field("block") as string;
  }
  /** Its traits and their fields. */
  get traits(): TabTraits {
    return this.field("traits") as TabTraits;
  }
  /** Where it is. */
  get place(): TabPlace {
    return this.field("place") as TabPlace;
  }
  /** Its title. */
  get title(): string | null {
    return this.field("title") as string | null;
  }
  /** Its directory. */
  get cwd(): string | null {
    return this.field("cwd") as string | null;
  }
  /** Its foreground process's directory. */
  get foregroundCwd(): string | null {
    return this.field("foregroundCwd") as string | null;
  }
  /** Its foreground process. */
  get process(): Process | null {
    return this.field("process") as Process | null;
  }
  /** Its scroll position. */
  get scroll(): Scroll {
    return this.field("scroll") as Scroll;
  }
  /** Its neighboring tabs. */
  get neighbors(): Neighbors {
    return this.field("neighbors") as Neighbors;
  }
  /** Whether it has focus. */
  get isFocused(): boolean {
    return this.field("isFocused") as boolean;
  }
  /** Its pane fills its view. */
  get isZoomed(): boolean {
    return this.field("isZoomed") as boolean;
  }
  /** Its options. */
  get options(): {
    [key: string]: unknown;
  } {
    return this.field("options") as {
    [key: string]: unknown;
  };
  }
  /**
   * Change a tab: its title, place, size, zoom, whether it was seen, and its options. Each change
   * fires `tab.changed` (or its view's `view.changed`) with what changed. No id is the caller's tab.
   *
   * @example
   * await tab.update({ title: "tests" })
   */
  update(patch: {
    /** Its title; `null` leaves it to what its program says next. */
    title?: string | null;
    /**
     * Where it goes: into a pane (`{"kind": "tiled", "pane": ":2"}`, or along its own pane's
     * strip), in a pane split from another tab's, into a new view or session, floating, or into
     * another tab's place (`{"kind": "tiled", "swap": "%2"}`).
     */
    place?: {
      /**
       * What a place is: tiled in a view's layout, floating over a view, hidden while it moves, or
       * tiled in a new view or a new session.
       */
      kind: "tiled" | "float" | "hidden" | "view" | "session";
      /** Tiled: into this view, beside its active tab. Float: over this view. */
      view?: ViewId | null;
      /**
       * Tiled: into this pane (`current`: the caller's tab's), as one of its tabs; with `split`,
       * beside it, in a pane of its own. Where a tiled tab is, its pane.
       */
      pane?: PaneId | "current" | null;
      /** View: the session the new view goes in; `new` makes a session for the tab. */
      session?: SessionId | "new" | null;
      /** Tiled: which side of the pane it splits, making a pane for the tab; `down` by default. */
      split?: "left" | "right" | "up" | "down" | null;
      /**
       * View: the new view's index. Into a pane: where in its strip, after its front tab by
       * default.
       */
      index?: number | null;
      /** Moving: trade places with this tab. */
      swap?: TabId | null;
      /** Tiled: the new pane's share of what it splits, between 0 and 1. */
      ratio?: number | null;
      /**
       * Tiled: the tab whose pane to split, the caller's by default. Float: the tab
       * `relative: "pane"` is relative to.
       */
      beside?: TabId | null;
      /** Tiled: the new pane's size, `"30%"` or cells, instead of `ratio`. */
      size?: number | string | null;
      /** Tiled: split the whole view, not one pane. */
      full?: boolean | null;
      /** View, session: its name. */
      name?: string | null;
      /** Session: its directory. */
      cwd?: string | null;
      /** Make it the caller's tab; `true` by default. */
      focus?: boolean | null;
      /** Opening: in this tab instead of the block it holds, keeping its id, place and options. */
      replace?: TabId | null;
      /** Float: what it floats over, `view` (the default) or `client`. */
      relative?: string | null;
      /** Float: the corner or edge it is pinned to (`NW` … `SE`, `center`). */
      anchor?: "NW" | "N" | "NE" | "W" | "center" | "E" | "SW" | "S" | "SE" | null;
      /** Float: how far in from its anchor: cells, `"24px"` or `"10%"`. */
      row?: number | string | null;
      col?: number | string | null;
      /** Float: cells, `"480px"` or a share of what it floats over (`"50%"`, the default). */
      width?: number | string | null;
      height?: number | string | null;
      /** Float: `none` draws it bare. */
      border?: string | null;
      /** Float: it takes the keyboard when it opens (`true` by default). */
      focusable?: boolean | null;
      /** Float: it takes the pointer (`focusable` by default). */
      mouse?: boolean | null;
      /** Float: what closes it: `escape`, `key`, `press`, `scroll`. */
      dismiss?: string[] | null;
      /** Float: dims what is under it, 0 to 100. */
      backdrop?: number | null;
      /** Float: above floats with a lower one (50 by default). */
      zIndex?: number | null;
    } | null;
    /** Grow or shrink its pane from a side. */
    size?: {
      /** The side whose border moves. */
      direction: "left" | "right" | "up" | "down";
      /** By how many cells: its neighbour there gives or takes them. */
      amount: number;
    } | null;
    /** Zoom its pane to fill its view (`true`), unzoom its view (`false`), or flip it (`"toggle"`). */
    zoom?: boolean | "toggle" | null;
    /**
     * The user has seen it: what its program left for them (`done`, `error`, OSC 7501) goes, so
     * its `@agent.status` stops asking. The app says so when the tab is in front of the user.
     */
    seen?: boolean | null;
    /** Options to set on it; `null` unsets one. */
    options?: {
      [key: string]: unknown;
    } | null;
  }): Promise<null> {
    return this.invoke("tab.update", [this.id, patch]);
  }
  /**
   * Make a tab its pane's front tab and its view's active tab, and its view and session current;
   * or move from it: to the next tab of its pane (`offset`), to the pane beside it (`direction`),
   * or to its view's tab used last (`last`). No id is the caller's tab. Moving out of a zoomed
   * pane unzooms the view; nothing there is not an error.
   *
   * @example
   * await tab.focus()
   */
  focus(opts?: {
    /** To the tab in front of the pane beside its pane on this side. */
    direction?: "left" | "right" | "up" | "down" | null;
    /** To so many tabs on (or back, negative) in its pane's strip, wrapping. */
    offset?: number | null;
    /** To its view's tab used last. */
    last?: boolean;
  } | null): Promise<null> {
    return this.invoke("tab.focus", [this.id, opts]);
  }
  /**
   * Close a tab. Its state is kept, so it can be reopened (`tab.list({closed: true})`). No id is
   * the caller's.
   *
   * @example
   * await tab.close()
   */
  close(): Promise<null> {
    return this.invoke("tab.close", [this.id]);
  }
  /**
   * Text read from a tab: a terminal's as its screen shows it, one line a row.
   *
   * @example
   * await tab.read({ source: "visible", lines: 80 }) // { text: "ready\n" }
   */
  read(args?: {
    /** `visible`: the screen as it is; `recent` (the default): the scrollback and the screen; `unwrapped`: those, with lines wrapped at the screen's width joined. */
    source?: "visible" | "recent" | "unwrapped";
    /** The last this many lines that hold something: the screen's rows for `visible`, 200 otherwise (`pumi tab read`: 50). */
    lines?: number;
    /** The output as the program wrote it, its escape sequences kept. */
    ansi?: boolean;
  }): Promise<{
    text: string;
  }> {
    return this.invoke("tab.call", [this.id, "read", args ?? {}]);
  }
  /**
   * Text, a paste, or keys sent to a tab.
   *
   * @example
   * await tab.input({ keys: ["ctrl+c"] })
   */
  input(args: {
    text: string;
  } | {
    paste: string;
  } | {
    keys: string[];
  }): Promise<null> {
    return this.invoke("tab.call", [this.id, "input", args]);
  }
  /**
   * A command pasted and entered as one write.
   *
   * @example
   * await tab.run({ command: ["make", "test"] })
   */
  run(args: {
    command: string | string[];
  }): Promise<null> {
    return this.invoke("tab.call", [this.id, "run", args]);
  }
  /**
   * A tab waited on for output or exit.
   *
   * @example
   * await tab.wait({ exit: true, timeout: 30 }) // { match: null, exitCode: 0 }
   */
  wait(args?: {
    match?: string;
    regex?: string;
    exit?: boolean;
    timeout?: number;
  }): Promise<{
    match: string | null;
    exitCode: number | null;
  }> {
    return this.invoke("tab.call", [this.id, "wait", args ?? {}]);
  }
  /**
   * The tab selection.
   *
   * @example
   * await tab.selection({}) // { text: "selected" }
   */
  selection(args?: {}): Promise<{
    text: string;
  }> {
    return this.invoke("tab.call", [this.id, "selection", args ?? {}]);
  }
  /**
   * An OSC 7501 report sent by a tab.
   *
   * @example
   * await tab.report({ body: { kind: "metadata", tokens: 12 } })
   */
  report(args: {
    body: unknown;
  }): Promise<null> {
    return this.invoke("tab.call", [this.id, "report", args]);
  }
  /**
   * A terminal stream attached to a tab.
   *
   * @example
   * await tab.attach({ mode: "observe", cols: 120, rows: 40 }) // { stream: "stream/1" }
   */
  attach(args: {
    mode: "control" | "observe";
    takeover?: boolean;
    cols?: number;
    rows?: number;
  }): Promise<{
    stream: string;
  }> {
    return this.invoke("tab.call", [this.id, "attach", args]);
  }
}

/** A request shown to the user. Its fields read the live cache; each method is one call. */
export class Request extends Model<RequestData, RequestKey> {
  static readonly kind = "request";
  /** Its tab. */
  get tab(): Tab | null {
    return this.model("tab", this.field("tab")) as Tab | null;
  }
  /** Its kind. */
  get kind(): "notification" | "permission" | "question" {
    return this.field("kind") as "notification" | "permission" | "question";
  }
  /** Its title. */
  get title(): string {
    return this.field("title") as string;
  }
  /** Its detail. */
  get detail(): string | null {
    return this.field("detail") as string | null;
  }
  /** Its possible answers. */
  get options(): unknown[] {
    return this.field("options") as unknown[];
  }
  /** The tool a permission is for. */
  get tool(): string | null {
    return this.field("tool") as string | null;
  }
  /** How much it matters: info, warn, error or debug (only kept). */
  get level(): "info" | "warn" | "error" | "debug" | null {
    return this.field("level") as "info" | "warn" | "error" | "debug" | null;
  }
  /** What a notification does in the OS's notification centre. */
  get notification(): Notification | null {
    return this.field("notification") as Notification | null;
  }
  /** Who opened it. */
  get source(): string | null {
    return this.field("source") as string | null;
  }
  /** When it opened. */
  get created(): string {
    return this.field("created") as string;
  }
  /** When it expires. */
  get expires(): string | null {
    return this.field("expires") as string | null;
  }
  /**
   * Close a request: answer it, or dismiss it without `answer`. A notification takes no answer.
   * The first close wins: a request already closed is not found.
   *
   * @example
   * await request.close({ answer: { option: "allow" } })
   */
  close(opts?: {
    /**
     * Its answer: `{"option": id}` picks one of its options, `{"text": …}` answers a question in
     * words (other fields go along). Without, it is closed unanswered.
     */
    answer?: unknown;
  } | null): Promise<null> {
    return this.invoke("request.close", [this.id, opts]);
  }
}

/** An installed plugin. Its fields read the live cache; each method is one call. */
export class Plugin extends Model<PluginData, PluginKey> {
  static readonly kind = "plugin";
  /** Its version. */
  get version(): string {
    return this.field("version") as string;
  }
  /** Where it came from. */
  get source(): string {
    return this.field("source") as string;
  }
  /** Its directory. */
  get path(): string {
    return this.field("path") as string;
  }
  /** Whether it is enabled. */
  get enabled(): boolean {
    return this.field("enabled") as boolean;
  }
  /**
   * Patch a plugin, or refresh it from its source: a linked one is read again where it is, and a
   * git one is fetched and checked out at the ref it follows (or the one `version` gives). What
   * its manifest contributes is read again, and the app starts it again. `enabled` turns it on or
   * off, and it stays installed.
   *
   * @example
   * await plugin.update({ enabled: false })
   */
  update(patch?: {
    /** Turn it on or off; it stays installed. */
    enabled?: boolean;
    /** Follow another git ref from now on (a tag, a branch, a commit). */
    version?: string;
  }): Promise<null> {
    return this.invoke("plugin.update", [this.id, patch]);
  }
  /**
   * Uninstall a plugin. A linked one's directory stays: only the link goes. What it kept stays
   * too, unless `data` says.
   *
   * @example
   * await plugin.remove({})
   */
  remove(opts?: {
    /**
     * Also what it kept: its own `config`, `state` and `cache` directories
     * (`stdpath(…)/plugins/<owner.name>`, its options' store among them), and its `secret`
     * options in the keychain. Without it they stay, for when it is installed again.
     */
    data?: boolean;
  }): Promise<null> {
    return this.invoke("plugin.remove", [this.id, opts]);
  }
}

/** Each model's class, by its kind. */
export const MODEL_CLASSES = { session: Session, view: View, pane: Pane, tab: Tab, request: Request, plugin: Plugin } as const;

installClasses(MODEL_CLASSES);
