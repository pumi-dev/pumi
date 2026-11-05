import { expect, test } from "bun:test";
import { byLetter, fromSlot, kinds, SELF, type NewTab, type SlotAction } from "../src/kinds.ts";

const row = (id: string, letter: string, ran: unknown[] = [], value = `pumi_dev.${id}`): NewTab => ({
  id,
  title: id,
  icon: "plus",
  value,
  letter,
  open: async (at, place) => void ran.push([at, place]),
});

test("a file comes first, then the + menu's list in its order, with its letters", () => {
  const list = kinds([row("claude", "C"), row("terminal", "T"), row("pi", "P")], () => {});
  expect(list.map((k) => [k.id, k.letter, k.group])).toEqual([
    ["file", "F", "file"],
    ["claude", "C", "new"],
    ["terminal", "T", "new"],
    ["pi", "P", "new"],
  ]);
});

test("the list leaves itself out", () => {
  const list = kinds([row("launchpad", "N", [], SELF), row("pi", "P")], () => {});
  expect(list.map((k) => k.id)).toEqual(["file", "pi"]);
});

test("a key picks its row, any case, and opens it from the tab in the list's place", () => {
  const ran: unknown[] = [];
  const list = kinds([row("claude", "C", ran)], () => {});
  expect(byLetter(list, "f")?.id).toBe("file");
  expect(byLetter(list, "")).toBeUndefined();
  byLetter(list, "c")!.open("%4", { kind: "tiled", replace: "%4" });
  expect(ran).toEqual([["%4", { kind: "tiled", replace: "%4" }]]);
});

const action = (plugin: string, id: string, ran: unknown[]): SlotAction => ({ id, title: id, plugin, run: (args) => void ran.push(args) });

test("without useNewTabs, the slot's actions are numbered in order, and none past nine", () => {
  const ran: unknown[] = [];
  const list = fromSlot([action("pumi_dev/terminal", "open", ran), ...Array.from({ length: 9 }, (_, i) => action("p", `a${i}`, []))], () => {});
  expect(list.slice(0, 3).map((k) => [k.id, k.letter])).toEqual([
    ["file", "F"],
    ["pumi_dev/terminal/open", "1"],
    ["p/a0", "2"],
  ]);
  expect(list.at(-1)!.letter).toBe("");
  byLetter(list, "1")!.open("%4", { kind: "tiled", replace: "%4" });
  expect(ran).toEqual([{ tab: "%4", place: { kind: "tiled", replace: "%4" } }]);
});
