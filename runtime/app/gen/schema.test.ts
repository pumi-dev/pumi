import { expect, test } from "bun:test";
import { Types, type Schema } from "./schema.ts";
import { typesFile, type Export } from "./api.ts";
import { EXPORT, ROOT } from "./index.ts";
import { join } from "node:path";

/** A tree as the export writes one: written out in place, its branches a reference back to it. */
const tree: Schema = {
  type: "object",
  properties: {
    node: {
      anyOf: [{ type: "object", properties: { leaf: { type: "string" } }, required: ["leaf"] }, { type: "object", properties: { kids: { type: "array", items: { $ref: "#/$defs/Node" } } }, required: ["kids"] }],
    },
  },
  required: ["node"],
  $defs: {
    Node: { anyOf: [{ type: "object", properties: { leaf: { $ref: "#/$defs/Leaf" } }, required: ["leaf"] }, { type: "object", properties: { kids: { type: "array", items: { $ref: "#/$defs/Node" } } }, required: ["kids"] }] },
    Leaf: { type: "string" },
  },
};

test("a tree's reference back to itself is a named type, declared once", () => {
  const types = new Types();
  const local = types.defs(tree, "tree.get");
  expect(local).toEqual(new Map([["Node", "Node"]]));
  expect(types.ts(tree, local, "")).not.toContain("unknown");
  expect(types.ts(tree, local, "")).toContain("kids: Node[];");
  expect([...types.named.keys()]).toEqual(["Node"]);
  expect(types.named.get("Node")).toContain("leaf: string;");
  // Another function's same definition is the same type.
  expect(types.defs(structuredClone(tree), "tree.update")).toEqual(new Map([["Node", "Node"]]));
  expect(types.named.size).toBe(1);
});

test("a definition a caller declares is named as it says, and not declared again", () => {
  const types = new Types();
  types.claim(tree, "Node", "Tree");
  expect(types.defs(tree, "tree.get")).toEqual(new Map([["Node", "Tree"]]));
  expect(types.named.size).toBe(0);
});

test("the SDK's types leave no branch of a layout untyped", async () => {
  const api: Export = await Bun.file(join(ROOT, EXPORT)).json();
  const out = typesFile(api, "");
  expect(out).not.toMatch(/(first|second): unknown;/);
  expect(out).toContain("first: Layout;");
});
