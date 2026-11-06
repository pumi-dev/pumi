// What Launchpad's list offers, apart from drawing it, so a test reads it without the app: what a
// new tab can be, the `+` menu's list (`useNewTabs` from `pumi/theme`): the most used first, each
// with the letter that picks it in both lists (`C` for Claude Code). The list leaves itself out. A
// pumi before `useNewTabs` gives the `tab.new` slot's actions instead, numbered 1 to 9.

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
  /** The key that picks it while the list is focused (`C`, `T`…), or none. */
  letter: string;
  /** The keys that open one elsewhere, when they are bound. */
  keys?: string;
  /** Opens it, from tab `at`, at `place`. */
  open(at: string, place: Place): Promise<unknown> | void;
}

/** This list's own block: a row that opens it would only open it again. */
export const SELF = "pumi.launchpad.new";

/** The rows: the `+` menu's list in its order, but this list itself. */
export function kinds(list: NewTab[]): Kind[] {
  return list
    .filter((k) => k.value !== SELF)
    .map((k): Kind => ({ id: k.id, title: k.title, icon: k.icon, letter: k.letter, keys: k.keys, open: (at, place) => k.open(at, place) }));
}

/** The rows from the `tab.new` slot's actions, numbered in order: for a pumi without `useNewTabs`. */
export function fromSlot(actions: SlotAction[]): Kind[] {
  return actions.map((a, i): Kind => ({
    id: `${a.plugin}/${a.id}`,
    title: a.title,
    icon: a.icon ?? "plus",
    letter: i < 9 ? String(i + 1) : "",
    keys: a.keys,
    open: (tab, place) => a.run({ tab, place }),
  }));
}

/** The row a key picks: its letter, any case. */
export function byLetter(list: Kind[], key: string): Kind | undefined {
  const k = key.toLowerCase();
  return k ? list.find((row) => row.letter !== "" && row.letter.toLowerCase() === k) : undefined;
}
