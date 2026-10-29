// Generated from api/pumi-api.json by `bun runtime/app/gen/index.ts` (`cargo xtask api` runs it); do not edit.

// Ids are prefixed strings: `$1` a session, `@2` a view, `%3` a tab, `+7` a request, `owner/name` a
// plugin, and `devbox/%3` one on the server `devbox`.

/** Session's id (`$1`). */
export type SessionId = string;
/** View's id (`@1`). */
export type ViewId = string;
/** Pane's id (`:1`). */
export type PaneId = string;
/** Tab's id (`%1`). */
export type TabId = string;
/** Request's id (`+1`). */
export type RequestId = string;
/** Plugin's id (`owner/name`). */
export type PluginId = string;
/** Any object's id, for an option's scope. */
export type ObjectId = string;

export type Layout = {
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
  first: Layout;
  second: Layout;
};

export type TabPlace = {
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
};

export type Process = {
  pid: number;
  /** Its program, as the system names it. */
  command: string[];
};

export type Scroll = {
  offsetFromBottom: number;
};

export type Neighbors = {
  left?: TabId | null;
  right?: TabId | null;
  up?: TabId | null;
  down?: TabId | null;
};

export type Notification = {
  subtitle?: string | null;
  /** `default`, or a sound's name. */
  sound?: string | null;
  /** A notification opened with the group of one still open replaces it. */
  group?: string | null;
  actions?: ({
    id: string;
    label: string;
  })[];
  /** A reply field's placeholder. */
  reply?: string | null;
  /** A file's path. */
  image?: string | null;
  urgency?: "low" | "normal" | "critical" | null;
  /** Carried back with what the user does with it. */
  data?: unknown;
};

/** A tab's traits, each with its fields. */
export interface TabTraits {
  [trait: string]: unknown;
}

/** What a Session is, as data: a session containing views. */
export interface SessionData {
  /** Its prefixed id. */
  id: SessionId;
  /** Its name. */
  name: string;
  /** Its directory. */
  cwd: string | null;
  /** Its views in order. */
  views: ViewId[];
  /** Its current view. */
  activeView: ViewId | null;
}
/** What a Session can be watched for. */
export type SessionKey = "name" | "cwd" | "views" | "activeView";

/** What a View is, as data: a view: a layout of panes, each holding tabs. */
export interface ViewData {
  /** Its prefixed id. */
  id: ViewId;
  /** Its session. */
  session: SessionId;
  /** Its given name. */
  name: string | null;
  /** Its index in the session. */
  index: number;
  /** Its panes as they split, each with its tabs and the one in front. */
  layout: Layout;
  /** Its active tab: the one with the focus, in front of its pane. */
  activeTab: TabId | null;
  /** Its pane with the focus. */
  activePane: PaneId | null;
  /** Its zoomed pane. */
  zoom: PaneId | null;
}
/** What a View can be watched for. */
export type ViewKey = "session" | "name" | "index" | "layout" | "activeTab" | "activePane" | "zoom";

/** What a Pane is, as data: a pane: an area of a view's layout, holding tabs with one in front. */
export interface PaneData {
  /** Its prefixed id. */
  id: PaneId;
  /** Its view. */
  view: ViewId;
  /** Its tabs, in the order its strip shows them. */
  tabs: TabId[];
  /** The tab in front. */
  activeTab: TabId;
  /** It has its view's focus. */
  isFocused: boolean;
  /** It fills its view. */
  isZoomed: boolean;
}
/** What a Pane can be watched for. */
export type PaneKey = "tabs" | "activeTab";

/** What a Tab is, as data: a tab holding one block. */
export interface TabData {
  /** Its prefixed id. */
  id: TabId;
  /** Its session. */
  session: SessionId;
  /** Its view. */
  view: ViewId | null;
  /** The block it holds. */
  block: string;
  /** Its traits and their fields. */
  traits: TabTraits;
  /** Where it is. */
  place: TabPlace;
  /** Its title. */
  title: string | null;
  /** Its directory. */
  cwd: string | null;
  /** Its foreground process's directory. */
  foregroundCwd: string | null;
  /** Its foreground process. */
  process: Process | null;
  /** Its scroll position. */
  scroll: Scroll;
  /** Its neighboring tabs. */
  neighbors: Neighbors;
  /** Whether it has focus. */
  isFocused: boolean;
  /** Its pane fills its view. */
  isZoomed: boolean;
  /** Its options. */
  options: {
    [key: string]: unknown;
  };
}
/** What a Tab can be watched for. */
export type TabKey = "session" | "view" | "block" | "traits" | "place" | "title" | "cwd" | "foregroundCwd" | "process" | "scroll" | "neighbors" | "isFocused" | "isZoomed" | "options";

/** What a Request is, as data: a request shown to the user. */
export interface RequestData {
  /** Its prefixed id. */
  id: RequestId;
  /** Its tab. */
  tab: TabId | null;
  /** Its kind. */
  kind: "notification" | "permission" | "question";
  /** Its title. */
  title: string;
  /** Its detail. */
  detail: string | null;
  /** Its possible answers. */
  options: unknown[];
  /** The tool a permission is for. */
  tool: string | null;
  /** How much it matters: info, warn, error or debug (only kept). */
  level: "info" | "warn" | "error" | "debug" | null;
  /** What a notification does in the OS's notification centre. */
  notification: Notification | null;
  /** Who opened it. */
  source: string | null;
  /** When it opened. */
  created: string;
  /** When it expires. */
  expires: string | null;
}
/** What a Request can be watched for. */
export type RequestKey = never;

/** What a Plugin is, as data: an installed plugin. */
export interface PluginData {
  /** Its owner/name id. */
  id: PluginId;
  /** Its version. */
  version: string;
  /** Where it came from. */
  source: string;
  /** Its directory. */
  path: string;
  /** Whether it is enabled. */
  enabled: boolean;
}
/** What a Plugin can be watched for. */
export type PluginKey = "version" | "source" | "path" | "enabled";

/** Every API function: its arguments, in order, and what it answers. */
export interface Functions {
  /**
   * Call any function by name, as the same caller. `args` is positional (an array) or named (an
   * object); leaving it out calls the function with no arguments. A list of `{name, args}` is a
   * batch: called in order with nothing else running in between, answering every result; it stops
   * at the first error, which names the call, and what earlier calls did is not undone.
   */
  "call": { args: [name: string | ({
    name: string;
    args?: unknown;
  })[], args?: unknown]; returns: unknown };
  /** The clipboard's text: `null` when it holds none. */
  "clipboard.get": { args: []; returns: string | null };
  /** Put text on the clipboard; `null` clears it. */
  "clipboard.set": { args: [text: string | null]; returns: null };
  /**
   * Fire an event to every channel subscribed: mostly a plugin's own (`owner.name.done`), with
   * which plugins and scripts tell each other things, and the events channels have of their own
   * (the CLI's `plugin.reloadRequested`).
   */
  "event.emit": { args: [event: string, opts?: {
    data?: unknown;
    /** The event's `match`: what its listeners' `match` is tested against. */
    match?: string | null;
    /** The tab it is about; one just closed may be named. */
    tab?: TabId | null;
  } | null]; returns: null };
  /** End a subscription (`event.on`'s handle); whether there was one. */
  "event.off": { args: [subscription: number]; returns: boolean };
  /**
   * Subscribe to events, by name or glob (`tab.*`; `*` is every event but those fired before
   * what they name, in `-ing`). Each one that fires goes to the caller's channel
   * as `{"method": "rpc.event", "params": [subscription, event]}`, until `event.off` or the
   * channel closes. Answers the subscription's integer handle.
   */
  "event.on": { args: [event: string | string[], opts?: {
    /**
     * Globs matched against the event's `match` (a tab's id for `tab.changed`, a request's
     * kind for `request.opened`); every match without one.
     */
    match?: string | string[] | null;
    /** End the subscription after its first event. */
    once?: boolean;
    /** What it is for, as a listing shows it. */
    desc?: string | null;
  } | null]; returns: number };
  /**
   * Wait for an event, by name or glob: the call answers when one that matches fires, with the
   * event (`{event, match, session, view, tab, data}`), or with `null` when `timeout` passes or
   * the caller goes. A wait that times out is not an error.
   */
  "event.wait": { args: [event: string | string[], opts?: {
    /**
     * Globs matched against the event's `match` (a tab's id for `tab.changed`, a request's id
     * for `request.closed`); any match without one.
     */
    match?: string | string[] | null;
    /** Seconds to wait; none: until it fires. */
    timeout?: number | null;
  } | null]; returns: {
    event: string;
    match: string;
    session?: SessionId | null;
    view?: ViewId | null;
    tab?: TabId | null;
    data: unknown;
  } | null };
  /**
   * Provide a function every surface can call (`pumi call git.status`, a script's
   * `pumi.api["git.status"]`): calls to it from any channel come to this one as requests, with
   * the caller in `rpc.caller`, and its answers go back; or, for one with a `callback`, as a
   * `rpc.callback` notification, while the call answers `null`. It appears in `api.getInfo` with
   * the rest. A core function cannot be provided, and nor can one another channel provides as
   * someone else (another plugin, another program): it is theirs until they withdraw it. With
   * `runsOn: "client"` each app provides its own, for its own client. `null` withdraws it; it goes
   * with the channel too.
   */
  "fn.set": { args: [name: string, spec: {
    desc: string;
    /** `read` or `write`; `write` by default. */
    effect?: "read" | "write" | null;
    /** A JSON Schema for its arguments; one that takes any object without it. */
    paramsSchema?: {
      [key: string]: unknown;
    } | null;
    /** A JSON Schema for its answer. */
    returnsSchema?: {
      [key: string]: unknown;
    } | null;
    /**
     * Send each call here as a `rpc.callback` notification, with its arguments in order, and
     * answer `null`: `{"chan": 0, "ref": …}`, a reference a channel holds. Without one the
     * call comes to the channel as a request, and its answer is the call's.
     */
    callback?: unknown;
    /**
     * `client`: each app that provides it answers the calls made for its own client (its
     * clipboard, its updater), and others' go to the first app that does. `server` by default:
     * one channel answers every call.
     */
    runsOn?: "server" | "client" | null;
  } | null]; returns: null };
  /**
   * The entries of a folder on the server's machine: `~` and `~/…` are its home, a relative path
   * is in it, and an empty path is the home itself. Hidden entries are left out unless `hidden`,
   * and `recursive` lists every folder under it too; both sorted by name, case-insensitively.
   */
  "fs.list": { args: [path: string, opts?: {
    /** Hidden entries too (`.` files). */
    hidden?: boolean;
    /** Every folder under it, not only its own entries. */
    recursive?: boolean;
  }]; returns: ({
    /** Its name in its folder. */
    name: string;
    /** Its full path. */
    path: string;
    /** What a path in a listing is. */
    kind: "file" | "directory" | "symlink";
  })[] };
  /**
   * Open a path in its default app, or in the one `with` names; `reveal` shows it in the file
   * manager instead.
   */
  "fs.open": { args: [path: string, opts?: {
    /** Show it in the file manager (the Finder, Explorer) instead of opening it. */
    reveal?: boolean;
    /** The app to open it with: by name on macOS (`Safari`), a program elsewhere (`code`). */
    with?: string;
  }]; returns: null };
  /**
   * Choose files to open, or a folder: a native picker. Answers the path chosen, the paths when
   * it takes more than one, and `null` when it was cancelled.
   */
  "fs.pick": { args: [opts?: {
    /** Files, or a folder (`file`). */
    kind?: "file" | "directory";
    /** Allow choosing more than one file. */
    multiple?: boolean;
    /** What the picker's button says (`Open Local Folder`). */
    title?: string;
    /**
     * Only files these globs fit (`["*.png", "*.jpg"]`), as a renderer's `match` reads them:
     * one without a `/` is matched against the file's name. On macOS the picker greys out the
     * rest when every glob is `*.<ext>`; elsewhere a file none fits is refused once chosen.
     */
    match?: string[];
    /**
     * Where it starts: a file's folder, or a folder. Taken but not followed: the system's
     * picker opens where it last was.
     */
    path?: string;
  }]; returns: string | string[] | null };
  /** Move paths to the Trash. */
  "fs.trash": { args: [paths: string[]]; returns: null };
  /**
   * Set a menu item: `path` names it and every menu above it, which are made as needed, and the
   * item is the function choosing it calls, with its title and when it shows. Setting a path
   * again replaces the item; `null` removes it, and a menu with everything in it.
   */
  "menu.set": { args: [path: string, item: {
    /** The function choosing it calls. */
    fn: string;
    /** Its arguments, positional or by name; none by default. */
    args?: unknown;
    /** What it says in the menu; made of the function's name by default. */
    title?: string;
    /** When it shows; always without it. */
    when?: string;
  } | null]; returns: null };
  /**
   * An option's value in a scope: the object's own, else the one it inherits, else the default.
   * A plugin's is its own store, and a `secret` one is in the OS keychain.
   */
  "option.get": { args: [name: string, opts?: {
    /**
     * `global`, `session`, `view`, `tab` or `plugin`. Without it: the option's own scope, at the
     * object `id` names (the caller's own without that).
     */
    scope?: "global" | "session" | "view" | "tab" | "plugin";
    /** The object: `$1`, `@2` or `%3`. */
    id?: string;
    /** The plugin whose store it is, for `{scope: "plugin"}`: `owner/name`. */
    plugin?: PluginId;
  }]; returns: unknown };
  /**
   * Set an option on an object, in a plugin's store, or globally. `@name` options need no
   * declaration. Setting a type's option is acting on the tab: a browser's `url` navigates.
   * `null` unsets it, so it inherits again. `ttl` makes the value expire, and a report whose `seq`
   * is not past the last one from the same `source` is dropped.
   */
  "option.set": { args: [name: string, value: unknown, opts?: {
    /** Which object an option is read from or set on, by scope. */
    scope?: "global" | "session" | "view" | "tab" | "plugin";
    id?: string;
    plugin?: PluginId;
    /** Seconds the value holds for; then it is the one it inherits again. */
    ttl?: number;
    /** Who set it (`report-agent`'s reporter); the caller's source by default. */
    source?: string;
    /** With `source`: a report older than the last one from it is dropped. */
    seq?: number;
  }]; returns: null };
  /** Close a pane and every tab in it. No id is the caller's tab's pane. */
  "pane.close": { args: [pane?: PaneId | null]; returns: null };
  /**
   * Give a pane its view's focus (its front tab becomes the view's active tab), and make its view
   * and session current; or move from it to another: the one beside it (`direction`), so many
   * on (`offset`), or the one used last (`last`). No id is the caller's tab's pane. Moving out of
   * a zoomed pane unzooms the view; nothing there is not an error.
   */
  "pane.focus": { args: [pane?: PaneId | null, opts?: {
    /** To the pane beside it on this side. */
    direction?: "left" | "right" | "up" | "down" | null;
    /** To so many panes on (or back, negative), in layout order, wrapping. */
    offset?: number | null;
    /** To the pane of its view's tab used last. */
    last?: boolean;
  } | null]; returns: null };
  /**
   * A pane: its view, its tabs in its strip's order, the one in front, and whether it has the
   * focus or is zoomed. No id is the caller's tab's pane.
   */
  "pane.get": { args: [pane?: PaneId | null]; returns: {
    id: PaneId;
    view: ViewId;
    /** Its tabs, in the order its strip shows them. */
    tabs: TabId[];
    /** The tab in front. */
    activeTab: TabId;
    /** It has its view's focus. */
    isFocused: boolean;
    /** It fills its view. */
    isZoomed: boolean;
  } };
  /** List panes: a view's, or every view's, in layout order. */
  "pane.list": { args: [opts?: {
    /** Only this view's. */
    view?: ViewId | null;
  } | null]; returns: PaneId[] };
  /** Zoom a pane, or move its borders. No id is the caller's tab's pane. */
  "pane.update": { args: [pane: PaneId | null | undefined, patch: {
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
  }]; returns: null };
  /**
   * Install a plugin: a store id (`owner/name`, or a name only one listing has), a GitHub
   * repository (`owner/repo`, or `owner/repo/dir` for one in a folder), a git URL, or a directory
   * on this machine, which is linked. The commit installed is recorded; a manifest's `minPumi`
   * newer than this pumi is refused. One from git with `dependencies` has them installed (`bun
   * install --production --no-save --ignore-scripts`), and is refused when they fail to install.
   * An install from GitHub is counted by the store
   * (`pluginindex`'s server, once per address a day). One already installed stays as it is.
   * Answers its id (`owner/name`).
   */
  "plugin.add": { args: [source: string, opts?: {
    /** The git ref to follow: a tag, a branch, a commit. */
    version?: string;
    /** Link a directory on this machine instead of copying it. */
    linked?: boolean;
    /** Start it once installed (`true`, the default); `false` installs it turned off. */
    enabled?: boolean;
  }]; returns: string };
  /**
   * Change what a running plugin contributes, without starting it again: `patch` is a JSON merge
   * patch (RFC 7396) over its `contributes` as they are now (an object merges into an object, `null`
   * takes a key away, anything else replaces what was there, a list as a whole), checked as its
   * manifest is. Settings, the palette, keys, blocks and the window's slots follow at once. `null`
   * gives back what its manifest says, as the plugin stopping does; nothing is written to it.
   */
  "plugin.contribute": { args: [plugin: PluginId, patch: unknown]; returns: null };
  /**
   * Uninstall a plugin. A linked one's directory stays: only the link goes. What it kept stays
   * too, unless `data` says.
   */
  "plugin.remove": { args: [plugin: PluginId, opts?: {
    /**
     * Also what it kept: its own `config`, `state` and `cache` directories
     * (`stdpath(…)/plugins/<owner.name>`, its options' store among them), and its `secret`
     * options in the keychain. Without it they stay, for when it is installed again.
     */
    data?: boolean;
  }]; returns: null };
  /**
   * Patch a plugin, or refresh it from its source: a linked one is read again where it is, and a
   * git one is fetched and checked out at the ref it follows (or the one `version` gives), and its
   * dependencies are installed when they changed or are missing. What
   * its manifest contributes is read again, and the app starts it again. `enabled` turns it on or
   * off, and it stays installed.
   */
  "plugin.update": { args: [plugin: PluginId, patch?: {
    /** Turn it on or off; it stays installed. */
    enabled?: boolean;
    /** Follow another git ref from now on (a tag, a branch, a commit). */
    version?: string;
  }]; returns: null };
  /**
   * The processes on this machine, matching the fields given: `pid`, `command` (any part of its
   * command line), and `cwd` (its directory). No fields matches every process.
   */
  "process.list": { args: [opts?: {
    /** The process with this id. */
    pid?: number;
    /** The processes whose command line contains this. */
    command?: string;
    /** The processes whose directory is this one. */
    cwd?: string;
  }]; returns: ({
    pid: number;
    parentPid: number | null;
    /** How much processor it uses: percent of one core, over its whole life (as `ps` reports it). */
    cpu: number;
    /** Its resident memory, in bytes. */
    rss: number;
    command: string[];
    /**
     * `null` unless a `cwd` was asked for (reading it takes a system call a process), and on
     * Windows.
     */
    cwd: string | null;
  })[] };
  /**
   * Registered entries of one kind: functions, blocks, traits and options; the key bindings; the
   * menus; the plugins (or the store's, with `available`); the themes; the viewers of a file
   * (`path`); the methods `tab.call` answers; the models; and the plugins' pages and sections in
   * Settings. `{name}` picks one entry.
   */
  "registry.list": { args: [kind: "fn" | "block" | "trait" | "option" | "key" | "menu" | "plugin" | "theme" | "viewer" | "tabMethod" | "model" | "settings", opts?: {
    /**
     * One entry by its name (a function's, a block's, a trait's, an option's, a menu's path, a
     * tab method's; a model's, in any case).
     */
    name?: string;
    /** `plugin`: search the store's listings for `name` instead of the installed ones. */
    available?: boolean;
    /** `viewer`: the file the viewer is for. */
    path?: string;
    /** `fn`: the palette's entries, with the title a manifest's command gives them. */
    palette?: boolean;
  }]; returns: ({
    [key: string]: unknown;
  })[] };
  /**
   * Close a request: answer it, or dismiss it without `answer`. A notification takes no answer.
   * The first close wins: a request already closed is not found.
   */
  "request.close": { args: [request: RequestId, opts?: {
    /**
     * Its answer: `{"option": id}` picks one of its options, `{"text": …}` answers a question in
     * words (other fields go along). Without, it is closed unanswered.
     */
    answer?: unknown;
  } | null]; returns: null };
  /** An open request. */
  "request.get": { args: [request: RequestId]; returns: {
    id: RequestId;
    tab?: TabId | null;
    /** What a request puts to the user. */
    kind: "notification" | "permission" | "question";
    title: string;
    detail?: string | null;
    tool?: string | null;
    options: unknown[];
    level?: "info" | "warn" | "error" | "debug" | null;
    notification?: {
      subtitle?: string | null;
      /** `default`, or a sound's name. */
      sound?: string | null;
      /** A notification opened with the group of one still open replaces it. */
      group?: string | null;
      actions?: ({
        id: string;
        label: string;
      })[];
      /** A reply field's placeholder. */
      reply?: string | null;
      /** A file's path. */
      image?: string | null;
      urgency?: "low" | "normal" | "critical" | null;
      /** Carried back with what the user does with it. */
      data?: unknown;
    } | null;
    /** Who opened it: a plugin's id, or its channel's name. */
    source?: string | null;
    /** RFC 3339. */
    created: string;
    /** RFC 3339; `null`: it stays until it is closed. */
    expires?: string | null;
  } };
  /** The open requests, oldest first: every one, or those of `tab` and of `kind`. */
  "request.list": { args: [opts?: {
    /** Only this tab's. */
    tab?: TabId | null;
    /** Only requests of this kind. */
    kind?: "notification" | "permission" | "question" | null;
  } | null]; returns: RequestId[] };
  /**
   * Put something to the user: a notification (`kind: "notification"`, no `options`), a
   * permission for what runs in `tab` (the `tool` and the `options` to allow or reject it), or a
   * question. Whoever listens to `request.opened` shows it, and anyone may answer it with
   * `request.close(id, {answer})`; the first answer wins, and `request.closed` says how it ended.
   * pumi-host posts notifications to the OS's notification centre and takes them back when they
   * close. Answers the request's id.
   */
  "request.open": { args: [request: {
    /** What a request puts to the user. */
    kind?: "notification" | "permission" | "question";
    /** One line: what is asked or said (`Run "rm -rf build"?`). */
    title?: string;
    detail?: string | null;
    /** The tool a permission is for (`Bash`). */
    tool?: string | null;
    /**
     * The answers it offers: a permission's are `{id, label, kind}`; a question may offer none
     * and take text; a notification has none.
     */
    options?: unknown[];
    level?: "info" | "warn" | "error" | "debug" | null;
    notification?: {
      subtitle?: string | null;
      /** `default`, or a sound's name. */
      sound?: string | null;
      /** A notification opened with the group of one still open replaces it. */
      group?: string | null;
      actions?: ({
        id: string;
        label: string;
      })[];
      /** A reply field's placeholder. */
      reply?: string | null;
      /** A file's path. */
      image?: string | null;
      urgency?: "low" | "normal" | "critical" | null;
      /** Carried back with what the user does with it. */
      data?: unknown;
    } | null;
  }, opts?: {
    /** The tab it is for; a permission has one. */
    tab?: TabId | null;
    /**
     * When it closes unanswered (`timeout`), RFC 3339. A permission or a question expires in
     * 300 seconds without; a notification stays until it is closed.
     */
    expires?: string | null;
    /** Who opens it; the caller's plugin, or its channel's name, without. */
    source?: string | null;
  } | null]; returns: RequestId };
  /** Close a session and its views and tabs. No id is the caller's. */
  "session.close": { args: [session?: SessionId | null]; returns: null };
  /**
   * Show a session in the caller's client; or move from it to another by offset, or to the last
   * one (`-n`, `-p`, `-l`). No id is the caller's session.
   */
  "session.focus": { args: [session?: SessionId | null, opts?: {
    /** To so many sessions on (or back), in the order they were made, wrapping. */
    offset?: number | null;
    /** To the session used most recently before it. */
    last?: boolean;
  } | null]; returns: null };
  /**
   * A session: its name, directory, views and active view; with `deep`, the whole tree in one call.
   * No id is the caller's: its tab's session, its client's, or the one used last.
   */
  "session.get": { args: [session?: SessionId | null, opts?: {
    /** The whole tree: every view, each with its tabs. */
    deep?: boolean;
  } | null]; returns: {
    id: SessionId;
    name: string;
    /** Its directory, where its terminals start without a `cwd` of their own. */
    cwd?: string | null;
    /** Its views, in index order. */
    views: ViewId[];
    activeView?: ViewId | null;
    /** With `deep`: every view, each with its tabs. */
    tree?: ({
      id: ViewId;
      session: SessionId;
      /** The name it was given; `null` while it takes its active tab's title. */
      name?: string | null;
      /** Its index in its session. */
      index: number;
      /** A view's panes: a pane, or a split of two halves along an axis. */
      layout: {
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
        first: Layout;
        second: Layout;
      };
      /** The tab with the focus, in front of its pane. */
      activeTab?: TabId | null;
      /** The pane with the focus. */
      activePane?: PaneId | null;
      /** The pane zoomed to fill it. */
      zoom?: PaneId | null;
      tabs: ({
        id: TabId;
        session: SessionId;
        view?: ViewId | null;
        /** The block it holds (`file`, `owner.name.page`). */
        block: string;
        /** Each trait it has, with its fields: `{"pumi_dev.files.file": {path}}`. */
        traits: {
          [key: string]: unknown;
        };
        /** Where a tab is, and where `tab.open` and `tab.update` put one. */
        place: {
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
        };
        title?: string | null;
        /** Its directory: a terminal's program's. */
        cwd?: string | null;
        /** The directory of its foreground process. */
        foregroundCwd?: string | null;
        process?: {
          pid: number;
          /** Its program, as the system names it. */
          command: string[];
        } | null;
        /** How far a tab is scrolled back. */
        scroll: {
          offsetFromBottom: number;
        };
        /** The tab beside a tab on each side, in its view. */
        neighbors: {
          left?: TabId | null;
          right?: TabId | null;
          up?: TabId | null;
          down?: TabId | null;
        };
        /** It is the tab a client is in: its view's active tab, in its session's active view. */
        isFocused: boolean;
        /** It fills its view. */
        isZoomed: boolean;
        /** The options set on it. */
        options: {
          [key: string]: unknown;
        };
        /** With `state`: its saved state. */
        state?: {
          pumi_state: number;
          /** The block that saved it. */
          block: string;
          /** The version of that block's `data`. */
          block_version: number;
          /** The tab's own options, `@user` ones included. */
          options?: {
            [key: string]: unknown;
          };
          /** What the block's `serialize` returned. */
          data?: unknown;
          /** Large parts (scrollback, images), by name. */
          blobs?: {
            [key: string]: {
              /** How to read it: `vt` (terminal output), `png`, `bytes`. */
              encoding: string;
              size: number;
              /** `blob:<sha256>`. */
              ref: string;
              /** The bytes, in base64, when the state is to leave this server (`opts.blobs = "inline"`). */
              data?: string | null;
            };
          };
          /** What other plugins added in `TabSavePre`, by plugin. */
          extra?: {
            [key: string]: unknown;
          };
          /** How a state comes back. */
          strategy: "exact" | "relaunch" | "reference";
          /** What must be true where it is restored. */
          needs?: ({
            kind: "path";
            path: string;
          } | {
            kind: "program";
            name: string;
          } | {
            kind: "url";
            url: string;
          })[];
          origin?: {
            server: string;
            tab: number;
            /** RFC 3339, UTC. */
            at: string;
          } | null;
        } | null;
      })[];
    })[] | null;
  } };
  /** List every session, in creation order. */
  "session.list": { args: []; returns: SessionId[] };
  /**
   * Change a session's name or directory. Fires `session.changed` with what changed. No id is the
   * caller's session.
   */
  "session.update": { args: [session: SessionId | null | undefined, patch: {
    /** Its name; `null` gives it a free one of its own (its folder's, else a number). */
    name?: string | null;
    /** Its directory, where its terminals start without a `cwd` of their own; `null` has none. */
    cwd?: string | null;
  }]; returns: null };
  /**
   * Call a method of the block in a tab: the protocol every block answers (`read`, its text;
   * `input`, `{text}`, `{paste}` or `{keys}`; `selection`; `report`, an OSC 7501 body), a
   * terminal tab's `run`, `wait` and `attach` (a block on the terminal engine), and the block's own
   * methods — a terminal's `respawn`, a plugin block's `<block>.<method>`, a file tab's
   * `<viewer>.<method>`. `tab` is the caller's own
   * when it is `null`.
   */
  "tab.call": { args: [tab: TabId | null | undefined, method: string, args?: unknown]; returns: unknown };
  /**
   * Close a tab. Its state is kept, so it can be reopened (`tab.list({closed: true})`). No id is
   * the caller's.
   */
  "tab.close": { args: [tab?: TabId | null]; returns: null };
  /**
   * Make a tab its pane's front tab and its view's active tab, and its view and session current;
   * or move from it: to the next tab of its pane (`offset`), to the pane beside it (`direction`),
   * or to its view's tab used last (`last`). No id is the caller's tab. Moving out of a zoomed
   * pane unzooms the view; nothing there is not an error.
   */
  "tab.focus": { args: [tab?: TabId | null, opts?: {
    /** To the tab in front of the pane beside its pane on this side. */
    direction?: "left" | "right" | "up" | "down" | null;
    /** To so many tabs on (or back, negative) in its pane's strip, wrapping. */
    offset?: number | null;
    /** To its view's tab used last. */
    last?: boolean;
  } | null]; returns: null };
  /**
   * A tab: its block, traits, place, title, directories, process, neighbours, focus, zoom and
   * options; with `state`, its saved state. No id is the caller's: `PUMI_TAB` for a program in a
   * tab, else its view's active tab.
   */
  "tab.get": { args: [tab?: TabId | null, opts?: {
    /** Its saved state, as persistence saves it (`true`), or saved as these say. */
    state?: boolean | {
      /**
       * What the state holds beyond its options and type data, instead of the `viewoptions`
       * option: `"scrollback,history"`.
       */
      viewOptions?: string | null;
      /** `restore` (the default), `copy`, `move` or `share`: a type leaves out what should not travel. */
      purpose?: "restore" | "copy" | "move" | "share" | null;
      /** `ref` keeps blobs on this server (the default); `inline` puts their bytes in the state. */
      blobs?: "ref" | "inline" | null;
    } | null;
  } | null]; returns: {
    id: TabId;
    session: SessionId;
    view?: ViewId | null;
    /** The block it holds (`file`, `owner.name.page`). */
    block: string;
    /** Each trait it has, with its fields: `{"pumi_dev.files.file": {path}}`. */
    traits: {
      [key: string]: unknown;
    };
    /** Where a tab is, and where `tab.open` and `tab.update` put one. */
    place: {
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
    };
    title?: string | null;
    /** Its directory: a terminal's program's. */
    cwd?: string | null;
    /** The directory of its foreground process. */
    foregroundCwd?: string | null;
    process?: {
      pid: number;
      /** Its program, as the system names it. */
      command: string[];
    } | null;
    /** How far a tab is scrolled back. */
    scroll: {
      offsetFromBottom: number;
    };
    /** The tab beside a tab on each side, in its view. */
    neighbors: {
      left?: TabId | null;
      right?: TabId | null;
      up?: TabId | null;
      down?: TabId | null;
    };
    /** It is the tab a client is in: its view's active tab, in its session's active view. */
    isFocused: boolean;
    /** It fills its view. */
    isZoomed: boolean;
    /** The options set on it. */
    options: {
      [key: string]: unknown;
    };
    /** With `state`: its saved state. */
    state?: {
      pumi_state: number;
      /** The block that saved it. */
      block: string;
      /** The version of that block's `data`. */
      block_version: number;
      /** The tab's own options, `@user` ones included. */
      options?: {
        [key: string]: unknown;
      };
      /** What the block's `serialize` returned. */
      data?: unknown;
      /** Large parts (scrollback, images), by name. */
      blobs?: {
        [key: string]: {
          /** How to read it: `vt` (terminal output), `png`, `bytes`. */
          encoding: string;
          size: number;
          /** `blob:<sha256>`. */
          ref: string;
          /** The bytes, in base64, when the state is to leave this server (`opts.blobs = "inline"`). */
          data?: string | null;
        };
      };
      /** What other plugins added in `TabSavePre`, by plugin. */
      extra?: {
        [key: string]: unknown;
      };
      /** How a state comes back. */
      strategy: "exact" | "relaunch" | "reference";
      /** What must be true where it is restored. */
      needs?: ({
        kind: "path";
        path: string;
      } | {
        kind: "program";
        name: string;
      } | {
        kind: "url";
        url: string;
      })[];
      origin?: {
        server: string;
        tab: number;
        /** RFC 3339, UTC. */
        at: string;
      } | null;
    } | null;
  } };
  /**
   * List tabs, every one or those `opts` asks for: pane by pane in layout order within a view,
   * then floats and hidden tabs. `closed` lists the tabs closed lately instead.
   */
  "tab.list": { args: [opts?: {
    /** Only this session's. */
    session?: SessionId | null;
    /** Only this view's. */
    view?: ViewId | null;
    /** Only tabs that hold this block. */
    block?: string | null;
    /** Only tabs with this trait (`pumi_dev.files.file`): their option `@<trait>` is set. */
    trait?: string | null;
    /**
     * The tabs closed lately instead, newest first, each with its saved state and where it
     * was. Reopening one is `tab.open(state.block, {state}, {kind: "tiled", pane})` while its
     * pane is there, else `{kind: "tiled", view, beside}`.
     */
    closed?: boolean;
  } | null]; returns: TabId[] | ({
    tab: TabId;
    /** Everything needed to bring a tab back, here or on another server. */
    state: {
      pumi_state: number;
      /** The block that saved it. */
      block: string;
      /** The version of that block's `data`. */
      block_version: number;
      /** The tab's own options, `@user` ones included. */
      options?: {
        [key: string]: unknown;
      };
      /** What the block's `serialize` returned. */
      data?: unknown;
      /** Large parts (scrollback, images), by name. */
      blobs?: {
        [key: string]: {
          /** How to read it: `vt` (terminal output), `png`, `bytes`. */
          encoding: string;
          size: number;
          /** `blob:<sha256>`. */
          ref: string;
          /** The bytes, in base64, when the state is to leave this server (`opts.blobs = "inline"`). */
          data?: string | null;
        };
      };
      /** What other plugins added in `TabSavePre`, by plugin. */
      extra?: {
        [key: string]: unknown;
      };
      /** How a state comes back. */
      strategy: "exact" | "relaunch" | "reference";
      /** What must be true where it is restored. */
      needs?: ({
        kind: "path";
        path: string;
      } | {
        kind: "program";
        name: string;
      } | {
        kind: "url";
        url: string;
      })[];
      origin?: {
        server: string;
        tab: number;
        /** RFC 3339, UTC. */
        at: string;
      } | null;
    };
    /** Where it was: its view, its pane, and a tab beside its pane, if they are still there. */
    view?: ViewId | null;
    pane?: PaneId | null;
    beside?: TabId | null;
    /** When it closed, in RFC 3339. */
    at: string;
  })[] };
  /**
   * Open a tab holding `block` (`null`: what a new tab holds, what the `new_tab` option names, a
   * block or a plugin's `tab.new` action; while it names nothing installed, the Launcher's list, a
   * shell or the first thing installed that starts one; refused while nothing is, as pumi has no
   * block of its own for one), placed as `place` says:
   * in a pane split from a tab's, into a pane
   * as one of its tabs (`{"pane": ":2"}`), in a new view or session, floating; or in an open tab
   * instead of its block (`{"kind": "tiled", "replace": "%1"}`), which
   * keeps its id, place, focus and options (`tab.changed {block}`). It is the one way anything
   * appears. `args` are the block's (a terminal block's `command`, `cwd` and `env`; a file's `path`;
   * a page's `url`), or `{"state": …}` to restore a saved state into the tab. Returns the tab's id.
   */
  "tab.open": { args: [block?: string | null, args?: unknown, place?: {
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
  } | null]; returns: TabId };
  /**
   * Change a tab: its title, place, size, zoom, whether it was seen, and its options. Each change
   * fires `tab.changed` (or its view's `view.changed`) with what changed. No id is the caller's tab.
   */
  "tab.update": { args: [tab: TabId | null | undefined, patch: {
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
     * its `@status` stops asking. The app says so when the tab is in front of the user.
     */
    seen?: boolean | null;
    /** Options to set on it; `null` unsets one. */
    options?: {
      [key: string]: unknown;
    } | null;
  }]; returns: null };
  /** Close a view and every tab in it. No id is the caller's. */
  "view.close": { args: [view?: ViewId | null]; returns: null };
  /**
   * Make a view its session's active view; or move from it to another of its session by offset
   * (`next-window` is `{"offset": 1}`), by index, or the last one. No id is the caller's view.
   */
  "view.focus": { args: [view?: ViewId | null, opts?: {
    /** To so many views on (or back, negative), wrapping. */
    offset?: number | null;
    /** To the session's view at this index. */
    index?: number | null;
    /** To the view used before the session's active one. */
    last?: boolean;
  } | null]; returns: null };
  /**
   * A view: its session, name, index, layout of panes (each with its tabs), active tab and pane,
   * and zoom. No id is the caller's: its session's active view.
   */
  "view.get": { args: [view?: ViewId | null]; returns: {
    id: ViewId;
    session: SessionId;
    /** The name it was given; `null` while it takes its active tab's title. */
    name?: string | null;
    /** Its index in its session. */
    index: number;
    /** A view's panes: a pane, or a split of two halves along an axis. */
    layout: {
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
      first: Layout;
      second: Layout;
    };
    /** The tab with the focus, in front of its pane. */
    activeTab?: TabId | null;
    /** The pane with the focus. */
    activePane?: PaneId | null;
    /** The pane zoomed to fill it. */
    zoom?: PaneId | null;
  } };
  /**
   * Change a view: its name, index or session, layout, and zoom. Each change fires `view.changed`
   * with what changed. No id is the caller's view.
   */
  "view.update": { args: [view: ViewId | null | undefined, patch: {
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
      first: Layout;
      second: Layout;
    } | null;
    /** The pane zoomed to fill it, which gets its focus; `null` unzooms it. */
    zoom?: PaneId | null;
  }]; returns: null };
}

/** Every method a tab answers through `tab.call`: its arguments and its answer. */
export interface TabMethods {
  /** Text read from a tab: a terminal's as its screen shows it, one line a row. */
  read: { args: {
    /** `visible`: the screen as it is; `recent` (the default): the scrollback and the screen; `unwrapped`: those, with lines wrapped at the screen's width joined. */
    source?: "visible" | "recent" | "unwrapped";
    /** The last this many lines that hold something: the screen's rows for `visible`, 200 otherwise (`pumi tab read`: 50). */
    lines?: number;
    /** The output as the program wrote it, its escape sequences kept. */
    ansi?: boolean;
  }; returns: {
    text: string;
  } };
  /** Text, a paste, or keys sent to a tab. */
  input: { args: {
    text: string;
  } | {
    paste: string;
  } | {
    keys: string[];
  }; returns: null };
  /** A command pasted and entered as one write. */
  run: { args: {
    command: string | string[];
  }; returns: null };
  /** A tab waited on for output or exit. */
  wait: { args: {
    match?: string;
    regex?: string;
    exit?: boolean;
    timeout?: number;
  }; returns: {
    match: string | null;
    exitCode: number | null;
  } };
  /** The tab selection. */
  selection: { args: {}; returns: {
    text: string;
  } };
  /** An OSC 7501 report sent by a tab. */
  report: { args: {
    body: unknown;
  }; returns: null };
  /** A terminal stream attached to a tab. */
  attach: { args: {
    mode: "control" | "observe";
    takeover?: boolean;
    cols?: number;
    rows?: number;
  }; returns: {
    stream: string;
  } };
}

/** Every event the core and the host fire, and its `data`. */
export interface Events {
  /**
   * A tab opened: `block` is what it holds; `restored` when it opened from a saved state.
   *
   * Its `match` is the tab's block.
   */
  "tab.opened": {
    tab: TabId;
    block: string;
    restored: boolean;
  };
  /**
   * A tab closed: `block` is what it held.
   *
   * Its `match` is the tab's block.
   */
  "tab.closed": {
    tab: TabId;
    block: string;
  };
  /**
   * A tab changed in the fields `keys` names (`title`, `cwd`, `block`, `place`, `size`, `exitCode`, `options.<name>`, `traits.<trait>`); `patch` holds their new values.
   *
   * Its `match` is the tab id.
   */
  "tab.changed": {
    tab: TabId;
    keys: ("title" | "cwd" | "foregroundCwd" | "block" | "place" | "size" | "exitCode" | "options.*" | "traits.*")[];
    patch: {
      [key: string]: unknown;
    };
  };
  /**
   * A tab's program asked for attention: it rang the bell, or sent an OSC 9 or 777 notification (`message`).
   *
   * Its `match` is the alert kind.
   */
  "tab.alerted": {
    tab: TabId;
    kind: "bell" | "osc9" | "osc777";
    message: string | null;
  };
  /**
   * A request was opened for the user (`request.open`): show it, and answer it with `request.close(id, {answer})`.
   *
   * Its `match` is the request's kind: `notification`, `permission` or `question`.
   */
  "request.opened": {
    request: RequestId;
    tab: TabId | null;
    kind: string;
    title: string;
    detail: string | null;
    options: unknown[];
    tool: string | null;
    level: "info" | "warn" | "error" | "debug" | null;
    notification: {
      subtitle?: string;
      sound?: string;
      group?: string;
      actions?: ({
        id: string;
        label: string;
      })[];
      reply?: string;
      image?: string;
      urgency?: "low" | "normal" | "critical";
      data?: unknown;
    } | null;
    source: string | null;
    created: string;
    expires: string | null;
  };
  /**
   * A request closed: `answered`, `closed` (unanswered, its tab closed, or a newer one of its group replaced it), `timeout` or `cancelled` (the channel that opened it went).
   *
   * Its `match` is the request's id.
   */
  "request.closed": {
    request: RequestId;
    tab: TabId | null;
    answer: unknown;
    reason: "answered" | "timeout" | "closed" | "cancelled";
  };
  /**
   * A view opened in a session.
   *
   * Its `match` is the object id.
   */
  "view.opened": {
    view: ViewId;
    session: SessionId;
  };
  /**
   * A view closed.
   *
   * Its `match` is the object id.
   */
  "view.closed": {
    view: ViewId;
    session: SessionId;
  };
  /**
   * A view changed in the fields `keys` names (`name`, `index`, `session`, `layout`, `activeTab`, `activePane`, `zoom`); `patch` holds their new values.
   *
   * Its `match` is the object id.
   */
  "view.changed": {
    view: ViewId;
    keys: ("name" | "index" | "session" | "layout" | "activeTab" | "activePane" | "zoom")[];
    patch: {
      [key: string]: unknown;
    };
  };
  /**
   * A pane opened in a view: a tab split one off, or a view was made.
   *
   * Its `match` is the object id.
   */
  "pane.opened": {
    pane: PaneId;
    view: ViewId;
  };
  /**
   * A pane closed: its last tab went, or its view did.
   *
   * Its `match` is the object id.
   */
  "pane.closed": {
    pane: PaneId;
    view: ViewId;
  };
  /**
   * A pane changed in the fields `keys` names (`tabs`, `activeTab`); `patch` holds their new values.
   *
   * Its `match` is the object id.
   */
  "pane.changed": {
    pane: PaneId;
    keys: ("tabs" | "activeTab")[];
    patch: {
      [key: string]: unknown;
    };
  };
  /**
   * A session opened.
   *
   * Its `match` is the object id.
   */
  "session.opened": {
    session: SessionId;
    name: string;
    activeView: ViewId | null;
  };
  /**
   * A session closed.
   *
   * Its `match` is the object id.
   */
  "session.closed": {
    session: SessionId;
  };
  /**
   * A session changed in the fields `keys` names (`name`, `cwd`, `activeView`); `patch` holds their new values.
   *
   * Its `match` is the object id.
   */
  "session.changed": {
    session: SessionId;
    keys: ("name" | "activeView" | "cwd")[];
    patch: {
      [key: string]: unknown;
    };
  };
  /**
   * A global or plugin option changed; an object's options fire that object's `changed`.
   *
   * Its `match` is the option's name.
   */
  "option.changed": {
    name: string;
    scope: "global" | "plugin";
    plugin: PluginId | null;
    old: unknown;
    new: unknown;
    source: string | null;
  };
  /**
   * A client's app came to the front or went behind (`isFocused`): one of its windows has the focus, or none has; `client` says whose.
   *
   * Its `match` is the client's id.
   */
  "app.changed": {
    client: number;
    keys: ("isFocused")[];
    patch: {
      isFocused: boolean;
    };
  };
  /**
   * Entries of a registry kind (`registry.list`) were set, removed or changed: `names` are theirs.
   *
   * Its `match` is the registry kind.
   */
  "registry.changed": {
    kind: "fn" | "block" | "trait" | "option" | "key" | "menu" | "plugin" | "theme" | "viewer";
    names: string[];
  };
}

/** One object per noun of the export, a method per function: each is one wire call. */
export interface Nouns {
  clipboard: {
    /**
     * The clipboard's text: `null` when it holds none.
     *
     * @example
     * await clipboard.get() // "hello"
     */
    get(): Promise<string | null>;
    /**
     * Put text on the clipboard; `null` clears it.
     *
     * @example
     * await clipboard.set("hello")
     */
    set(text: string | null): Promise<null>;
  };
  event: {
    /**
     * Fire an event to every channel subscribed: mostly a plugin's own (`owner.name.done`), with
     * which plugins and scripts tell each other things, and the events channels have of their own
     * (the CLI's `plugin.reloadRequested`).
     *
     * @example
     * await event.emit("owner.name.done", { data: { status: 0 }, tab: "%3" })
     */
    emit(event: string, opts?: {
    data?: unknown;
    /** The event's `match`: what its listeners' `match` is tested against. */
    match?: string | null;
    /** The tab it is about; one just closed may be named. */
    tab?: TabId | null;
  } | null): Promise<null>;
    /**
     * End a subscription (`event.on`'s handle); whether there was one.
     *
     * @example
     * await event.off(8) // true
     */
    off(subscription: number): Promise<boolean>;
    /**
     * Subscribe to events, by name or glob (`tab.*`; `*` is every event but those fired before
     * what they name, in `-ing`). Each one that fires goes to the caller's channel
     * as `{"method": "rpc.event", "params": [subscription, event]}`, until `event.off` or the
     * channel closes. Answers the subscription's integer handle.
     *
     * @example
     * await event.on("tab.changed", { match: "%3" }) // 8
     */
    on(event: string | string[], opts?: {
    /**
     * Globs matched against the event's `match` (a tab's id for `tab.changed`, a request's
     * kind for `request.opened`); every match without one.
     */
    match?: string | string[] | null;
    /** End the subscription after its first event. */
    once?: boolean;
    /** What it is for, as a listing shows it. */
    desc?: string | null;
  } | null): Promise<number>;
    /**
     * Wait for an event, by name or glob: the call answers when one that matches fires, with the
     * event (`{event, match, session, view, tab, data}`), or with `null` when `timeout` passes or
     * the caller goes. A wait that times out is not an error.
     *
     * @example
     * await event.wait("tab.changed", { match: "%3", timeout: 10 })
     */
    wait(event: string | string[], opts?: {
    /**
     * Globs matched against the event's `match` (a tab's id for `tab.changed`, a request's id
     * for `request.closed`); any match without one.
     */
    match?: string | string[] | null;
    /** Seconds to wait; none: until it fires. */
    timeout?: number | null;
  } | null): Promise<{
    event: string;
    match: string;
    session?: SessionId | null;
    view?: ViewId | null;
    tab?: TabId | null;
    data: unknown;
  } | null>;
  };
  fn: {
    /**
     * Provide a function every surface can call (`pumi call git.status`, a script's
     * `pumi.api["git.status"]`): calls to it from any channel come to this one as requests, with
     * the caller in `rpc.caller`, and its answers go back; or, for one with a `callback`, as a
     * `rpc.callback` notification, while the call answers `null`. It appears in `api.getInfo` with
     * the rest. A core function cannot be provided, and nor can one another channel provides as
     * someone else (another plugin, another program): it is theirs until they withdraw it. With
     * `runsOn: "client"` each app provides its own, for its own client. `null` withdraws it; it goes
     * with the channel too.
     *
     * @example
     * await fn.set("hello.greet", { desc: "Greet.", effect: "read" })
     */
    set(name: string, spec: {
    desc: string;
    /** `read` or `write`; `write` by default. */
    effect?: "read" | "write" | null;
    /** A JSON Schema for its arguments; one that takes any object without it. */
    paramsSchema?: {
      [key: string]: unknown;
    } | null;
    /** A JSON Schema for its answer. */
    returnsSchema?: {
      [key: string]: unknown;
    } | null;
    /**
     * Send each call here as a `rpc.callback` notification, with its arguments in order, and
     * answer `null`: `{"chan": 0, "ref": …}`, a reference a channel holds. Without one the
     * call comes to the channel as a request, and its answer is the call's.
     */
    callback?: unknown;
    /**
     * `client`: each app that provides it answers the calls made for its own client (its
     * clipboard, its updater), and others' go to the first app that does. `server` by default:
     * one channel answers every call.
     */
    runsOn?: "server" | "client" | null;
  } | null): Promise<null>;
  };
  fs: {
    /**
     * The entries of a folder on the server's machine: `~` and `~/…` are its home, a relative path
     * is in it, and an empty path is the home itself. Hidden entries are left out unless `hidden`,
     * and `recursive` lists every folder under it too; both sorted by name, case-insensitively.
     *
     * @example
     * await fs.list("~/src")
     */
    list(path: string, opts?: {
    /** Hidden entries too (`.` files). */
    hidden?: boolean;
    /** Every folder under it, not only its own entries. */
    recursive?: boolean;
  }): Promise<({
    /** Its name in its folder. */
    name: string;
    /** Its full path. */
    path: string;
    /** What a path in a listing is. */
    kind: "file" | "directory" | "symlink";
  })[]>;
    /**
     * Open a path in its default app, or in the one `with` names; `reveal` shows it in the file
     * manager instead.
     *
     * @example
     * await fs.open("/tmp", { reveal: true })
     */
    open(path: string, opts?: {
    /** Show it in the file manager (the Finder, Explorer) instead of opening it. */
    reveal?: boolean;
    /** The app to open it with: by name on macOS (`Safari`), a program elsewhere (`code`). */
    with?: string;
  }): Promise<null>;
    /**
     * Choose files to open, or a folder: a native picker. Answers the path chosen, the paths when
     * it takes more than one, and `null` when it was cancelled.
     *
     * @example
     * await fs.pick({ kind: "file", multiple: true }) // ["/tmp/a.log"]
     */
    pick(opts?: {
    /** Files, or a folder (`file`). */
    kind?: "file" | "directory";
    /** Allow choosing more than one file. */
    multiple?: boolean;
    /** What the picker's button says (`Open Local Folder`). */
    title?: string;
    /**
     * Only files these globs fit (`["*.png", "*.jpg"]`), as a renderer's `match` reads them:
     * one without a `/` is matched against the file's name. On macOS the picker greys out the
     * rest when every glob is `*.<ext>`; elsewhere a file none fits is refused once chosen.
     */
    match?: string[];
    /**
     * Where it starts: a file's folder, or a folder. Taken but not followed: the system's
     * picker opens where it last was.
     */
    path?: string;
  }): Promise<string | string[] | null>;
    /**
     * Move paths to the Trash.
     *
     * @example
     * await fs.trash(["/tmp/old.log"])
     */
    trash(paths: string[]): Promise<null>;
  };
  menu: {
    /**
     * Set a menu item: `path` names it and every menu above it, which are made as needed, and the
     * item is the function choosing it calls, with its title and when it shows. Setting a path
     * again replaces the item; `null` removes it, and a menu with everything in it.
     *
     * @example
     * await menu.set("File.Build", { fn: "owner.name.build", title: "Build" })
     */
    set(path: string, item: {
    /** The function choosing it calls. */
    fn: string;
    /** Its arguments, positional or by name; none by default. */
    args?: unknown;
    /** What it says in the menu; made of the function's name by default. */
    title?: string;
    /** When it shows; always without it. */
    when?: string;
  } | null): Promise<null>;
  };
  option: {
    /**
     * An option's value in a scope: the object's own, else the one it inherits, else the default.
     * A plugin's is its own store, and a `secret` one is in the OS keychain.
     *
     * @example
     * await option.get("guifont", {}) // "Berkeley Mono 14"
     */
    get(name: string, opts?: {
    /**
     * `global`, `session`, `view`, `tab` or `plugin`. Without it: the option's own scope, at the
     * object `id` names (the caller's own without that).
     */
    scope?: "global" | "session" | "view" | "tab" | "plugin";
    /** The object: `$1`, `@2` or `%3`. */
    id?: string;
    /** The plugin whose store it is, for `{scope: "plugin"}`: `owner/name`. */
    plugin?: PluginId;
  }): Promise<unknown>;
    /**
     * Set an option on an object, in a plugin's store, or globally. `@name` options need no
     * declaration. Setting a type's option is acting on the tab: a browser's `url` navigates.
     * `null` unsets it, so it inherits again. `ttl` makes the value expire, and a report whose `seq`
     * is not past the last one from the same `source` is dropped.
     *
     * @example
     * await option.set("@build", "ok", { scope: "global" })
     */
    set(name: string, value: unknown, opts?: {
    /** Which object an option is read from or set on, by scope. */
    scope?: "global" | "session" | "view" | "tab" | "plugin";
    id?: string;
    plugin?: PluginId;
    /** Seconds the value holds for; then it is the one it inherits again. */
    ttl?: number;
    /** Who set it (`report-agent`'s reporter); the caller's source by default. */
    source?: string;
    /** With `source`: a report older than the last one from it is dropped. */
    seq?: number;
  }): Promise<null>;
  };
  pane: {
    /**
     * Close a pane and every tab in it. No id is the caller's tab's pane.
     *
     * @example
     * await pane.close(":1")
     */
    close(pane?: PaneId | null): Promise<null>;
    /**
     * Give a pane its view's focus (its front tab becomes the view's active tab), and make its view
     * and session current; or move from it to another: the one beside it (`direction`), so many
     * on (`offset`), or the one used last (`last`). No id is the caller's tab's pane. Moving out of
     * a zoomed pane unzooms the view; nothing there is not an error.
     *
     * @example
     * await pane.focus(":2")
     */
    focus(pane?: PaneId | null, opts?: {
    /** To the pane beside it on this side. */
    direction?: "left" | "right" | "up" | "down" | null;
    /** To so many panes on (or back, negative), in layout order, wrapping. */
    offset?: number | null;
    /** To the pane of its view's tab used last. */
    last?: boolean;
  } | null): Promise<null>;
    /**
     * A pane: its view, its tabs in its strip's order, the one in front, and whether it has the
     * focus or is zoomed. No id is the caller's tab's pane.
     *
     * @example
     * await pane.get(":1") // { id: ":1", view: "@1", tabs: ["%1", "%2"], activeTab: "%1", isFocused: true, isZoomed: false }
     */
    get(pane?: PaneId | null): Promise<{
    id: PaneId;
    view: ViewId;
    /** Its tabs, in the order its strip shows them. */
    tabs: TabId[];
    /** The tab in front. */
    activeTab: TabId;
    /** It has its view's focus. */
    isFocused: boolean;
    /** It fills its view. */
    isZoomed: boolean;
  }>;
    /**
     * List panes: a view's, or every view's, in layout order.
     *
     * @example
     * await pane.list({}) // [":1"]
     */
    list(opts?: {
    /** Only this view's. */
    view?: ViewId | null;
  } | null): Promise<PaneId[]>;
    /**
     * Zoom a pane, or move its borders. No id is the caller's tab's pane.
     *
     * @example
     * await pane.update(":2", { zoom: true })
     */
    update(pane: PaneId | null | undefined, patch: {
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
  }): Promise<null>;
  };
  plugin: {
    /**
     * Install a plugin: a store id (`owner/name`, or a name only one listing has), a GitHub
     * repository (`owner/repo`, or `owner/repo/dir` for one in a folder), a git URL, or a directory
     * on this machine, which is linked. The commit installed is recorded; a manifest's `minPumi`
     * newer than this pumi is refused. One from git with `dependencies` has them installed (`bun
     * install --production --no-save --ignore-scripts`), and is refused when they fail to install.
     * An install from GitHub is counted by the store
     * (`pluginindex`'s server, once per address a day). One already installed stays as it is.
     * Answers its id (`owner/name`).
     *
     * @example
     * await plugin.add("https://github.com/someone/pumi-caffeinate", {}) // "someone/caffeinate"
     */
    add(source: string, opts?: {
    /** The git ref to follow: a tag, a branch, a commit. */
    version?: string;
    /** Link a directory on this machine instead of copying it. */
    linked?: boolean;
    /** Start it once installed (`true`, the default); `false` installs it turned off. */
    enabled?: boolean;
  }): Promise<string>;
    /**
     * Change what a running plugin contributes, without starting it again: `patch` is a JSON merge
     * patch (RFC 7396) over its `contributes` as they are now (an object merges into an object, `null`
     * takes a key away, anything else replaces what was there, a list as a whole), checked as its
     * manifest is. Settings, the palette, keys, blocks and the window's slots follow at once. `null`
     * gives back what its manifest says, as the plugin stopping does; nothing is written to it.
     *
     * @example
     * await plugin.contribute("owner/name", { slots: { "tab.new": { actions: [{ id: "claude", title: "Claude Code", icon: "sparkle", command: "open" }] } } })
     */
    contribute(plugin: PluginId, patch: unknown): Promise<null>;
    /**
     * Uninstall a plugin. A linked one's directory stays: only the link goes. What it kept stays
     * too, unless `data` says.
     *
     * @example
     * await plugin.remove("owner/name", {})
     */
    remove(plugin: PluginId, opts?: {
    /**
     * Also what it kept: its own `config`, `state` and `cache` directories
     * (`stdpath(…)/plugins/<owner.name>`, its options' store among them), and its `secret`
     * options in the keychain. Without it they stay, for when it is installed again.
     */
    data?: boolean;
  }): Promise<null>;
    /**
     * Patch a plugin, or refresh it from its source: a linked one is read again where it is, and a
     * git one is fetched and checked out at the ref it follows (or the one `version` gives), and its
     * dependencies are installed when they changed or are missing. What
     * its manifest contributes is read again, and the app starts it again. `enabled` turns it on or
     * off, and it stays installed.
     *
     * @example
     * await plugin.update("owner/name", { enabled: false })
     */
    update(plugin: PluginId, patch?: {
    /** Turn it on or off; it stays installed. */
    enabled?: boolean;
    /** Follow another git ref from now on (a tag, a branch, a commit). */
    version?: string;
  }): Promise<null>;
  };
  process: {
    /**
     * The processes on this machine, matching the fields given: `pid`, `command` (any part of its
     * command line), and `cwd` (its directory). No fields matches every process.
     *
     * @example
     * await process.list()
     */
    list(opts?: {
    /** The process with this id. */
    pid?: number;
    /** The processes whose command line contains this. */
    command?: string;
    /** The processes whose directory is this one. */
    cwd?: string;
  }): Promise<({
    pid: number;
    parentPid: number | null;
    /** How much processor it uses: percent of one core, over its whole life (as `ps` reports it). */
    cpu: number;
    /** Its resident memory, in bytes. */
    rss: number;
    command: string[];
    /**
     * `null` unless a `cwd` was asked for (reading it takes a system call a process), and on
     * Windows.
     */
    cwd: string | null;
  })[]>;
  };
  registry: {
    /**
     * Registered entries of one kind: functions, blocks, traits and options; the key bindings; the
     * menus; the plugins (or the store's, with `available`); the themes; the viewers of a file
     * (`path`); the methods `tab.call` answers; the models; and the plugins' pages and sections in
     * Settings. `{name}` picks one entry.
     *
     * @example
     * await registry.list("fn", { palette: true }) // [{ name: "owner.name.build", kind: "fn" }]
     */
    list(kind: "fn" | "block" | "trait" | "option" | "key" | "menu" | "plugin" | "theme" | "viewer" | "tabMethod" | "model" | "settings", opts?: {
    /**
     * One entry by its name (a function's, a block's, a trait's, an option's, a menu's path, a
     * tab method's; a model's, in any case).
     */
    name?: string;
    /** `plugin`: search the store's listings for `name` instead of the installed ones. */
    available?: boolean;
    /** `viewer`: the file the viewer is for. */
    path?: string;
    /** `fn`: the palette's entries, with the title a manifest's command gives them. */
    palette?: boolean;
  }): Promise<({
    [key: string]: unknown;
  })[]>;
  };
  request: {
    /**
     * Close a request: answer it, or dismiss it without `answer`. A notification takes no answer.
     * The first close wins: a request already closed is not found.
     *
     * @example
     * await request.close("+7", { answer: { option: "allow" } })
     */
    close(request: RequestId, opts?: {
    /**
     * Its answer: `{"option": id}` picks one of its options, `{"text": …}` answers a question in
     * words (other fields go along). Without, it is closed unanswered.
     */
    answer?: unknown;
  } | null): Promise<null>;
    /**
     * An open request.
     *
     * @example
     * await request.get("+7")
     */
    get(request: RequestId): Promise<{
    id: RequestId;
    tab?: TabId | null;
    /** What a request puts to the user. */
    kind: "notification" | "permission" | "question";
    title: string;
    detail?: string | null;
    tool?: string | null;
    options: unknown[];
    level?: "info" | "warn" | "error" | "debug" | null;
    notification?: {
      subtitle?: string | null;
      /** `default`, or a sound's name. */
      sound?: string | null;
      /** A notification opened with the group of one still open replaces it. */
      group?: string | null;
      actions?: ({
        id: string;
        label: string;
      })[];
      /** A reply field's placeholder. */
      reply?: string | null;
      /** A file's path. */
      image?: string | null;
      urgency?: "low" | "normal" | "critical" | null;
      /** Carried back with what the user does with it. */
      data?: unknown;
    } | null;
    /** Who opened it: a plugin's id, or its channel's name. */
    source?: string | null;
    /** RFC 3339. */
    created: string;
    /** RFC 3339; `null`: it stays until it is closed. */
    expires?: string | null;
  }>;
    /**
     * The open requests, oldest first: every one, or those of `tab` and of `kind`.
     *
     * @example
     * await request.list({ tab: "%3" }) // ["+7"]
     */
    list(opts?: {
    /** Only this tab's. */
    tab?: TabId | null;
    /** Only requests of this kind. */
    kind?: "notification" | "permission" | "question" | null;
  } | null): Promise<RequestId[]>;
    /**
     * Put something to the user: a notification (`kind: "notification"`, no `options`), a
     * permission for what runs in `tab` (the `tool` and the `options` to allow or reject it), or a
     * question. Whoever listens to `request.opened` shows it, and anyone may answer it with
     * `request.close(id, {answer})`; the first answer wins, and `request.closed` says how it ended.
     * pumi-host posts notifications to the OS's notification centre and takes them back when they
     * close. Answers the request's id.
     *
     * @example
     * await request.open({ kind: "notification", title: "Build finished" }) // "+7"
     */
    open(request: {
    /** What a request puts to the user. */
    kind?: "notification" | "permission" | "question";
    /** One line: what is asked or said (`Run "rm -rf build"?`). */
    title?: string;
    detail?: string | null;
    /** The tool a permission is for (`Bash`). */
    tool?: string | null;
    /**
     * The answers it offers: a permission's are `{id, label, kind}`; a question may offer none
     * and take text; a notification has none.
     */
    options?: unknown[];
    level?: "info" | "warn" | "error" | "debug" | null;
    notification?: {
      subtitle?: string | null;
      /** `default`, or a sound's name. */
      sound?: string | null;
      /** A notification opened with the group of one still open replaces it. */
      group?: string | null;
      actions?: ({
        id: string;
        label: string;
      })[];
      /** A reply field's placeholder. */
      reply?: string | null;
      /** A file's path. */
      image?: string | null;
      urgency?: "low" | "normal" | "critical" | null;
      /** Carried back with what the user does with it. */
      data?: unknown;
    } | null;
  }, opts?: {
    /** The tab it is for; a permission has one. */
    tab?: TabId | null;
    /**
     * When it closes unanswered (`timeout`), RFC 3339. A permission or a question expires in
     * 300 seconds without; a notification stays until it is closed.
     */
    expires?: string | null;
    /** Who opens it; the caller's plugin, or its channel's name, without. */
    source?: string | null;
  } | null): Promise<RequestId>;
  };
  session: {
    /**
     * Close a session and its views and tabs. No id is the caller's.
     *
     * @example
     * await session.close("$1")
     */
    close(session?: SessionId | null): Promise<null>;
    /**
     * Show a session in the caller's client; or move from it to another by offset, or to the last
     * one (`-n`, `-p`, `-l`). No id is the caller's session.
     *
     * @example
     * await session.focus("$1")
     */
    focus(session?: SessionId | null, opts?: {
    /** To so many sessions on (or back), in the order they were made, wrapping. */
    offset?: number | null;
    /** To the session used most recently before it. */
    last?: boolean;
  } | null): Promise<null>;
    /**
     * A session: its name, directory, views and active view; with `deep`, the whole tree in one call.
     * No id is the caller's: its tab's session, its client's, or the one used last.
     *
     * @example
     * await session.get()
     */
    get(session?: SessionId | null, opts?: {
    /** The whole tree: every view, each with its tabs. */
    deep?: boolean;
  } | null): Promise<{
    id: SessionId;
    name: string;
    /** Its directory, where its terminals start without a `cwd` of their own. */
    cwd?: string | null;
    /** Its views, in index order. */
    views: ViewId[];
    activeView?: ViewId | null;
    /** With `deep`: every view, each with its tabs. */
    tree?: ({
      id: ViewId;
      session: SessionId;
      /** The name it was given; `null` while it takes its active tab's title. */
      name?: string | null;
      /** Its index in its session. */
      index: number;
      /** A view's panes: a pane, or a split of two halves along an axis. */
      layout: {
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
        first: Layout;
        second: Layout;
      };
      /** The tab with the focus, in front of its pane. */
      activeTab?: TabId | null;
      /** The pane with the focus. */
      activePane?: PaneId | null;
      /** The pane zoomed to fill it. */
      zoom?: PaneId | null;
      tabs: ({
        id: TabId;
        session: SessionId;
        view?: ViewId | null;
        /** The block it holds (`file`, `owner.name.page`). */
        block: string;
        /** Each trait it has, with its fields: `{"pumi_dev.files.file": {path}}`. */
        traits: {
          [key: string]: unknown;
        };
        /** Where a tab is, and where `tab.open` and `tab.update` put one. */
        place: {
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
        };
        title?: string | null;
        /** Its directory: a terminal's program's. */
        cwd?: string | null;
        /** The directory of its foreground process. */
        foregroundCwd?: string | null;
        process?: {
          pid: number;
          /** Its program, as the system names it. */
          command: string[];
        } | null;
        /** How far a tab is scrolled back. */
        scroll: {
          offsetFromBottom: number;
        };
        /** The tab beside a tab on each side, in its view. */
        neighbors: {
          left?: TabId | null;
          right?: TabId | null;
          up?: TabId | null;
          down?: TabId | null;
        };
        /** It is the tab a client is in: its view's active tab, in its session's active view. */
        isFocused: boolean;
        /** It fills its view. */
        isZoomed: boolean;
        /** The options set on it. */
        options: {
          [key: string]: unknown;
        };
        /** With `state`: its saved state. */
        state?: {
          pumi_state: number;
          /** The block that saved it. */
          block: string;
          /** The version of that block's `data`. */
          block_version: number;
          /** The tab's own options, `@user` ones included. */
          options?: {
            [key: string]: unknown;
          };
          /** What the block's `serialize` returned. */
          data?: unknown;
          /** Large parts (scrollback, images), by name. */
          blobs?: {
            [key: string]: {
              /** How to read it: `vt` (terminal output), `png`, `bytes`. */
              encoding: string;
              size: number;
              /** `blob:<sha256>`. */
              ref: string;
              /** The bytes, in base64, when the state is to leave this server (`opts.blobs = "inline"`). */
              data?: string | null;
            };
          };
          /** What other plugins added in `TabSavePre`, by plugin. */
          extra?: {
            [key: string]: unknown;
          };
          /** How a state comes back. */
          strategy: "exact" | "relaunch" | "reference";
          /** What must be true where it is restored. */
          needs?: ({
            kind: "path";
            path: string;
          } | {
            kind: "program";
            name: string;
          } | {
            kind: "url";
            url: string;
          })[];
          origin?: {
            server: string;
            tab: number;
            /** RFC 3339, UTC. */
            at: string;
          } | null;
        } | null;
      })[];
    })[] | null;
  }>;
    /**
     * List every session, in creation order.
     *
     * @example
     * await session.list() // ["$1"]
     */
    list(): Promise<SessionId[]>;
    /**
     * Change a session's name or directory. Fires `session.changed` with what changed. No id is the
     * caller's session.
     *
     * @example
     * await session.update("$1", { name: "work" })
     */
    update(session: SessionId | null | undefined, patch: {
    /** Its name; `null` gives it a free one of its own (its folder's, else a number). */
    name?: string | null;
    /** Its directory, where its terminals start without a `cwd` of their own; `null` has none. */
    cwd?: string | null;
  }): Promise<null>;
  };
  tab: {
    /**
     * Call a method of the block in a tab: the protocol every block answers (`read`, its text;
     * `input`, `{text}`, `{paste}` or `{keys}`; `selection`; `report`, an OSC 7501 body), a
     * terminal tab's `run`, `wait` and `attach` (a block on the terminal engine), and the block's own
     * methods — a terminal's `respawn`, a plugin block's `<block>.<method>`, a file tab's
     * `<viewer>.<method>`. `tab` is the caller's own
     * when it is `null`.
     *
     * @example
     * await tab.call("%3", "read", { source: "visible", lines: 80 }) // { text: "ready\n" }
     */
    call(tab: TabId | null | undefined, method: string, args?: unknown): Promise<unknown>;
    /**
     * Close a tab. Its state is kept, so it can be reopened (`tab.list({closed: true})`). No id is
     * the caller's.
     *
     * @example
     * await tab.close("%3")
     */
    close(tab?: TabId | null): Promise<null>;
    /**
     * Make a tab its pane's front tab and its view's active tab, and its view and session current;
     * or move from it: to the next tab of its pane (`offset`), to the pane beside it (`direction`),
     * or to its view's tab used last (`last`). No id is the caller's tab. Moving out of a zoomed
     * pane unzooms the view; nothing there is not an error.
     *
     * @example
     * await tab.focus("%3")
     */
    focus(tab?: TabId | null, opts?: {
    /** To the tab in front of the pane beside its pane on this side. */
    direction?: "left" | "right" | "up" | "down" | null;
    /** To so many tabs on (or back, negative) in its pane's strip, wrapping. */
    offset?: number | null;
    /** To its view's tab used last. */
    last?: boolean;
  } | null): Promise<null>;
    /**
     * A tab: its block, traits, place, title, directories, process, neighbours, focus, zoom and
     * options; with `state`, its saved state. No id is the caller's: `PUMI_TAB` for a program in a
     * tab, else its view's active tab.
     *
     * @example
     * await tab.get()
     */
    get(tab?: TabId | null, opts?: {
    /** Its saved state, as persistence saves it (`true`), or saved as these say. */
    state?: boolean | {
      /**
       * What the state holds beyond its options and type data, instead of the `viewoptions`
       * option: `"scrollback,history"`.
       */
      viewOptions?: string | null;
      /** `restore` (the default), `copy`, `move` or `share`: a type leaves out what should not travel. */
      purpose?: "restore" | "copy" | "move" | "share" | null;
      /** `ref` keeps blobs on this server (the default); `inline` puts their bytes in the state. */
      blobs?: "ref" | "inline" | null;
    } | null;
  } | null): Promise<{
    id: TabId;
    session: SessionId;
    view?: ViewId | null;
    /** The block it holds (`file`, `owner.name.page`). */
    block: string;
    /** Each trait it has, with its fields: `{"pumi_dev.files.file": {path}}`. */
    traits: {
      [key: string]: unknown;
    };
    /** Where a tab is, and where `tab.open` and `tab.update` put one. */
    place: {
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
    };
    title?: string | null;
    /** Its directory: a terminal's program's. */
    cwd?: string | null;
    /** The directory of its foreground process. */
    foregroundCwd?: string | null;
    process?: {
      pid: number;
      /** Its program, as the system names it. */
      command: string[];
    } | null;
    /** How far a tab is scrolled back. */
    scroll: {
      offsetFromBottom: number;
    };
    /** The tab beside a tab on each side, in its view. */
    neighbors: {
      left?: TabId | null;
      right?: TabId | null;
      up?: TabId | null;
      down?: TabId | null;
    };
    /** It is the tab a client is in: its view's active tab, in its session's active view. */
    isFocused: boolean;
    /** It fills its view. */
    isZoomed: boolean;
    /** The options set on it. */
    options: {
      [key: string]: unknown;
    };
    /** With `state`: its saved state. */
    state?: {
      pumi_state: number;
      /** The block that saved it. */
      block: string;
      /** The version of that block's `data`. */
      block_version: number;
      /** The tab's own options, `@user` ones included. */
      options?: {
        [key: string]: unknown;
      };
      /** What the block's `serialize` returned. */
      data?: unknown;
      /** Large parts (scrollback, images), by name. */
      blobs?: {
        [key: string]: {
          /** How to read it: `vt` (terminal output), `png`, `bytes`. */
          encoding: string;
          size: number;
          /** `blob:<sha256>`. */
          ref: string;
          /** The bytes, in base64, when the state is to leave this server (`opts.blobs = "inline"`). */
          data?: string | null;
        };
      };
      /** What other plugins added in `TabSavePre`, by plugin. */
      extra?: {
        [key: string]: unknown;
      };
      /** How a state comes back. */
      strategy: "exact" | "relaunch" | "reference";
      /** What must be true where it is restored. */
      needs?: ({
        kind: "path";
        path: string;
      } | {
        kind: "program";
        name: string;
      } | {
        kind: "url";
        url: string;
      })[];
      origin?: {
        server: string;
        tab: number;
        /** RFC 3339, UTC. */
        at: string;
      } | null;
    } | null;
  }>;
    /**
     * List tabs, every one or those `opts` asks for: pane by pane in layout order within a view,
     * then floats and hidden tabs. `closed` lists the tabs closed lately instead.
     *
     * @example
     * await tab.list({}) // ["%1"]
     */
    list(opts?: {
    /** Only this session's. */
    session?: SessionId | null;
    /** Only this view's. */
    view?: ViewId | null;
    /** Only tabs that hold this block. */
    block?: string | null;
    /** Only tabs with this trait (`pumi_dev.files.file`): their option `@<trait>` is set. */
    trait?: string | null;
    /**
     * The tabs closed lately instead, newest first, each with its saved state and where it
     * was. Reopening one is `tab.open(state.block, {state}, {kind: "tiled", pane})` while its
     * pane is there, else `{kind: "tiled", view, beside}`.
     */
    closed?: boolean;
  } | null): Promise<TabId[] | ({
    tab: TabId;
    /** Everything needed to bring a tab back, here or on another server. */
    state: {
      pumi_state: number;
      /** The block that saved it. */
      block: string;
      /** The version of that block's `data`. */
      block_version: number;
      /** The tab's own options, `@user` ones included. */
      options?: {
        [key: string]: unknown;
      };
      /** What the block's `serialize` returned. */
      data?: unknown;
      /** Large parts (scrollback, images), by name. */
      blobs?: {
        [key: string]: {
          /** How to read it: `vt` (terminal output), `png`, `bytes`. */
          encoding: string;
          size: number;
          /** `blob:<sha256>`. */
          ref: string;
          /** The bytes, in base64, when the state is to leave this server (`opts.blobs = "inline"`). */
          data?: string | null;
        };
      };
      /** What other plugins added in `TabSavePre`, by plugin. */
      extra?: {
        [key: string]: unknown;
      };
      /** How a state comes back. */
      strategy: "exact" | "relaunch" | "reference";
      /** What must be true where it is restored. */
      needs?: ({
        kind: "path";
        path: string;
      } | {
        kind: "program";
        name: string;
      } | {
        kind: "url";
        url: string;
      })[];
      origin?: {
        server: string;
        tab: number;
        /** RFC 3339, UTC. */
        at: string;
      } | null;
    };
    /** Where it was: its view, its pane, and a tab beside its pane, if they are still there. */
    view?: ViewId | null;
    pane?: PaneId | null;
    beside?: TabId | null;
    /** When it closed, in RFC 3339. */
    at: string;
  })[]>;
    /**
     * Open a tab holding `block` (`null`: what a new tab holds, what the `new_tab` option names, a
     * block or a plugin's `tab.new` action; while it names nothing installed, the Launcher's list, a
     * shell or the first thing installed that starts one; refused while nothing is, as pumi has no
     * block of its own for one), placed as `place` says:
     * in a pane split from a tab's, into a pane
     * as one of its tabs (`{"pane": ":2"}`), in a new view or session, floating; or in an open tab
     * instead of its block (`{"kind": "tiled", "replace": "%1"}`), which
     * keeps its id, place, focus and options (`tab.changed {block}`). It is the one way anything
     * appears. `args` are the block's (a terminal block's `command`, `cwd` and `env`; a file's `path`;
     * a page's `url`), or `{"state": …}` to restore a saved state into the tab. Returns the tab's id.
     *
     * @example
     * await tab.open("pumi_dev.terminal.shell", { command: ["make", "test"] }, { kind: "tiled", split: "right" }) // "%3"
     */
    open(block?: string | null, args?: unknown, place?: {
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
  } | null): Promise<TabId>;
    /**
     * Change a tab: its title, place, size, zoom, whether it was seen, and its options. Each change
     * fires `tab.changed` (or its view's `view.changed`) with what changed. No id is the caller's tab.
     *
     * @example
     * await tab.update("%3", { title: "tests" })
     */
    update(tab: TabId | null | undefined, patch: {
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
     * its `@status` stops asking. The app says so when the tab is in front of the user.
     */
    seen?: boolean | null;
    /** Options to set on it; `null` unsets one. */
    options?: {
      [key: string]: unknown;
    } | null;
  }): Promise<null>;
  };
  view: {
    /**
     * Close a view and every tab in it. No id is the caller's.
     *
     * @example
     * await view.close("@1")
     */
    close(view?: ViewId | null): Promise<null>;
    /**
     * Make a view its session's active view; or move from it to another of its session by offset
     * (`next-window` is `{"offset": 1}`), by index, or the last one. No id is the caller's view.
     *
     * @example
     * await view.focus("@1")
     */
    focus(view?: ViewId | null, opts?: {
    /** To so many views on (or back, negative), wrapping. */
    offset?: number | null;
    /** To the session's view at this index. */
    index?: number | null;
    /** To the view used before the session's active one. */
    last?: boolean;
  } | null): Promise<null>;
    /**
     * A view: its session, name, index, layout of panes (each with its tabs), active tab and pane,
     * and zoom. No id is the caller's: its session's active view.
     *
     * @example
     * await view.get("@1") // { id: "@1", session: "$1", name: null, index: 0, layout: { pane: ":1", tabs: ["%1"], active: "%1" }, activeTab: "%1", activePane: ":1", zoom: null }
     */
    get(view?: ViewId | null): Promise<{
    id: ViewId;
    session: SessionId;
    /** The name it was given; `null` while it takes its active tab's title. */
    name?: string | null;
    /** Its index in its session. */
    index: number;
    /** A view's panes: a pane, or a split of two halves along an axis. */
    layout: {
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
      first: Layout;
      second: Layout;
    };
    /** The tab with the focus, in front of its pane. */
    activeTab?: TabId | null;
    /** The pane with the focus. */
    activePane?: PaneId | null;
    /** The pane zoomed to fill it. */
    zoom?: PaneId | null;
  }>;
    /**
     * Change a view: its name, index or session, layout, and zoom. Each change fires `view.changed`
     * with what changed. No id is the caller's view.
     *
     * @example
     * await view.update("@1", { name: "tests", zoom: ":3" })
     */
    update(view: ViewId | null | undefined, patch: {
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
      first: Layout;
      second: Layout;
    } | null;
    /** The pane zoomed to fill it, which gets its focus; `null` unzooms it. */
    zoom?: PaneId | null;
  }): Promise<null>;
  };
}

export type EventName = keyof Events;
export type FunctionName = keyof Functions;
export type TabMethodName = keyof TabMethods;
