// The git work against a real repository: what a status says of a file, the two groups, the
// history, and each action the page can take — the same `git` binary a user has.
import { describe, expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import * as git from "../src/git.ts";
import { relevant } from "../src/watch.ts";
import { empty, repository, run, scratch, write } from "./repo.ts";

describe("what a repository has", () => {
  test("the root is found from a directory in it", async () => {
    const dir = repository({ "src/a.ts": "one\n" });
    expect(await git.repository(dir)).toBe(await git.repository(join(dir, "src")));
  });

  test("a directory that is not a repository says so", async () => {
    const dir = scratch("plain");
    await expect(git.repository(dir)).rejects.toThrow(/not a git repository/);
  });

  test("staged and unstaged are two sides of a file", async () => {
    const dir = repository({ "a.txt": "one\ntwo\n" });
    writeFileSync(join(dir, "a.txt"), "one\nTWO\nthree\n");
    let collected = await git.collect(dir);
    expect(collected.staged).toEqual([]);
    expect(collected.changes.map((each) => [each.path, each.glyph, each.color, each.stats])).toEqual([
      ["a.txt", "M", "modified", [2, 1]],
    ]);
    run(dir, "add", "--all");
    collected = await git.collect(dir);
    expect(collected.staged.map((each) => [each.path, each.glyph, each.color, each.stats, each.actions])).toEqual([
      ["a.txt", "M", "modified", [2, 1], ["unstage"]],
    ]);
    expect(collected.changes).toEqual([]);
  });

  test("an untracked file is a change that can be staged or ignored", async () => {
    const dir = repository();
    write(dir, { "new.txt": "hi\n" });
    const { changes } = await git.collect(dir);
    expect(changes.map((each) => [each.path, each.glyph, each.color, each.actions])).toEqual([
      ["new.txt", "?", "untracked", ["stage", "ignore"]],
    ]);
  });

  test("a staged and changed again file is both", async () => {
    const dir = repository({ "a.txt": "one\n" });
    writeFileSync(join(dir, "a.txt"), "one\ntwo\n");
    run(dir, "add", "--all");
    writeFileSync(join(dir, "a.txt"), "one\ntwo\nthree\n");
    const { staged, changes } = await git.collect(dir);
    expect(staged.map((each) => each.path)).toEqual(["a.txt"]);
    expect(changes.map((each) => each.path)).toEqual(["a.txt"]);
  });

  test("a rename is named as one, with the path it has now", async () => {
    const dir = repository({ "a.txt": "one\n" });
    run(dir, "mv", "a.txt", "b.txt");
    const { staged } = await git.collect(dir);
    expect(staged.map((each) => [each.path, each.glyph, each.color])).toEqual([["b.txt", "R", "renamed"]]);
  });

  test("a conflict says which sides it has", async () => {
    const dir = repository({ "f.txt": "base\n" });
    run(dir, "checkout", "--quiet", "-b", "other");
    writeFileSync(join(dir, "f.txt"), "theirs\n");
    run(dir, "commit", "--quiet", "--all", "--message", "theirs");
    run(dir, "checkout", "--quiet", "main");
    writeFileSync(join(dir, "f.txt"), "ours\n");
    run(dir, "commit", "--quiet", "--all", "--message", "ours");
    expect(() => run(dir, "merge", "other")).toThrow();
    const { changes } = await git.collect(dir);
    const conflicted = changes.find((each) => each.color === "conflict");
    expect(conflicted?.glyph).toBe("U");
    expect(conflicted?.actions).toEqual(["stage", "ours", "theirs"]);
    expect(conflicted?.note).toMatch(/^UU · ours [0-9a-f]{7} · theirs [0-9a-f]{7}$/);
    // Taking a side stages it, and there is no conflict left.
    await git.resolve(dir, "theirs", "f.txt");
    expect(readFileSync(join(dir, "f.txt"), "utf8")).toBe("theirs\n");
    expect((await git.collect(dir)).changes).toEqual([]);
    expect((await git.collect(dir)).staged.map((each) => each.path)).toEqual(["f.txt"]);
    // The version that side had is the index's now, and `HEAD` is still this branch: a change.
    expect((await git.collect(dir)).staged.map((each) => each.color)).toEqual(["modified"]);
  });

  test("the branch and the commit HEAD is at", async () => {
    const dir = repository();
    expect(await git.branch(dir)).toBe("main");
    expect(await git.headId(dir)).toMatch(/^[0-9a-f]{40}$/);
    expect(await git.branch(empty())).toBe("main");
    expect(await git.headId(empty())).toBe("");
  });
});

describe("the history", () => {
  test("newest first, with its author, its date and its tags", async () => {
    const dir = repository();
    writeFileSync(join(dir, "a.txt"), "one\ntwo\nthree\n");
    run(dir, "commit", "--quiet", "--all", "--message", "second\n\nwith a body");
    run(dir, "tag", "v1.0.0");
    const commits = await git.history(dir);
    expect(commits.map((each) => each.subject)).toEqual(["second", "first"]);
    expect(commits.map((each) => each.author)).toEqual(["A Tester", "A Tester"]);
    expect(commits[0]!.tags).toEqual(["v1.0.0"]);
    expect(commits[1]!.tags).toEqual([]);
    expect(commits[0]!.timestamp).toBeGreaterThan(0);
    // Nothing is upstream, so nothing has been pushed.
    expect(commits.map((each) => each.unpushed)).toEqual([false, false]);
  });

  test("a repository with no commit has an empty history", async () => {
    expect(await git.history(empty())).toEqual([]);
  });

  test("the commits the upstream does not have are marked", async () => {
    const dir = repository();
    const upstream = scratch("upstream");
    run(upstream, "init", "--quiet", "--bare");
    run(dir, "remote", "add", "origin", upstream);
    run(dir, "push", "--quiet", "--set-upstream", "origin", "main");
    writeFileSync(join(dir, "a.txt"), "one\ntwo\nthree\n");
    run(dir, "commit", "--quiet", "--all", "--message", "second");
    const commits = await git.history(dir);
    expect(commits.map((each) => [each.subject, each.unpushed])).toEqual([
      ["second", true],
      ["first", false],
    ]);
  });
});

describe("what the page can do", () => {
  test("stage takes a file, or a whole directory, in", async () => {
    const dir = repository({ "src/a.ts": "one\n" });
    write(dir, { "src/b.ts": "two\n", "top.txt": "three\n" });
    await git.stage(dir, "src");
    expect((await git.collect(dir)).staged.map((each) => each.path)).toEqual(["src/b.ts"]);
    await git.stageAll(dir);
    expect((await git.collect(dir)).staged.map((each) => each.path).sort()).toEqual(["src/b.ts", "top.txt"]);
    expect((await git.collect(dir)).changes).toEqual([]);
  });

  test("stage takes a deletion in too", async () => {
    const dir = repository({ "a.txt": "one\n", "b.txt": "two\n" });
    run(dir, "rm", "--quiet", "a.txt");
    await git.stageAll(dir);
    expect((await git.collect(dir)).staged.map((each) => [each.path, each.glyph])).toEqual([["a.txt", "D"]]);
  });

  test("unstage takes it back out, a file at a time or all of it", async () => {
    const dir = repository();
    write(dir, { "a.txt": "changed\n", "b.txt": "new\n" });
    await git.stageAll(dir);
    await git.unstage(dir, "a.txt");
    expect((await git.collect(dir)).staged.map((each) => each.path)).toEqual(["b.txt"]);
    await git.unstageAll(dir);
    expect((await git.collect(dir)).staged).toEqual([]);
  });

  test("unstage works before the first commit", async () => {
    const dir = empty();
    write(dir, { "a.txt": "one\n" });
    await git.stageAll(dir);
    await git.unstageAll(dir);
    expect((await git.collect(dir)).staged).toEqual([]);
    expect((await git.collect(dir)).changes.map((each) => each.path)).toEqual(["a.txt"]);
  });

  test("discard puts the work tree's file back as the index has it", async () => {
    const dir = repository({ "a.txt": "one\n" });
    writeFileSync(join(dir, "a.txt"), "one\ntwo\n");
    await git.discard(dir, "a.txt");
    expect(readFileSync(join(dir, "a.txt"), "utf8")).toBe("one\n");
  });

  test("ignore writes the one rule, and says nothing the second time", async () => {
    const dir = repository();
    write(dir, { "notes.log": "hi\n" });
    await git.ignore(dir, "notes.log");
    await git.ignore(dir, "notes.log");
    expect(readFileSync(join(dir, ".gitignore"), "utf8")).toBe("/notes.log\n");
  });

  test("commit takes what is staged, and answers the commit it made", async () => {
    const dir = repository();
    writeFileSync(join(dir, "a.txt"), "one\ntwo\nthree\n");
    await git.stageAll(dir);
    const sha = await git.commit(dir, "second");
    expect(sha).toBe(await git.headId(dir));
    expect(run(dir, "log", "-1", "--format=%s").trim()).toBe("second");
    expect((await git.collect(dir)).staged).toEqual([]);
  });

  test("a commit with nothing staged fails in git's own words", async () => {
    const dir = repository();
    await expect(git.commit(dir, "nothing")).rejects.toThrow(/nothing to commit|no changes/i);
  });

  test("the entries a status is read from: a path, its two columns, and a rename's origin", () => {
    expect(git.entries(" M a.txt\0?? b.txt\0R  c.txt\0c-old.txt\0")).toEqual([
      { path: "a.txt", index: " ", work: "M" },
      { path: "b.txt", index: "?", work: "?" },
      { path: "c.txt", index: "R", work: " ", from: "c-old.txt" },
    ]);
  });

  test("commit tracked takes every tracked change, staged or not, and leaves new files", async () => {
    const dir = repository();
    writeFileSync(join(dir, "a.txt"), "changed\n");
    write(dir, { "new.txt": "hi\n" });
    await git.commit(dir, "tracked", "tracked");
    const { staged, changes } = await git.collect(dir);
    expect(staged).toEqual([]);
    expect(changes.map((each) => each.path)).toEqual(["new.txt"]);
  });

  test("amend with no message keeps the last commit's", async () => {
    const dir = repository();
    writeFileSync(join(dir, "a.txt"), "changed\n");
    await git.stageAll(dir);
    await git.commit(dir, "", "amend");
    expect(run(dir, "log", "--format=%s").trim()).toBe("first");
  });

  test("publish pushes the branch and tracks it; then it is level with its upstream", async () => {
    const dir = repository();
    const upstream = scratch("upstream");
    run(upstream, "init", "--quiet", "--bare");
    expect(await git.remote(dir)).toEqual({ upstream: null, remote: null, ahead: 0, behind: 0 });
    run(dir, "remote", "add", "origin", upstream);
    expect((await git.remote(dir)).remote).toBe("origin");
    await git.sync(dir, "publish", "origin");
    expect(await git.remote(dir)).toEqual({ upstream: "origin/main", remote: "origin", ahead: 0, behind: 0 });
    writeFileSync(join(dir, "a.txt"), "changed\n");
    run(dir, "commit", "--quiet", "--all", "--message", "second");
    expect((await git.remote(dir)).ahead).toBe(1);
    await git.sync(dir, "push", "origin");
    expect((await git.remote(dir)).ahead).toBe(0);
  });

  test("the ignored directories are what a watch skips", async () => {
    const dir = repository({ ".gitignore": "target/\n", "a.txt": "one\n" });
    write(dir, { "target/debug/x.o": "bin" });
    const ignored = await git.ignoredDirs(dir);
    expect(ignored).toEqual(["target/"]);
    expect(relevant(["target/debug/x.o", "target"], ignored)).toBe(false);
    expect(relevant(["a.txt"], ignored)).toBe(true);
    expect(relevant([".git/objects/ab/cd"], ignored)).toBe(false);
    expect(relevant([".git/index"], ignored)).toBe(true);
  });

  test("a patch for each thing a diff tab shows", async () => {
    const dir = repository({ "a.txt": "one\n", "b.txt": "two\n" });
    writeFileSync(join(dir, "a.txt"), "one\nmore\n");
    run(dir, "add", "a.txt");
    writeFileSync(join(dir, "b.txt"), "TWO\n");
    write(dir, { "new.txt": "hi\n" });
    const staged = await git.patch(dir, { kind: "work", side: "staged" });
    expect(staged).toContain("+++ b/a.txt");
    expect(staged).not.toContain("b.txt\n+TWO");
    const changes = await git.patch(dir, { kind: "work", side: "changes" });
    expect(changes).toContain("+TWO");
    expect(changes).toContain("+++ b/new.txt");
    const all = await git.patch(dir, { kind: "work", side: "all" });
    expect(all).toContain("+more");
    expect(all).toContain("+TWO");
    expect(all).toContain("+hi");
    expect(await git.patch(dir, { kind: "work", side: "changes", path: "new.txt", untracked: true })).toContain("+hi");
    const head = await git.headId(dir);
    expect(await git.patch(dir, { kind: "commit", sha: head })).toContain("+++ b/a.txt");
  });
});
