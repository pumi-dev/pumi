// What Launchpad's list offers, apart from drawing it, so a test reads it without the app: a file
// always, then what a new tab can be, the `+` menu's list (`useNewTabs` from `pumi/theme`): the most
// used first, each with the letter that picks it in both lists (`C` for Claude Code). The list leaves
// itself out. A pumi before `useNewTabs` gives the `tab.new` slot's actions instead, numbered 1 to 9.

/** One row of the `+` menu's list, as `useNewTabs` gives it. */
export interface NewTab {
  id: string;
  title: string;
  icon: string;
  /** What `new_tab` names it by: an action's function, or a block's id. */
  value: string;
  letter: string;
  keys?: string;
  open(at: string, place: Place): Promise<unknown>;
}

/** One of the `tab.new` slot's actions, as the window gives it. */
export interface SlotAction {
  id: string;
  title: string;
  icon?: string;
  plugin: string;
  keys?: string;
  run(args?: unknown): void;
}

/** Where what is picked goes: in the list's place, the same tab. */
export interface Place {
  kind: "tiled";
  replace: string;
}

/** One row of the list. */
export interface Kind {
  id: string;
  title: string;
  icon: string;
  /** The key that picks it while the list is focused (`F`, `C`…), or none. */
  letter: string;
  /** The keys that open one elsewhere, when they are bound. */
  keys?: string;
  /** A rule is drawn between groups: the file, then the rest. */
  group: string;
  /** Opens it, from tab `at`, at `place`. */
  open(at: string, place: Place): Promise<unknown> | void;
}

/** This list's own block: a row that opens it would only open it again. */
export const SELF = "pumi.launchpad.new";

const file = (open: Kind["open"]): Kind => ({ id: "file", title: "File…", icon: "file", letter: "F", group: "file", open });

/** The rows: `file` first (it is the core's), then the `+` menu's in its order, but this list. */
export function kinds(list: NewTab[], openFile: Kind["open"]): Kind[] {
  const rows = list
    .filter((k) => k.value !== SELF)
    .map((k): Kind => ({ id: k.id, title: k.title, icon: k.icon, letter: k.letter, keys: k.keys, group: "new", open: (at, place) => k.open(at, place) }));
  return [file(openFile), ...rows];
}

/** The rows from the `tab.new` slot's actions, numbered in order: for a pumi without `useNewTabs`. */
export function fromSlot(actions: SlotAction[], openFile: Kind["open"]): Kind[] {
  const rows = actions.map((a, i): Kind => ({
    id: `${a.plugin}/${a.id}`,
    title: a.title,
    icon: a.icon ?? "plus",
    letter: i < 9 ? String(i + 1) : "",
    keys: a.keys,
    group: "new",
    open: (tab, place) => a.run({ tab, place }),
  }));
  return [file(openFile), ...rows];
}

/** The row a key picks: its letter, any case. */
export function byLetter(list: Kind[], key: string): Kind | undefined {
  const k = key.toLowerCase();
  return k ? list.find((row) => row.letter !== "" && row.letter.toLowerCase() === k) : undefined;
}
