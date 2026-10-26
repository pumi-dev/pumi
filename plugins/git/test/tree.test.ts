// The tree of changes: what a group's rows are, in what order, as deep as the directories that are
// open, and the group states its checkboxes read.
import { describe, expect, test } from "bun:test";
import type { Change } from "../src/git.ts";
import { diffTotals, rows, stageStates, visible } from "../src/tree.ts";

function change(path: string, glyph = "M", color = "modified", actions = ["stage"]): Change {
  return { path, glyph, color, actions };
}

/** The text of each row, indented by its depth: the shape a reader sees. */
function shape(found: ReturnType<typeof rows>): string[] {
  return found.map((row) => `${"  ".repeat(row.depth)}${row.kind === "folder" ? `${row.text}/` : row.text}`);
}

describe("the tree", () => {
  test("directories come before files, each by name", () => {
    const found = rows("changes", [change("b.txt"), change("a/x.ts"), change("a/y.ts"), change("z.txt")], new Set(), false, null, new Map(), new Set());
    expect(shape(found)).toEqual(["a/", "  x.ts", "  y.ts", "b.txt", "z.txt"]);
  });

  test("a directory with one directory in it is one row", () => {
    const found = rows("changes", [change("src/deep/deeper/x.ts"), change("src/top.ts")], new Set(), false, null, new Map(), new Set());
    expect(shape(found)).toEqual(["src/", "  deep/deeper/", "    x.ts", "  top.ts"]);
  });

  test("a folded directory shows its row and nothing under it", () => {
    const folded = new Set(["changes:dir:src"]);
    const found = rows("changes", [change("src/a.ts"), change("src/b.ts"), change("top.ts")], folded, false, null, new Map(), new Set());
    expect(shape(found)).toEqual(["src/", "top.ts"]);
    expect(found[0]!.open).toBe(false);
  });

  test("a filter shows what it found, every directory open", () => {
    const folded = new Set(["changes:dir:src"]);
    const found = rows("changes", [change("src/deep/a.ts")], folded, true, null, new Map(), new Set());
    // `src/deep` is one row: a directory with nothing but a directory in it is folded together.
    expect(shape(found)).toEqual(["src/deep/", "  a.ts"]);
    expect(found.every((row) => row.kind !== "folder" || row.open)).toBe(true);
  });

  test("a contiguous directory is blue while it has changes in it", () => {
    const found = rows("changes", [change("src/a.ts"), change("empty")], new Set(), false, null, new Map(), new Set());
    expect(found[0]!.color).toBe("folderChanged");
  });

  test("a file's row says what git says of it, and what it can do", () => {
    const found = rows(
      "changes",
      [{ path: "a.txt", glyph: "?", color: "untracked", stats: [3, 1], actions: ["stage", "ignore"] }],
      new Set(),
      false,
      "changes:file:a.txt",
      new Map(),
      new Set(),
    );
    expect(found[0]).toMatchObject({
      id: "changes:file:a.txt",
      kind: "file",
      glyph: "?",
      color: "untracked",
      checkbox: "off",
      stats: "+3 -1",
      actions: ["stage", "ignore"],
      selected: true,
    });
  });

  test("a conflict's sides are a note under its row", () => {
    const conflicted: Change = {
      path: "f.txt",
      glyph: "U",
      color: "conflict",
      actions: ["stage", "ours", "theirs"],
      note: "UU · ours abc1234 · theirs def5678",
    };
    const found = rows("changes", [conflicted], new Set(), false, null, new Map(), new Set());
    expect(found.map((row) => [row.kind, row.text, row.depth])).toEqual([
      ["file", "f.txt", 0],
      ["note", "UU · ours abc1234 · theirs def5678", 1],
    ]);
    expect(found[0]!.actions).toEqual(["stage", "ours", "theirs"]);
  });

  test("a directory's checkbox says how much of it is staged", () => {
    const staged = [change("src/a.ts", "A", "added", ["unstage"])];
    const changes = [change("src/b.ts")];
    const { dirStage, both } = stageStates(staged, changes, []);
    expect(dirStage.get("src")).toBe("partial");
    expect([...both]).toEqual([]);
    const all = stageStates([change("src/a.ts", "A", "added", ["unstage"])], [], []);
    expect(all.dirStage.get("src")).toBe("on");
    const none = stageStates([], [change("src/a.ts")], []);
    expect(none.dirStage.get("src")).toBe("off");
  });

  test("a file staged and changed again is partial on both sides", () => {
    const path = "src/a.ts";
    const { both } = stageStates([change(path, "M", "modified", ["unstage"])], [change(path)], []);
    expect([...both]).toEqual([path]);
    const found = rows("staged", [change(path, "M", "modified", ["unstage"])], new Set(), false, null, new Map(), both);
    expect(found.find((row) => row.kind === "file")!.checkbox).toBe("partial");
  });

  test("the totals count a file once, whatever group it is in", () => {
    const staged: Change[] = [{ ...change("a.ts", "M", "modified", ["unstage"]), stats: [2, 0] }];
    const changes: Change[] = [{ ...change("a.ts"), stats: [2, 0] }, { ...change("b.ts"), stats: [0, 3] }, change("c.ts")];
    expect(diffTotals(staged, changes, [])).toEqual([2, 3]);
  });

  test("a filter keeps the paths that hold it", () => {
    const all = [change("src/main.ts"), change("README.md")];
    expect(visible(all, "").length).toBe(2);
    expect(visible(all, "MAIN").map((each) => each.path)).toEqual(["src/main.ts"]);
    expect(visible(all, "nope")).toEqual([]);
  });
});
