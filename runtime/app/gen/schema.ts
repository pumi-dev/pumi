// JSON Schema (as the export's `params_schema` and `returns_schema` have it: schemars' draft 2020-12
// subset) to TypeScript types. `$defs` become named types, shared by every function whose schema
// has the same definition under the same name; a name two functions define differently gets the
// function's name as a suffix for the second. A string whose `pattern` is an id's is that id's type
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

  /**
   * The names `schema`'s `$defs` get here, by their name in it. `owner` (a function's name) is
   * what tells two different definitions of one name apart.
   */
  defs(schema: Schema, owner: string): Map<string, string> {
    const local = new Map<string, string>();
    if (typeof schema !== "object") return local;
    const defs: Record<string, Schema> = schema.$defs ?? {};
    // Names first, so definitions can refer to each other in any order.
    for (const [name, def] of Object.entries(defs)) {
      const json = JSON.stringify(def);
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
    for (const [name, def] of Object.entries(defs)) {
      const given = local.get(name)!;
      if (this.named.has(given)) continue;
      const desc = typeof def === "object" ? def.description : undefined;
      this.named.set(given, `${doc(desc)}export type ${given} = ${this.ts(def, local, "")};\n`);
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
    if (Array.isArray(s.allOf)) return s.allOf.map((p: Schema) => this.ts(p, local, indent)).join(" & ");
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
