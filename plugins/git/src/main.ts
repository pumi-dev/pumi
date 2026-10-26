// git: the repository in the left sidebar — its changes as a path tree you can stage, unstage,
// discard and commit from, and its history — for whichever repository the tab you are working in
// is in, on the `git` binary and a watch of the work tree.
//
//   ctrl+shift+g                    show the app's sidebar, which is where the page is
//   pumi call pumi_dev.git.toggle   the same from anywhere
//
// The repository is `panel.ts`, the rows `tree.ts`'s, the git work `git.ts`. What is here is the
// page's model and the exports `ui.tsx` calls. The page is pushed, not asked for: each time the
// panel changes the whole view is posted (`ui.post`, at most once a tick), and the window draws the
// last one it has the moment the sidebar opens — nothing is read on the way.
import { call, fn, on, option, paths, self, ui, workspace } from "pumi";

import { panel } from "./panel.ts";
import * as git from "./git.ts";
import type { Change, Commit, CommitMode, DiffSpec, Remote, Sync } from "./git.ts";
import { diffTotals, rows, stageStates, visible, type Checkbox, type Row } from "./tree.ts";

/** One line of the changes list: a group's header, its empty note, or a row of its tree. */
export interface Line {
  /** `group:staged`, `staged:file:src/main.ts`, `changes:dir:src`, `empty:staged`. */
  id: string;
  kind: "header" | "empty" | Row["kind"];
  depth: number;
  open: boolean;
  text: string;
  /** What git says of a file (`added`, `modified`, `deleted`, `renamed`, `untracked`, `conflict`). */
  status: string;
  checkbox: Checkbox;
  /** A header's count of files. */
  count: number;
  selected: boolean;
  /** What its menu offers, best first. */
  actions: string[];
}

/** The changes page: the repository, its lines, and what the user is doing to them. */
export interface ChangesView {
  version: number;
  root: string | null;
  branch: string;
  error: string | null;
  /** Before the first look there is nothing to say, and "no changes" would be a guess. */
  looked: boolean;
  lines: Line[];
  /** Every changed file, each counted once. */
  total: number;
  staged: number;
  /** Files with a change in the work tree that `commit -a` takes: tracked, not new. */
  tracked: number;
  notice: { ok: boolean; text: string } | null;
  filter: string;
  /** The draft in the commit box, for a box drawn again. */
  message: string;
  added: number;
  deleted: number;
  remote: Remote;
  busy: string | null;
}

/** The history page: the commits, newest first. */
export interface HistoryView {
  version: number;
  root: string | null;
  error: string | null;
  commits: Commit[] | null;
  dates: "relative" | "absolute";
  author: boolean;
}

export interface Views {
  changes: ChangesView;
  history: HistoryView;
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Rises with each post. It starts at the clock, not at 0, so a worker started again (a reload, a
 * crash) posts pages newer than what the last one answered, and the window draws them. */
let version = Date.now();

/** A group's lines: its header, then its tree, or the note that it is empty. */
function group(
  lines: Line[],
  key: "conflicts" | "staged" | "changes",
  label: string,
  all: readonly Change[],
  state: Checkbox,
  states: ReturnType<typeof stageStates>,
  empty: string | null,
): void {
  const kept = visible(all, panel.filter);
  if (kept.length === 0 && empty === null) return;
  const id = `group:${key}`;
  const open = panel.revealing || !panel.collapsed.has(id);
  lines.push({ id, kind: "header", depth: 0, open, text: label, status: "", checkbox: kept.length === 0 ? "" : state, count: kept.length, selected: panel.cursor === id, actions: [] });
  if (!open) return;
  if (kept.length === 0) {
    lines.push({ id: `empty:${key}`, kind: "empty", depth: 0, open: false, text: empty ?? "", status: "", checkbox: "", count: 0, selected: false, actions: [] });
    return;
  }
  const tree = rows(key === "staged" ? "staged" : "changes", kept, panel.collapsed, panel.revealing, panel.cursor, states.dirStage, states.both);
  for (const row of tree) {
    lines.push({ id: row.id, kind: row.kind, depth: row.depth, open: row.open, text: row.text, status: row.color, checkbox: row.checkbox, count: 0, selected: row.selected, actions: row.actions });
  }
}

/** The changes page as the panel has it now. */
function changesView(): ChangesView {
  const snapshot = panel.snapshot;
  const { staged, changes: unstaged, conflicts } = snapshot;
  const states = stageStates(staged, unstaged, conflicts);
  const lines: Line[] = [];
  const any = staged.length + unstaged.length + conflicts.length > 0;
  if (any) {
    group(lines, "conflicts", "Conflicts", conflicts, "", states, null);
    group(lines, "staged", "Staged", staged, staged.some((change) => states.both.has(change.path)) ? "partial" : "on", states, "No staged changes yet");
    group(lines, "changes", "Unstaged", unstaged, unstaged.some((change) => states.both.has(change.path)) ? "partial" : "off", states, "No unstaged changes");
  }
  const [added, deleted] = diffTotals(staged, unstaged, conflicts);
  return {
    version,
    root: panel.root,
    branch: snapshot.branch,
    error: snapshot.error,
    looked: panel.looked,
    lines,
    total: new Set([...staged, ...unstaged, ...conflicts].map((change) => change.path)).size,
    staged: staged.length,
    tracked: unstaged.filter((change) => change.glyph !== "?").length,
    notice: panel.notice,
    filter: panel.filter,
    message: panel.message,
    added,
    deleted,
    remote: snapshot.remote,
    busy: panel.busy,
  };
}

function historyView(): HistoryView {
  return {
    version,
    root: panel.root,
    error: panel.snapshot.error,
    commits: panel.snapshot.commits,
    dates: panel.settings.dates,
    author: panel.settings.author,
  };
}

function views(): Views {
  return { changes: changesView(), history: historyView() };
}

// ---- what the window hears

/** What the history was last posted with: it is posted again only when one of them changed. */
let postedHistory: { commits: Commit[] | null; dates: string; author: boolean; root: string | null } | null = null;
let queued = false;

/** Posts the page to the window, once however many times the panel changed in this tick. */
function post(): void {
  if (queued) return;
  queued = true;
  queueMicrotask(() => {
    queued = false;
    version += 1;
    ui.post("changes", changesView());
    const { commits } = panel.snapshot;
    const { dates, author } = panel.settings;
    const had = postedHistory;
    if (had === null || had.commits !== commits || had.dates !== dates || had.author !== author || had.root !== panel.root) {
      postedHistory = { commits, dates, author, root: panel.root };
      ui.post("history", historyView());
    }
  });
}
panel.watch(post);

// ---- what the window asks for

/** The user is in `cwd`: the repository around it, and the page as it is (for a window the posts
 * do not reach, and the first draw of one that has none yet). With no directory yet, the page as
 * it is, after the first look. */
export async function follow(cwd: string | null): Promise<Views> {
  if (cwd !== null) await panel.follow(cwd);
  else if (!panel.looked) await panel.refresh();
  return views();
}

/** A line pressed: a header or a folder folds, a file takes the cursor. */
export async function picked(id: string): Promise<void> {
  if (id.startsWith("group:") || id.includes(":dir:")) await panel.toggle(id);
  else panel.select(id);
}

/** A file line opened (double click): the file, in a pane beside the current one. */
export async function opened(id: string): Promise<void> {
  const at = id.indexOf(":file:");
  if (at >= 0) await panel.open(id.slice(at + ":file:".length));
}

/** An action of a line's menu or checkbox (`stage`, `unstage`, `discard`, `stage-all`, …). */
export async function chose(name: string, id: string): Promise<void> {
  await panel.act(name, id);
}

/** What is typed in the commit box: kept, not drawn (the box draws its own text). */
export function typed(text: string): void {
  panel.message = text;
}

/** Commits `text`: what is staged, every tracked change, or onto the last commit. */
export async function committed(text: string, mode: CommitMode): Promise<boolean> {
  return panel.commit(text, mode);
}

/** The branch button: fetch, pull, push or publish. */
export async function synced(what: Sync): Promise<void> {
  await panel.sync(what);
}

/** What the filter box holds, which is what the tree keeps. */
export function filtered(text: string): void {
  panel.filter = text;
  panel.changed();
}

/** The notice's close button: what an action said goes away, the repository as it is stays. */
export function dismissed(): void {
  panel.dismiss();
}

// ---- the diff tab

/** What a diff tab is told to show: the spec, and the repository it is of. */
export type Shown = DiffSpec & { root: string; title: string };

/** The diff tab the page opened last: the next diff is drawn in it, not in another — while the user
 * is still in that view. */
let diffPane: string | null = null;

/** Shows `spec` in the diff tab, opening one beside the tab the user is in when there is none. */
async function showDiff(spec: DiffSpec, title: string): Promise<void> {
  const root = panel.root;
  if (root === null) return;
  const value = JSON.stringify({ ...spec, root, title } satisfies Shown);
  try {
    const here = workspace.currentTab;
    const view = here?.place.kind === "tiled" ? here.place.view ?? null : null;
    if (diffPane !== null) {
      const there = workspace.getTab(diffPane);
      if (there && there.block === self.plugin.prefixed("diff") && there.place.kind === "tiled" && (view === null || there.place.view === view)) {
        await option.set("diff", value, { scope: "tab", id: diffPane });
        await option.set("title", title, { scope: "tab", id: diffPane });
        return;
      }
    }
    const opened = (await workspace.openTab(self.plugin.prefixed("diff"), {}, { kind: "view", name: title })).id;
    diffPane = opened;
    await option.set("diff", value, { scope: "tab", id: opened });
    await option.set("title", title, { scope: "tab", id: opened });
  } catch (err) {
    console.error(`git: diff: ${message(err)}`);
  }
}

/** The whole diff, staged and not, new files with it. */
export async function diff(): Promise<void> {
  await showDiff({ kind: "work", side: "all" }, "Changes");
}

/** A file line's diff: its staged side, or its work tree's (`staged:file:src/a.ts`). */
export async function diffed(id: string): Promise<void> {
  const at = id.indexOf(":file:");
  if (at < 0) return;
  const group = id.slice(0, at);
  const path = id.slice(at + ":file:".length);
  const side = group === "staged" ? "staged" : "changes";
  const change = (side === "staged" ? panel.snapshot.staged : [...panel.snapshot.changes, ...panel.snapshot.conflicts]).find((each) => each.path === path);
  const name = path.split("/").at(-1) ?? path;
  await showDiff({ kind: "work", side, path, untracked: change?.glyph === "?" }, `${name}${side === "staged" ? " (staged)" : ""}`);
}

/** A commit of the history, as a diff. */
export async function shown(sha: string): Promise<void> {
  if (!/^[0-9a-f]{4,64}$/.test(sha)) return;
  const commit = panel.snapshot.commits?.find((each) => each.sha === sha);
  await showDiff({ kind: "commit", sha }, commit ? `${sha.slice(0, 7)} ${commit.subject}` : sha.slice(0, 7));
}

/** The patch a diff tab draws, read now. The view asks again when the repository changes. */
export async function patchOf(value: string): Promise<{ patch: string; error: string | null }> {
  let spec: Shown;
  try {
    spec = JSON.parse(value) as Shown;
  } catch {
    return { patch: "", error: "nothing to show" };
  }
  try {
    return { patch: await git.patch(spec.root, spec), error: null };
  } catch (err) {
    return { patch: "", error: message(err) };
  }
}

/** A line of a diff pressed: the file, at that line, in the editor. */
export async function reveal(root: string, path: string, line: number | null): Promise<void> {
  try {
    await workspace.openTab("file", { path: `${root}/${path}`, ...(line === null ? {} : { line }) }, { kind: "tiled", split: "right" });
  } catch (err) {
    console.error(`git: open: ${message(err)}`);
  }
}

/** A key the page answers: the cursor, the filter, and a line's first action. Answers whether it
 * was one of them. */
export async function pressed(key: string): Promise<boolean> {
  switch (key) {
    case "/":
      filtered("");
      return true;
    case "escape": {
      // Escape backs out of what the page last put in front of the user: the filter, the cursor, and
      // then the notice under the toolbar, which is otherwise only dismissed by its close button.
      const had = panel.filter !== "" || panel.cursor !== null;
      panel.filter = "";
      panel.select(null);
      if (!had && panel.notice !== null) panel.dismiss();
      panel.changed();
      return true;
    }
    case "j":
    case "down":
      panel.select(step(1));
      return true;
    case "k":
    case "up":
      panel.select(step(-1));
      return true;
    case "enter":
    case "space": {
      const line = changesView().lines.find((each) => each.id === panel.cursor);
      if (!line) return false;
      if (line.kind === "header" || line.kind === "folder") await panel.toggle(line.id);
      else if (line.kind === "file") await panel.act(line.checkbox === "on" ? "unstage" : "stage", line.id);
      return true;
    }
    default:
      return false;
  }
}

/** The line `by` lines away from the cursor, in the order the page draws them. */
function step(by: number): string | null {
  const order = changesView()
    .lines.filter((line) => line.kind !== "note" && line.kind !== "empty")
    .map((line) => line.id);
  if (order.length === 0) return null;
  const at = panel.cursor === null ? -1 : order.indexOf(panel.cursor);
  const to = Math.min(order.length - 1, Math.max(0, at + by));
  return order[to] ?? null;
}

// ---- what the app reaches

/** `ctrl+shift+g`, as the app's own key does: the sidebar, which is where the page is. */
fn.set("toggle", { desc: "Open the git sidebar." }, () => call("toggle_sidebar"));

/** Its options changed: read them again, and draw with them. */
on("option.changed", { match: self.plugin.prefixed("*"), desc: "the git page's options" }, () => void panel.reread());

/** Starts the panel, with where its folded directories are kept, and reads the repository the
 * worker started in, so the page has something to draw before it is first shown. */
async function start(): Promise<void> {
  let state: string | null = null;
  try {
    state = paths.state;
  } catch (err) {
    // A host that gives packages no directories of their own: the folded directories are the run's only.
    console.error(`git: no state directory: ${message(err)}`);
  }
  await panel.start(state);
  await panel.follow(await directory());
}

/** The directory of the tab the user is in, as the window will ask for it when the page opens. */
async function directory(): Promise<string> {
  try {
    const current = workspace.currentTab;
    if (current === null) return process.cwd();
    const path = await option.get("tab_current_path", { scope: "tab", id: current.id });
    if (typeof path === "string" && path !== "") return path;
  } catch {
    // No tab in front yet: the worker's own directory.
  }
  return process.cwd();
}

void start().catch((err) => console.error(`git: starting threw:`, err));
