// The git work: the repository a directory is in, its status, its history, and the one action a
// row asks for. Every answer comes from the `git` binary run in the work tree's root (Bun.spawn);
// `git.ts` knows what porcelain means, and `tree.ts` draws what it finds.
//
// Nothing here invents a repository: a directory that is not one, a repository with no commits, a
// `git` that refuses an action — each comes back as `git`'s own words, which the page shows.

/** A file the repository has something to say about, in one group of the page. */
export interface Change {
  /** Its path, from the root, `/`-separated. */
  path: string;
  /** The letter git gives it: `A`, `M`, `D`, `R`, `U`, `?`. */
  glyph: string;
  /** The colour token its mark is drawn in: `status.added`, `status.conflict`, … */
  color: string;
  /** The lines it adds and deletes, when git counts them. */
  stats?: [number, number];
  /** What the row can do, best first: `unstage`, `stage`, `discard`, `ignore`, `ours`, `theirs`. */
  actions: string[];
  /** A conflict's sides, under its row. */
  note?: string;
}

/** A commit in the history. */
export interface Commit {
  sha: string;
  /** Its first line. */
  subject: string;
  /** Its author's name. */
  author: string;
  /** When it was authored, in seconds since the epoch. */
  timestamp: number;
  /** The tags that point at it. */
  tags: string[];
  /** It is not on the branch's upstream yet. */
  unpushed: boolean;
}

/** Which half of the page a change is in; also the start of its row's id. */
export type Group = "staged" | "changes";

/** Which side of a conflict to take. */
export type Side = "ours" | "theirs";

/** What `git` said, as the page says it. */
export class GitError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "GitError";
  }
}

/** What a git call answered: its exit code and its two streams. */
interface Answer {
  code: number;
  out: string;
  err: string;
}

const COMMAND = "git";

/** git's environment: the one this package was started in, with its optional index writes off, so
 * a status never takes the index's lock from a commit the user is making. */
const ENV: Record<string, string | undefined> = { ...process.env, GIT_OPTIONAL_LOCKS: "0", GIT_TERMINAL_PROMPT: "0" };

/** Runs git in `dir`; what it wrote, and how it ended. */
async function run(dir: string, args: string[]): Promise<Answer> {
  let proc: Bun.Subprocess<"ignore", "pipe", "pipe">;
  try {
    proc = Bun.spawn([COMMAND, ...args], { cwd: dir, env: ENV, stdin: "ignore", stdout: "pipe", stderr: "pipe" });
  } catch (err) {
    throw new GitError(`cannot run ${COMMAND}: ${message(err)}`);
  }
  const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  return { code, out, err };
}

/** Runs git in `dir` and gives its standard output; a non-zero exit is `GitError` with its own
 * words (the last line of its standard error, which is where git puts the reason). */
async function ok(dir: string, args: string[]): Promise<string> {
  const answer = await run(dir, args);
  if (answer.code !== 0) throw new GitError(reason(answer));
  return answer.out;
}

/** git's reason, as one line: what it said last on its standard error, or on its standard output
 * when it said nothing there (a commit with nothing to commit explains itself on the latter), with
 * the `fatal:`/`error:` prefixes it puts in front taken off. */
function reason(answer: Answer): string {
  const said = (text: string) =>
    text
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line !== "");
  const last = said(answer.err).at(-1) ?? said(answer.out).at(-1) ?? `git exited ${answer.code}`;
  return last.replace(/^(fatal|error|warning):\s*/, "");
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** The work tree's root that `dir` is in. */
export async function repository(dir: string): Promise<string> {
  const out = await ok(dir, ["rev-parse", "--show-toplevel"]);
  const root = out.trim();
  if (root === "") throw new GitError("a bare repository has no work tree to show");
  return root;
}

// ---- what there is

/** The repository's changes, in the two groups the page shows them in. */
export async function collect(root: string): Promise<{ staged: Change[]; changes: Change[] }> {
  const [status, staged, worktree, unmerged] = await Promise.all([
    ok(root, ["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
    counts(root, ["diff", "--cached", "--numstat", "--no-renames", "-z"]),
    counts(root, ["diff", "--numstat", "--no-renames", "-z"]),
    conflicts(root),
  ]);
  const stagedChanges: Change[] = [];
  const worktreeChanges: Change[] = [];
  for (const entry of entries(status)) {
    const { path, index, work } = entry;
    const conflicted = isConflict(index, work);
    const inIndex = conflicted ? null : stagedStatus(index);
    if (inIndex) {
      stagedChanges.push({
        path,
        glyph: inIndex[0],
        color: inIndex[1],
        stats: staged.get(path),
        actions: ["unstage"],
      });
    }
    const inWorktree = conflicted ? (["U", "conflict"] as const) : worktreeStatus(work);
    if (!inWorktree) continue;
    const actions = ["stage"];
    if (conflicted) actions.push("ours", "theirs");
    else if (work === "?") actions.push("ignore");
    else actions.push("discard");
    worktreeChanges.push({
      path,
      glyph: inWorktree[0],
      color: inWorktree[1],
      stats: worktree.get(path),
      actions,
      ...(conflicted && unmerged.has(path) ? { note: unmerged.get(path)! } : {}),
    });
  }
  return { staged: stagedChanges, changes: worktreeChanges };
}

/** One line of `git status --porcelain=v1 -z`: a path, and the two columns git gives it. */
interface Entry {
  path: string;
  /** The index's column (`X`). */
  index: string;
  /** The work tree's column (`Y`). */
  work: string;
  /** Where the file was, for a rename or a copy. */
  from?: string;
}

/** `status -z`'s records: `XY path`, NUL-terminated, a rename's origin in the record after it. */
export function entries(status: string): Entry[] {
  const fields = status.split("\0");
  const found: Entry[] = [];
  for (let at = 0; at < fields.length; at++) {
    const record = fields[at]!;
    if (record.length < 4) continue;
    const index = record.slice(0, 1);
    const work = record.slice(1, 2);
    const path = record.slice(3);
    // A rename or a copy names the file it was before, in the record after it.
    const from = index === "R" || index === "C" || work === "R" || work === "C" ? fields[++at] : undefined;
    found.push(from === undefined ? { path, index, work } : { path, index, work, from });
  }
  return found;
}

/** Whether git put a file in both columns because it could not merge it. */
function isConflict(index: string, work: string): boolean {
  return index === "U" || work === "U" || (index === "A" && work === "A") || (index === "D" && work === "D");
}

/** The staged half of a file's status, as the glyph and colour of its row. */
function stagedStatus(index: string): [string, string] | null {
  switch (index) {
    case "A":
      return ["A", "added"];
    case "M":
    case "T":
      return ["M", "modified"];
    case "D":
      return ["D", "deleted"];
    case "R":
    case "C":
      return ["R", "renamed"];
    default:
      return null;
  }
}

/** The work tree's half of a file's status, as the glyph and colour of its row. */
function worktreeStatus(work: string): [string, string] | null {
  switch (work) {
    case "?":
      return ["?", "untracked"];
    case "M":
    case "T":
      return ["M", "modified"];
    case "D":
      return ["D", "deleted"];
    case "R":
    case "C":
      return ["R", "renamed"];
    default:
      return null;
  }
}

/** The lines each file adds and deletes in a `--numstat -z` diff, by path; a file git counts
 * nothing for (a rename only, a binary) is left out. */
async function counts(root: string, args: string[]): Promise<Map<string, [number, number]>> {
  const found = new Map<string, [number, number]>();
  const answer = await run(root, args);
  if (answer.code !== 0) return found;
  for (const record of answer.out.split("\0")) {
    const [added, deleted, path] = record.split("\t");
    if (path === undefined || path === "") continue;
    const plus = Number(added);
    const minus = Number(deleted);
    if (!Number.isFinite(plus) || !Number.isFinite(minus)) continue;
    if (plus === 0 && minus === 0) continue;
    found.set(path, [plus, minus]);
  }
  return found;
}

/** Each conflicted path's sides, as the note under its row: `UU · ours abc1234 · theirs def5678`. */
async function conflicts(root: string): Promise<Map<string, string>> {
  const notes = new Map<string, string>();
  const answer = await run(root, ["ls-files", "--unmerged", "-z"]);
  if (answer.code !== 0) return notes;
  const sides = new Map<string, { ancestor?: string; ours?: string; theirs?: string }>();
  for (const record of answer.out.split("\0")) {
    // `mode SP sha SP stage TAB path`
    const tab = record.indexOf("\t");
    if (tab < 0) continue;
    const [mode, sha, stage] = record.slice(0, tab).split(" ");
    const path = record.slice(tab + 1);
    if (!mode || !sha || !stage) continue;
    const at = sides.get(path) ?? {};
    if (stage === "1") at.ancestor = sha;
    else if (stage === "2") at.ours = sha;
    else if (stage === "3") at.theirs = sha;
    sides.set(path, at);
  }
  for (const [path, at] of sides) {
    const note = `${conflictCode(at)} · ours ${at.ours ? short(at.ours) : "deleted"} · theirs ${
      at.theirs ? short(at.theirs) : "deleted"
    }`;
    notes.set(path, note);
  }
  return notes;
}

/** git's two letters for a conflict, from the sides it has. */
function conflictCode(at: { ancestor?: string; ours?: string; theirs?: string }): string {
  const key = `${at.ours ? "o" : "-"}${at.theirs ? "t" : "-"}${at.ancestor ? "a" : "-"}`;
  switch (key) {
    case "---":
      return "DD";
    case "o--":
      return "AU";
    case "o-a":
      return "UD";
    case "-t-":
      return "UA";
    case "-ta":
      return "DU";
    case "ot-":
      return "AA";
    default:
      return "UU";
  }
}

/** The branch the work tree is on; `HEAD` when it is on none, and the branch it is about to make
 * when the repository has no commit yet. */
export async function branch(root: string): Promise<string> {
  const named = await run(root, ["rev-parse", "--abbrev-ref", "HEAD"]);
  if (named.code === 0 && named.out.trim() !== "") return named.out.trim();
  const unborn = await run(root, ["symbolic-ref", "--short", "HEAD"]);
  return unborn.code === 0 && unborn.out.trim() !== "" ? unborn.out.trim() : "HEAD";
}

/** The commit `HEAD` is at, or nothing when the repository has no commit yet. */
export async function headId(root: string): Promise<string> {
  const answer = await run(root, ["rev-parse", "--verify", "HEAD"]);
  return answer.code === 0 ? answer.out.trim() : "";
}

/** How many commits the history reads: enough to scroll through, never the whole of a big one. */
export const HISTORY_LIMIT = 1000;

/** The repository's commits, newest first, the ones the branch's upstream does not have marked. */
export async function history(root: string): Promise<Commit[]> {
  if ((await headId(root)) === "") return [];
  // A record a line, its fields between them: the subject may hold anything but these.
  // git's own order: the newest commit first, which is the order a revwalk sorted by time gives.
  const log = await run(root, ["log", `--max-count=${HISTORY_LIMIT}`, "--format=%H%x1f%an%x1f%at%x1f%s"]);
  if (log.code !== 0) return [];
  const tags = await tagsByCommit(root);
  const ahead = await aheadCount(root);
  const commits: Commit[] = [];
  for (const line of log.out.split("\n")) {
    if (line === "") continue;
    const [sha, author, when, subject] = line.split("\x1f");
    if (!sha) continue;
    commits.push({
      sha,
      subject: subject ?? "",
      author: author ?? "",
      timestamp: Number(when ?? 0) || 0,
      tags: tags.get(sha) ?? [],
      unpushed: commits.length < ahead,
    });
  }
  return commits;
}

/** The tags that point at each commit, by its sha: `%(objectname)` is an annotated tag's own
 * object, `%(*objectname)` the commit it is on. */
async function tagsByCommit(root: string): Promise<Map<string, string[]>> {
  const tags = new Map<string, string[]>();
  const answer = await run(root, ["for-each-ref", "--format=%(objectname)%09%(*objectname)%09%(refname:short)", "refs/tags"]);
  if (answer.code !== 0) return tags;
  for (const line of answer.out.split("\n")) {
    if (line === "") continue;
    const [object, peeled, name] = line.split("\t");
    if (!name) continue;
    const commit = peeled && peeled !== "" ? peeled : object;
    if (!commit) continue;
    const at = tags.get(commit) ?? [];
    at.push(name);
    tags.set(commit, at);
  }
  return tags;
}

/** How many commits the branch has that its upstream does not; none when it has no upstream. */
async function aheadCount(root: string): Promise<number> {
  const upstream = await run(root, ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]);
  if (upstream.code !== 0 || upstream.out.trim() === "") return 0;
  const count = await run(root, ["rev-list", "--count", `${upstream.out.trim()}..HEAD`]);
  if (count.code !== 0) return 0;
  const ahead = Number(count.out.trim());
  return Number.isFinite(ahead) ? ahead : 0;
}

/** Where the branch stands against its upstream: the remote it would go to, and how far apart. */
export interface Remote {
  /** `origin/main`, or `null` when the branch has none. */
  upstream: string | null;
  /** The remote a branch without an upstream is published to; `null` when there is no remote. */
  remote: string | null;
  ahead: number;
  behind: number;
}

/** The branch against its upstream, in two git calls (one when there is no upstream). */
export async function remote(root: string): Promise<Remote> {
  const [upstream, remotes] = await Promise.all([
    run(root, ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]),
    run(root, ["remote"]),
  ]);
  const names = remotes.code === 0 ? remotes.out.split("\n").map((line) => line.trim()).filter((line) => line !== "") : [];
  const target = names.includes("origin") ? "origin" : (names[0] ?? null);
  const name = upstream.code === 0 ? upstream.out.trim() : "";
  if (name === "") return { upstream: null, remote: target, ahead: 0, behind: 0 };
  const counts = await run(root, ["rev-list", "--left-right", "--count", `${name}...HEAD`]);
  const [behind, ahead] = counts.code === 0 ? counts.out.trim().split(/\s+/).map(Number) : [0, 0];
  return { upstream: name, remote: target, ahead: Number.isFinite(ahead) ? ahead! : 0, behind: Number.isFinite(behind) ? behind! : 0 };
}

/** The directories git ignores that hold something (`target/`, `node_modules/`): a write under one
 * says nothing the page shows, and a build writes thousands. */
export async function ignoredDirs(root: string): Promise<string[]> {
  const answer = await run(root, ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"]);
  if (answer.code !== 0) return [];
  return answer.out.split("\0").filter((path) => path.endsWith("/"));
}

// ---- what a row does

/** Stage `path` (a file, or everything under a directory), deletions included. */
export async function stage(root: string, path: string): Promise<void> {
  await ok(root, ["add", "-A", "--", path]);
}

/** Stage everything the work tree changed. */
export async function stageAll(root: string): Promise<void> {
  await ok(root, ["add", "-A"]);
}

/** Take `path` back out of the index, to the commit `HEAD` is at. */
export async function unstage(root: string, path: string): Promise<void> {
  await reset(root, ["--", path]);
}

/** Take everything back out of the index. */
export async function unstageAll(root: string): Promise<void> {
  await reset(root, []);
}

/** `git reset` to `HEAD`; before the first commit there is no HEAD to reset to, so an older git
 * refuses, and the index is emptied by hand instead. */
async function reset(root: string, paths: string[]): Promise<void> {
  const answer = await run(root, ["reset", "-q", "HEAD", ...paths]);
  if (answer.code === 0) return;
  // With no path given, the whole index is meant, which is `.`.
  const cached = await run(root, ["rm", "-q", "--cached", "-r", "--ignore-unmatch", "--", ...(paths.length === 0 ? ["."] : paths)]);
  if (cached.code === 0) return;
  throw new GitError(reason(answer));
}

/** Throw away the work tree's changes to `path`: what the index has, it has again. */
export async function discard(root: string, path: string): Promise<void> {
  await ok(root, ["checkout-index", "--force", "--", path]);
}

/** Resolve `path`'s conflict by taking one side, and stage it. */
export async function resolve(root: string, side: Side, path: string): Promise<void> {
  await ok(root, ["checkout", `--${side}`, "--", path]);
  await stage(root, path);
}

/** Add `path` to the repository's `.gitignore`, unless the rule is already there. */
export async function ignore(root: string, path: string): Promise<void> {
  const file = `${root}/.gitignore`;
  const rule = `/${path}`;
  const existing = (await Bun.file(file).exists()) ? await Bun.file(file).text() : "";
  if (existing.split("\n").some((line) => line.trim() === rule)) return;
  const text = existing === "" || existing.endsWith("\n") ? existing : `${existing}\n`;
  await Bun.write(file, `${text}${rule}\n`);
}

/** How a commit is made: what is staged, every tracked file's changes (`-a`), or onto the last one. */
export type CommitMode = "staged" | "tracked" | "amend";

/** Commit with `message`, as `mode` says; the commit it made. An amend with no message keeps the
 * last one's. */
export async function commit(root: string, message: string, mode: CommitMode = "staged"): Promise<string> {
  const args = ["commit", "--quiet"];
  if (mode === "tracked") args.push("--all");
  if (mode === "amend") args.push("--amend");
  if (message !== "") args.push("--message", message);
  else if (mode === "amend") args.push("--no-edit");
  await ok(root, args);
  return await headId(root);
}

/** What the branch button does with the remote. */
export type Sync = "fetch" | "pull" | "push" | "publish";

/** Fetch, pull (fast-forward only), push, or publish the branch to `target` and track it there. */
export async function sync(root: string, what: Sync, target: string | null): Promise<void> {
  switch (what) {
    case "fetch":
      await ok(root, ["fetch", "--quiet"]);
      return;
    case "pull":
      await ok(root, ["pull", "--ff-only", "--quiet"]);
      return;
    case "push":
      await ok(root, ["push", "--quiet"]);
      return;
    case "publish":
      if (target === null) throw new GitError("no remote to publish to");
      await ok(root, ["push", "--quiet", "--set-upstream", target, "HEAD"]);
      return;
  }
}

/** The first seven characters of a sha, as the page names a commit. */
export function short(sha: string): string {
  return sha.slice(0, 7);
}

/** What a diff tab shows: one side of the work tree (all of it, or a file of it), or a commit. */
export type DiffSpec =
  | { kind: "work"; side: "staged" | "changes" | "all"; path?: string; untracked?: boolean }
  | { kind: "commit"; sha: string };

/** Untracked files the whole-tree diff reads as additions; past this, they are counted, not read. */
export const UNTRACKED_LIMIT = 200;

const DIFF_FLAGS = ["--no-color", "--no-ext-diff", "--find-renames", "--patch"];

/** A unified patch for `spec`, as `git diff` writes it, for the `<diff>` element to draw. */
export async function patch(root: string, spec: DiffSpec): Promise<string> {
  if (spec.kind === "commit") {
    if (!/^[0-9a-f]{4,64}$/.test(spec.sha)) throw new GitError(`not a commit: ${spec.sha}`);
    return ok(root, ["show", "--format=", ...DIFF_FLAGS, spec.sha]);
  }
  const paths = spec.path ? ["--", spec.path] : [];
  if (spec.path && spec.untracked) return untrackedPatch(root, [spec.path]);
  if (spec.side === "staged") return ok(root, ["diff", "--cached", ...DIFF_FLAGS, ...paths]);
  if (spec.side === "changes") {
    const tracked = await ok(root, ["diff", ...DIFF_FLAGS, ...paths]);
    if (spec.path) return tracked;
    return tracked + (await untrackedPatch(root, await untracked(root)));
  }
  // Everything against HEAD (or, before the first commit, everything staged), and what is new.
  const born = (await run(root, ["rev-parse", "-q", "--verify", "HEAD"])).code === 0;
  const tracked = await ok(root, born ? ["diff", "HEAD", ...DIFF_FLAGS, ...paths] : ["diff", "--cached", ...DIFF_FLAGS, ...paths]);
  if (spec.path) return tracked;
  return tracked + (await untrackedPatch(root, await untracked(root)));
}

/** The untracked files git does not ignore. */
async function untracked(root: string): Promise<string[]> {
  const out = await ok(root, ["ls-files", "--others", "--exclude-standard", "-z"]);
  return out.split("\0").filter((path) => path !== "");
}

/** New files as additions: `git diff --no-index` against nothing, which exits 1 when they differ. */
async function untrackedPatch(root: string, paths: string[]): Promise<string> {
  const read = paths.slice(0, UNTRACKED_LIMIT);
  const parts = await Promise.all(
    read.map(async (path) => {
      const answer = await run(root, ["diff", "--no-index", ...DIFF_FLAGS, "--", "/dev/null", path]);
      return answer.code <= 1 ? answer.out : "";
    }),
  );
  return parts.join("");
}
