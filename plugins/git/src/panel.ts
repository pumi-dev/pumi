// The panel both pages read: the repository the current tab is in, what git says of it, the folded
// directories, the cursor, the filter and the draft in the commit message box. One of these runs
// per worker, and both views follow it: `main.ts` posts what it says to them each time it changes.
//
// It looks again when the work tree changes (`watch.ts`, minus what git ignores), when the window
// says the user is in another directory, and after every action it takes. A look that finds what
// the last one found changes nothing, so nothing is posted and nothing is drawn.
import { option, workspace } from "pumi";
import * as git from "./git.ts";
import type { Change, Commit, CommitMode, Remote, Sync } from "./git.ts";
import * as state from "./state.ts";
import { relevant, watch } from "./watch.ts";

/** Everything the pages draw, as one look at the repository. */
export interface Snapshot {
  staged: Change[];
  changes: Change[];
  conflicts: Change[];
  branch: string;
  /** The commit `HEAD` is at; empty in a repository with no commit yet. */
  headId: string;
  /** The branch against its upstream. */
  remote: Remote;
  /** The history, or `null` while it is being read. */
  commits: Commit[] | null;
  /** Why there is no repository, when there is none. */
  error: string | null;
}

const NO_REMOTE: Remote = { upstream: null, remote: null, ahead: 0, behind: 0 };

const EMPTY: Snapshot = {
  staged: [],
  changes: [],
  conflicts: [],
  branch: "",
  headId: "",
  remote: NO_REMOTE,
  commits: null,
  error: null,
};

/** What the two options say. */
export interface Settings {
  dates: "relative" | "absolute";
  author: boolean;
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** What a look found, as one string: two looks that say the same draw nothing new. */
function signature(s: Snapshot): string {
  return JSON.stringify([s.staged, s.changes, s.conflicts, s.branch, s.headId, s.remote, s.error]);
}

export class Panel {
  /** The directory the repository is looked for from: where the user is working. */
  cwd = "";
  /** The work tree's root, when the directory is in a repository. */
  root: string | null = null;
  snapshot: Snapshot = EMPTY;
  settings: Settings = { dates: "relative", author: true };
  /** The ids of the rows the tree has folded away (directories, and the groups' own headers). */
  collapsed = new Set<string>();
  cursor: string | null = null;
  filter = "";
  /** The last thing an action said, until the next one. */
  notice: { ok: boolean; text: string } | null = null;
  /** What is typed in the commit message box: the box's own, kept here for when it is drawn again. */
  message = "";
  /** What is running against the remote (`push`, `pull`, …), while it runs. */
  busy: string | null = null;
  /** The first look at the repository is behind it: before that the pages have nothing to say. */
  looked = false;

  /** The views drawing it now. */
  private readonly listeners = new Set<() => void>();
  private stopWatching: (() => void) | null = null;
  private watched: string | null = null;
  /** The directories git ignores, under which a write is not worth a look. */
  private ignored: string[] = [];
  /** Where its folded directories are kept; given at start, from the package's own state directory. */
  private stateDir: string | null = null;
  /** The look running now, or the last one. */
  private running: Promise<void> = Promise.resolve();
  /** The look asked for while one runs: every caller in between waits on this one. */
  private waiting: Promise<void> | null = null;
  private seen = "";
  /** The history is being read. */
  private reading = false;

  /** Calls `draw` whenever the panel changes; undo it when the view goes. */
  watch(draw: () => void): () => void {
    this.listeners.add(draw);
    return () => this.listeners.delete(draw);
  }

  /** Says it changed. */
  changed(): void {
    for (const draw of [...this.listeners]) draw();
  }

  /** Starts it: its options, and where its folded directories are kept (`null`: nowhere). */
  async start(stateDir: string | null): Promise<void> {
    this.stateDir = stateDir;
    await this.reread();
  }

  /** The options changed: read them again, and draw with them. */
  async reread(): Promise<void> {
    this.settings = await read();
    this.changed();
  }

  /** The user is in `cwd` now: looks at the repository around it, when it is another directory. */
  async follow(cwd: string): Promise<void> {
    if (cwd === this.cwd && this.looked) return;
    this.cwd = cwd;
    await this.refresh();
  }

  /** Looks at the repository again, and draws it when it is not as it was. Resolves once a look
   * that began after the call is done, so what is answered then is of the directory asked for. */
  refresh(): Promise<void> {
    if (this.waiting) return this.waiting;
    const next = this.running.then(async () => {
      this.waiting = null;
      try {
        await this.scan();
      } catch (err) {
        console.error(`git: ${message(err)}`);
      }
    });
    this.waiting = next;
    this.running = next;
    return next;
  }

  private async scan(): Promise<void> {
    const previous = this.root;
    let next: Snapshot;
    let root: string | null = null;
    try {
      root = await git.repository(this.cwd || process.cwd());
      const [collected, headId, branch, remote] = await Promise.all([git.collect(root), git.headId(root), git.branch(root), git.remote(root)]);
      // A file git cannot merge is a conflict of its own, above the two groups; the rest of the
      // work tree's side is the Unstaged group.
      const conflicts = collected.changes.filter((change) => change.color === "conflict");
      const changes = collected.changes.filter((change) => change.color !== "conflict");
      // The history is worth keeping while the commit it was read at is the one there is now.
      const commits = root === previous && headId === this.snapshot.headId ? this.snapshot.commits : null;
      next = { staged: collected.staged, changes, conflicts, branch, headId, remote, commits, error: null };
    } catch (err) {
      next = { ...EMPTY, commits: [], error: message(err) };
    }
    const moved = root !== previous;
    this.root = root;
    if (moved) {
      this.collapsed = await this.folded();
      this.cursor = null;
      this.filter = "";
      this.notice = null;
      this.followRoot();
    }
    const said = signature(next);
    const same = !moved && this.looked && said === this.seen;
    this.seen = said;
    this.looked = true;
    if (!same) {
      this.snapshot = next;
      this.changed();
    }
    if (this.snapshot.commits === null && !this.reading) void this.loadHistory();
  }

  /** Reads the history, which a big repository takes a moment over, and draws it. */
  private async loadHistory(): Promise<void> {
    const root = this.root;
    const head = this.snapshot.headId;
    if (root === null) return;
    let commits: Commit[] = [];
    this.reading = true;
    try {
      commits = await git.history(root);
    } catch (err) {
      console.error(`git: history: ${message(err)}`);
    } finally {
      this.reading = false;
    }
    // Read again for the commit there is now, when one was made while it read.
    if (this.root === root && this.snapshot.headId !== head && this.snapshot.commits === null) {
      void this.loadHistory();
      return;
    }
    // A commit in between: what was read is out of date, and the next look reads it again.
    if (this.root !== root || this.snapshot.headId !== head || this.snapshot.commits !== null) return;
    this.snapshot = { ...this.snapshot, commits };
    this.changed();
  }

  /** The directories the repository's tree had folded, from the last run. */
  private async folded(): Promise<Set<string>> {
    if (this.root === null || this.stateDir === null) return new Set();
    return state.load(this.stateDir, this.root);
  }

  /** Watches the repository now, and stops watching the one before it. */
  private followRoot(): void {
    if (this.watched === this.root) return;
    this.stopWatching?.();
    this.stopWatching = null;
    this.watched = this.root;
    this.ignored = [];
    const root = this.root;
    if (root === null) return;
    const learn = () =>
      void git
        .ignoredDirs(root)
        .then((dirs) => {
          if (this.root === root) this.ignored = dirs;
        })
        .catch(() => undefined);
    learn();
    this.stopWatching = watch(root, (paths) => {
      if (paths.some((path) => path === ".gitignore" || path.endsWith("/.gitignore"))) learn();
      if (relevant(paths, this.ignored)) void this.refresh();
    });
  }

  /** Folds a directory's children (or a group's rows) away, or opens them, and keeps the choice. */
  async toggle(id: string): Promise<void> {
    if (!this.collapsed.delete(id)) this.collapsed.add(id);
    this.cursor = id;
    this.changed();
    if (this.root !== null && this.stateDir !== null) {
      await state.save(this.stateDir, this.root, this.collapsed).catch((err) => console.error(`git: ${message(err)}`));
    }
  }

  /** Puts the cursor on a row, and draws it there. */
  select(id: string | null): void {
    if (this.cursor === id) return;
    this.cursor = id;
    this.changed();
  }

  /** Whether the tree is showing what the filter found, every directory open. */
  get revealing(): boolean {
    return this.filter !== "";
  }

  /** What an action says it did, for the notice under the toolbar. */
  private say(ok: boolean, text: string): void {
    this.notice = { ok, text };
  }

  /** Puts the notice away, without doing anything about what it says. */
  dismiss(): void {
    if (this.notice === null) return;
    this.notice = null;
    this.changed();
  }

  /**
   * Runs the action `name` on row `id`, which is `staged:file:src/main.ts`, `changes:dir:src`, or
   * nothing for the whole repository.
   */
  async act(name: string, id: string): Promise<void> {
    if (name === "stage-all" || name === "unstage-all") {
      await this.wholeTree(name);
      return;
    }
    const file = filePath(id);
    const dir = dirPath(id);
    if (file === null && dir === null) return;
    if (file === null && name !== "stage" && name !== "unstage") return;
    const target = file ?? dir!;
    const root = this.root;
    if (root === null) return;
    try {
      await run(root, name, target);
      this.notice = null;
    } catch (err) {
      this.say(false, `${name} failed: ${message(err)}`);
      this.changed();
    }
    await this.refresh();
  }

  /** Stage or unstage the whole repository. */
  private async wholeTree(name: "stage-all" | "unstage-all"): Promise<void> {
    const root = this.root;
    if (root === null) return;
    try {
      if (name === "stage-all") await git.stageAll(root);
      else await git.unstageAll(root);
      this.notice = null;
    } catch (err) {
      this.say(false, `${name} failed: ${message(err)}`);
      this.changed();
    }
    await this.refresh();
  }

  /** Commits with `text`, as `mode` says. Answers whether it did. */
  async commit(text: string, mode: CommitMode): Promise<boolean> {
    const root = this.root;
    const trimmed = text.trim();
    if (root === null) return false;
    if (trimmed === "" && mode !== "amend") {
      this.say(false, "a commit needs a message");
      this.changed();
      return false;
    }
    try {
      const sha = await git.commit(root, trimmed, mode);
      this.message = "";
      this.say(true, `${mode === "amend" ? "amended" : "committed"} ${git.short(sha)}`);
      await this.refresh();
      return true;
    } catch (err) {
      this.say(false, `commit failed: ${message(err)}`);
      this.changed();
      return false;
    }
  }

  /** Fetches, pulls, pushes or publishes the branch; the button says so while it runs. */
  async sync(what: Sync): Promise<void> {
    const root = this.root;
    if (root === null || this.busy !== null) return;
    this.busy = what;
    this.changed();
    try {
      await git.sync(root, what, this.snapshot.remote.remote);
      this.notice = null;
    } catch (err) {
      this.say(false, `${what} failed: ${message(err)}`);
    } finally {
      this.busy = null;
      this.changed();
    }
    await this.refresh();
  }

  /** Opens a file of the work tree in a pane beside the current tab, without taking the keyboard. */
  async open(path: string): Promise<void> {
    const root = this.root;
    if (root === null) return;
    try {
      await workspace.openTab("file", { path: `${root}/${path}` }, { kind: "tiled", split: "right", focus: false });
    } catch (err) {
      this.say(false, `open failed: ${message(err)}`);
      this.changed();
    }
  }
}

/** What the actions do to a path on disk, one git call each. */
async function run(root: string, name: string, path: string): Promise<void> {
  switch (name) {
    case "stage":
      return git.stage(root, path);
    case "unstage":
      return git.unstage(root, path);
    case "discard":
      return git.discard(root, path);
    case "ignore":
      return git.ignore(root, path);
    case "ours":
      return git.resolve(root, "ours", path);
    case "theirs":
      return git.resolve(root, "theirs", path);
    default:
      throw new Error(`unknown action ${name}`);
  }
}

/** The path of a file row (`staged:file:src/main.ts`), or nothing when the row is not one. */
export function filePath(id: string): string | null {
  const parts = id.split(":");
  if (parts.length < 3) return null;
  const path = parts.slice(2).join(":");
  return parts[1] === "file" && path !== "" ? path : null;
}

/** The path of a directory row (`changes:dir:src`), or nothing when the row is not one. */
export function dirPath(id: string): string | null {
  const parts = id.split(":");
  if (parts.length < 3) return null;
  const path = parts.slice(2).join(":");
  return parts[1] === "dir" && path !== "" ? path : null;
}

/** The two options, as they are now. */
async function read(): Promise<Settings> {
  const [format, author] = await Promise.all([option.get("date_format").catch(() => null), option.get("show_author").catch(() => null)]);
  return {
    dates: format === "absolute" ? "absolute" : "relative",
    author: author !== false,
  };
}

/** The panel the worker runs: one, whichever pages are open. */
export const panel = new Panel();
