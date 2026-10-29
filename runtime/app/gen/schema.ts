// JSON Schema (as the export's `params_schema` and `returns_schema` have it: schemars' draft 2020-12
// subset) to TypeScript types. The `$defs` a schema still refers to (a tree's branches) become
// named types, shared by every function whose schema has the same definition under the same name;
// a name two functions define differently gets the function's name as a suffix for the second. A string whose `pattern` is an id's is that id's type
// (`TabId`).

export type Schema = boolean | { [key: string]: any };

const IDENT = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
const key = (name: string) => (IDENT.test(name) ? name : JSON.stringify(name));

/** A doc comment of `text`, indented by `indent`; nothing for no text. */
export function doc(text: string | undefined, indent = ""): string {
  if (!text) return "";
  const lines = text.replaceAll("*/", "*\\/").split("\n");
  if (lines.length === 1) return `${indent}/** ${lines[0]} */\n`;
  return `${indent}/**\n${lines.map((l) => `${indent} *${l ? ` ${l}` : ""}`).join("\n")}\n${indent} */\n`;
}

/** The six JSON types: a schema that admits them all is any JSON value. */
const ANY_JSON = ["array", "boolean", "null", "number", "object", "string"];

export class Types {
  /** @param ids an id's type, by its `pattern` (`^(?:…/)?%[1-9][0-9]*$` is `TabId`). */
  constructor(private ids = new Map<string, string>()) {}
  /** Named types: name → declaration. */
  readonly named = new Map<string, string>();
  /** Each definition seen: name → [its JSON, the name it was given]. */
  private seen = new Map<string, [string, string][]>();

  /** The names of the definitions `schema` refers to outside its `$defs`. */
  refs(schema: Schema): Set<string> {
    const out = new Set<string>();
    const walk = (s: unknown) => {
      if (Array.isArray(s)) return s.forEach(walk);
      if (!s || typeof s !== "object") return;
      for (const [k, v] of Object.entries(s)) {
        if (k === "$ref" && typeof v === "string") out.add(v.replace("#/$defs/", ""));
        else if (k !== "$defs") walk(v);
      }
    };
    walk(schema);
    return out;
  }

  /**
   * Definition `name` of `root`'s `$defs`, with every other definition it refers to written out
   * in place, as the export writes a schema: only a reference back to one it is inside (a tree's
   * branch) stays one, and is added to `kept`.
   */
  body(root: Schema, name: string, kept = new Set<string>([name])): Schema {
    const defs: Record<string, Schema> = (typeof root === "object" && root.$defs) || {};
    const inline = (s: any, inside: string[]): any => {
      if (Array.isArray(s)) return s.map((v) => inline(v, inside));
      if (!s || typeof s !== "object") return s;
      if (typeof s.$ref === "string") {
        const ref = s.$ref.replace("#/$defs/", "");
        if (kept.has(ref) || inside.includes(ref) || !(ref in defs)) {
          if (ref in defs) kept.add(ref);
          return s;
        }
        const { $ref: _, ...rest } = s;
        return { ...inline(defs[ref], [...inside, ref]), ...rest };
      }
      return Object.fromEntries(Object.entries(s).map(([k, v]) => [k, inline(v, inside)]));
    };
    return inline(defs[name], [name]);
  }

  /**
   * Gives definition `ref` of `root` the name `given` from now on, which its caller declares: a
   * model's field type is the tree its schema refers back to (`Layout` is the export's
   * `LayoutTree`).
   */
  claim(root: Schema, ref: string, given: string) {
    const json = JSON.stringify(this.bodies(root).get(ref) ?? this.body(root, ref));
    this.seen.set(ref, [[json, given], ...(this.seen.get(ref) ?? []).filter(([j]) => j !== json)]);
    this.claimed.add(given);
  }
  private claimed = new Set<string>();

  /**
   * What each definition `root` refers to says, the others it uses written out; one a definition
   * refers back to while inside it is among them too.
   */
  private bodies(root: Schema): Map<string, Schema> {
    const kept = this.refs(root);
    const bodies = new Map<string, Schema>();
    for (let size = -1; size !== kept.size; ) {
      size = kept.size;
      for (const name of [...kept]) bodies.set(name, this.body(root, name, kept));
    }
    return bodies;
  }

  /**
   * The names the definitions `root` refers to get here, by their name in its `$defs`, each
   * declared once. The export writes every definition out in place but a tree's, so what is left
   * to name is the reference a tree makes to itself. `owner` (a function's name) is what tells two
   * different definitions of one name apart.
   */
  defs(root: Schema, owner: string): Map<string, string> {
    const local = new Map<string, string>();
    if (typeof root !== "object" || !root.$defs) return local;
    const bodies = this.bodies(root);
    for (const [name, body] of bodies) {
      const json = JSON.stringify(body);
      const known = this.seen.get(name) ?? [];
      const same = known.find(([j]) => j === json);
      if (same) {
        local.set(name, same[1]);
        continue;
      }
      const given = known.length === 0 ? name : `${name}_${owner.replace(/[^A-Za-z0-9]/g, "_")}`;
      known.push([json, given]);
      this.seen.set(name, known);
      local.set(name, given);
    }
    for (const [name, body] of bodies) {
      const given = local.get(name)!;
      if (this.named.has(given) || this.claimed.has(given)) continue;
      const desc = typeof body === "object" ? body.description : undefined;
      this.named.set(given, `${doc(desc)}export type ${given} = ${this.ts(body, local, "")};\n`);
    }
    return local;
  }

  /** The TypeScript type of `schema`; `$ref`s are resolved through `local`. */
  ts(schema: Schema | undefined, local: Map<string, string>, indent: string): string {
    if (schema === undefined || schema === true) return "unknown";
    if (schema === false) return "never";
    const s = schema;
    if (s.$ref) {
      const name = String(s.$ref).replace("#/$defs/", "");
      return local.get(name) ?? "unknown";
    }
    if ("const" in s) return JSON.stringify(s.const);
    if (Array.isArray(s.enum)) return s.enum.map((v: unknown) => JSON.stringify(v)).join(" | ");
    const union = s.anyOf ?? s.oneOf;
    if (Array.isArray(union)) {
      const base = { ...s };
      delete base.anyOf;
      delete base.oneOf;
      delete base.description;
      delete base.default;
      const parts = union.map((u: Schema) =>
        // `{ type: "object", oneOf: [ { properties … } ] }`: each part is the object it says.
        typeof u === "object" && base.type && !u.type ? this.ts({ ...base, ...u }, local, indent) : this.ts(u, local, indent),
      );
      return unique(parts).join(" | ");
    }
    if (Array.isArray(s.allOf)) {
      // `&` binds tighter than `|`: a part that is a union keeps its parentheses.
      const union = (p: Schema) => typeof p === "object" && Array.isArray(p.anyOf ?? p.oneOf) && !p.type;
      return s.allOf.map((p: Schema) => (union(p) ? `(${this.ts(p, local, indent)})` : this.ts(p, local, indent))).join(" & ");
    }
    const types: string[] = Array.isArray(s.type) ? s.type : s.type ? [s.type] : [];
    if (ANY_JSON.every((t) => types.includes(t))) return "unknown";
    if (types.length === 0) return s.properties ? this.object(s, local, indent) : "unknown";
    return unique(types.map((t) => this.one(t, s, local, indent))).join(" | ");
  }

  private one(type: string, s: { [key: string]: any }, local: Map<string, string>, indent: string): string {
    switch (type) {
      case "string":
        return (typeof s.pattern === "string" && this.ids.get(s.pattern)) || "string";
      case "integer":
      case "number":
        return "number";
      case "boolean":
        return "boolean";
      case "null":
        return "null";
      case "array": {
        if (Array.isArray(s.prefixItems)) return `[${s.prefixItems.map((p: Schema) => this.ts(p, local, indent)).join(", ")}]`;
        const item = this.ts(s.items, local, indent);
        return /^[A-Za-z0-9_.]+$/.test(item) ? `${item}[]` : `(${item})[]`;
      }
      case "object":
        return this.object(s, local, indent);
      default:
        return "unknown";
    }
  }

  private object(s: { [key: string]: any }, local: Map<string, string>, indent: string): string {
    const props: Record<string, Schema> = s.properties ?? {};
    const required = new Set<string>(s.required ?? []);
    const inner = `${indent}  `;
    const fields = Object.entries(props).map(([name, p]) => {
      const desc = typeof p === "object" ? p.description : undefined;
      const optional = required.has(name) ? "" : "?";
      return `${doc(desc, inner)}${inner}${key(name)}${optional}: ${this.ts(p, local, inner)};`;
    });
    const extra = s.additionalProperties;
    if (extra !== undefined && extra !== false) {
      const value = extra === true ? "unknown" : this.ts(extra, local, inner);
      // An index signature has to admit every named field's type too.
      const all = fields.length > 0 ? "unknown" : value;
      fields.push(`${inner}[key: string]: ${all};`);
    }
    if (fields.length === 0) return extra === false ? "{}" : "Record<string, unknown>";
    return `{\n${fields.join("\n")}\n${indent}}`;
  }
}

function unique(parts: string[]): string[] {
  const out = [...new Set(parts)];
  // `unknown` absorbs the rest.
  return out.includes("unknown") ? ["unknown"] : out;
}
