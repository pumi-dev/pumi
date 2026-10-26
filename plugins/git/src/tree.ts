// The changes as a path tree: the files of a group, in their directories, in the order git lists
// them and the order a person reads them — directories first, then files, each folded into one row
// while it holds nothing but the directory inside it.
//
// This file is the tree and nothing else: no git, no drawing. What a row is in the page — its
// indent, its mark, its checkbox — is `ui.tsx`.
import type { Change, Group } from "./git.ts";

/** A line of the tree: a directory, a file, or the note under a conflicted one. */
export interface Row {
  /** `staged:file:src/main.ts`, `changes:dir:src`, `changes:note:src/main.ts`. */
  id: string;
  kind: "file" | "folder" | "note";
  /** How many directories in it is. */
  depth: number;
  /** A directory's children show. */
  open: boolean;
  /** A file's letter (`A`, `M`, `U`, `?`); nothing for a directory. */
  glyph: string;
  /** Its colour token, for the mark and the note. */
  color: string;
  /** Whether git has it staged, and whether staging acts on all of it. */
  checkbox: Checkbox;
  /** The last part of its path, or the directory's folded name. */
  text: string;
  /** `+3 -1`, for a file git counts lines for. */
  stats: string;
  actions: string[];
  selected: boolean;
}

export type Checkbox = "" | "on" | "off" | "partial";

/** What one walk needs to know, carried down the tree. */
interface Walk {
  group: Group;
  /** The group's changes, which a node's `change` is an index into. */
  changes: readonly Change[];
  collapsed: ReadonlySet<string>;
  /** Every directory shows, whatever was folded: what a filter searching finds is in sight. */
  reveal: boolean;
  cursor: string | null;
  dirStage: ReadonlyMap<string, Checkbox>;
  both: ReadonlySet<string>;
  rows: Row[];
}

/** The rows of one group, as deep as the directories the user has left open. */
export function rows(
  group: Group,
  changes: readonly Change[],
  collapsed: ReadonlySet<string>,
  reveal: boolean,
  cursor: string | null,
  /** Each directory's staging state, by path (`on` when all of it is, `partial` when some). */
  dirStage: ReadonlyMap<string, Checkbox>,
  /** The files that are staged and changed again, which are `partial` on both sides. */
  both: ReadonlySet<string>,
): Row[] {
  const root = new Node();
  changes.forEach((change, index) => {
    const parts = change.path.split("/").filter((part) => part !== "");
    let level = root.children;
    parts.forEach((part, position) => {
      let node = level.get(part);
      if (!node) {
        node = new Node();
        level.set(part, node);
      }
      if (position + 1 === parts.length) node.change = index;
      level = node.children;
    });
  });
  const at: Walk = { group, changes, collapsed, reveal, cursor, dirStage, both, rows: [] };
  walk(at, root, "", 0);
  return at.rows;
}

/** One level of the tree: a directory before a file, then each by name — the order a map of them
 * gives when it is read sorted. */
function walk(at: Walk, node: Node, prefix: string, depth: number): void {
  const names = [...node.children.keys()].sort();
  const dirs = names.filter((name) => node.children.get(name)!.children.size > 0);
  const files = names.filter((name) => node.children.get(name)!.children.size === 0);
  for (const name of [...dirs, ...files]) {
    const child = node.children.get(name)!;
    if (child.children.size === 0) {
      const change = child.change === null ? undefined : at.changes[child.change];
      if (!change) continue;
      at.rows.push(fileRow(at, change, name, depth));
      if (change.note !== undefined) at.rows.push(noteRow(at.group, change.path, change.note, depth + 1));
      continue;
    }
    directory(at, child, name, prefix, depth);
  }
}

/** A directory's row, its name folded together with each single-child directory under it, then
 * its children when it is open. */
function directory(at: Walk, node: Node, name: string, prefix: string, depth: number): void {
  let label = name;
  let current = node;
  while (current.children.size === 1) {
    const [childName, child] = current.children.entries().next().value!;
    if (child.children.size === 0) break;
    label = `${label}/${childName}`;
    current = child;
  }
  const full = join(prefix, label);
  const id = `${at.group}:dir:${full}`;
  const open = at.reveal || !at.collapsed.has(id);
  at.rows.push({
    id,
    kind: "folder",
    depth,
    open,
    glyph: "",
    color: countChanges(current) > 0 ? "folderChanged" : "folder",
    checkbox: at.dirStage.get(full) ?? "off",
    text: label,
    stats: "",
    actions: [],
    selected: at.cursor === id,
  });
  if (open) walk(at, current, full, depth + 1);
}

/** A directory of the tree: what is in it, and the file it is, when it is one. */
class Node {
  children = new Map<string, Node>();
  /** The index in the group's changes of the file here, when a file is here. */
  change: number | null = null;
}

function fileRow(at: Walk, change: Change, name: string, depth: number): Row {
  const id = `${at.group}:file:${change.path}`;
  return {
    id,
    kind: "file",
    depth,
    open: false,
    glyph: change.glyph,
    color: change.color,
    checkbox: at.both.has(change.path) ? "partial" : at.group === "staged" ? "on" : "off",
    text: name,
    stats: change.stats ? `+${change.stats[0]} -${change.stats[1]}` : "",
    actions: change.actions,
    selected: at.cursor === id,
  };
}

function noteRow(group: Group, path: string, note: string, depth: number): Row {
  return {
    id: `${group}:note:${path}`,
    kind: "note",
    depth,
    open: false,
    glyph: "",
    color: "faint",
    checkbox: "",
    text: note,
    stats: "",
    actions: [],
    selected: false,
  };
}

function join(prefix: string, name: string): string {
  return prefix === "" ? name : `${prefix}/${name}`;
}

/** How many changes are at or under a node. */
function countChanges(node: Node): number {
  if (node.children.size === 0) return node.change === null ? 0 : 1;
  let count = 0;
  for (const child of node.children.values()) count += countChanges(child);
  return count;
}

/** Each directory's staging state, and the files that are staged and changed again — the two maps
 * the checkboxes of the tree read. */
export function stageStates(
  staged: readonly Change[],
  changes: readonly Change[],
  conflicts: readonly Change[],
): { dirStage: Map<string, Checkbox>; both: Set<string> } {
  const stagedPaths = new Set(staged.map((change) => change.path));
  const otherPaths = new Set([...changes, ...conflicts].map((change) => change.path));
  const both = new Set([...stagedPaths].filter((path) => otherPaths.has(path)));
  const totals = new Map<string, { all: number; fully: number }>();
  for (const path of [...stagedPaths, ...otherPaths]) {
    const fully = stagedPaths.has(path) && !otherPaths.has(path);
    const parts = path.split("/");
    let prefix = "";
    for (const part of parts.slice(0, Math.max(0, parts.length - 1))) {
      prefix = prefix === "" ? part : `${prefix}/${part}`;
      const entry = totals.get(prefix) ?? { all: 0, fully: 0 };
      entry.all += 1;
      if (fully) entry.fully += 1;
      totals.set(prefix, entry);
    }
  }
  const dirStage = new Map<string, Checkbox>();
  for (const [dir, { all, fully }] of totals) {
    dirStage.set(dir, fully === all ? "on" : fully === 0 ? "off" : "partial");
  }
  return { dirStage, both };
}

/** How many lines the changes add and delete, each file counted once — the toolbar's `+12 -3`. */
export function diffTotals(...groups: readonly (readonly Change[])[]): [number, number] {
  const seen = new Set<string>();
  let added = 0;
  let deleted = 0;
  for (const change of groups.flat()) {
    if (seen.has(change.path)) continue;
    seen.add(change.path);
    if (!change.stats) continue;
    added += change.stats[0];
    deleted += change.stats[1];
  }
  return [added, deleted];
}

/** The changes of a group whose path holds `filter`, all of them when it is empty. */
export function visible(changes: readonly Change[], filter: string): Change[] {
  if (filter === "") return [...changes];
  const needle = filter.toLowerCase();
  return changes.filter((change) => change.path.toLowerCase().includes(needle));
}

/** A path's file name: the last part. */
export function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}
