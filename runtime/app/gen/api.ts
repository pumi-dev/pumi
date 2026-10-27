// The `pumi` module's bindings, from the server's export (api/pumi-api.json): every function's
// types and one object per noun, every event's data, and the model classes (`Session`, `View`,
// `Tab`, `Request`, `Plugin`) whose fields are getters over the live cache and whose methods are
// one wire call each, the tab methods (`read`, `input`, …) among `Tab`'s.
import { Types, doc, type Schema } from "./schema.ts";

export interface Example {
  args: unknown;
  result?: unknown;
  desc?: string;
}
export interface Fn {
  name: string;
  desc: string;
  parameters: [string, string][];
  params_schema: Schema;
  returns: string;
  returns_schema: Schema;
  /** Who answers it: `server`, or `client` (it needs a window). */
  runs_on?: "server" | "client" | "either";
  examples?: Example[];
}
export interface Ev {
  name: string;
  desc: string;
  match: string;
  data: string[];
  params_schema: Schema;
}
export interface Field {
  name: string;
  /** Its type as the export writes it (`TabId | null`, `Layout`). */
  type: string;
  desc: string;
  watch: boolean;
  schema: Schema;
}
export interface Method {
  name: string;
  /** The function it calls: the model's id goes first. */
  fn: string;
  /** For `tab.call`: the tab method it is. */
  method?: string;
}
export interface ModelSpec {
  name: string;
  kind: string;
  idPrefix: string;
  desc: string;
  fields: Field[];
  methods: Method[];
  events: string[];
}
export interface TabMethod {
  name: string;
  desc: string;
  params_schema: Schema;
  returns_schema: Schema;
  example?: Example;
}
export interface Trait {
  name: string;
  desc: string;
  fields: Field[];
}
export interface Export {
  functions: Fn[];
  events: Ev[];
  models: ModelSpec[];
  tabMethods: TabMethod[];
  traits: Trait[];
}

const IDENT = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
/** `name` as a property name. */
const prop = (name: string) => (IDENT.test(name) ? name : JSON.stringify(name));
const pascal = (name: string) => name.replace(/(^|[^A-Za-z0-9])([a-z])/g, (_, __, c: string) => c.toUpperCase());

/** `value` as a JavaScript literal, for an example: `{ title: "tests" }`. */
export function literal(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(literal).join(", ")}]`;
  if (value && typeof value === "object") {
    const entries = Object.entries(value).map(([k, v]) => `${prop(k)}: ${literal(v)}`);
    return entries.length ? `{ ${entries.join(", ")} }` : "{}";
  }
  return JSON.stringify(value) ?? "undefined";
}

/** A doc comment's text: `desc`, then an `@example` of `code` (and what it answers). */
function documented(desc: string, code: string | undefined, result?: unknown): string {
  if (code === undefined) return desc;
  const answer = result === undefined || result === null ? "" : ` // ${literal(result)}`;
  return `${desc}\n\n@example\n${code}${answer}`;
}

/** Words a parameter cannot be named in a module (strict mode), so they get a `_`. */
const RESERVED = new Set([
  "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else", "enum",
  "export", "extends", "false", "finally", "for", "function", "if", "implements", "import", "in", "instanceof",
  "interface", "let", "new", "null", "package", "private", "protected", "public", "return", "static", "super",
  "switch", "this", "throw", "true", "try", "typeof", "var", "void", "while", "with", "yield", "await",
]);
const safe = (name: string) => (RESERVED.has(name) ? `${name}_` : name);

/** Everything the three files are made from: the types, and what each part of the export is. */
class Built {
  readonly types: Types;
  /** An id's type, by its model's kind (`tab` → `TabId`). */
  readonly idOf = new Map<string, string>();
  /** A model's class, by its id's type (`TabId` → `Tab`). */
  readonly modelOf = new Map<string, string>();
  /** The named types the models' fields use (`Layout`, `TabPlace`), declared. */
  readonly named = new Map<string, string>();
  readonly fns = new Map<string, Fn>();
  readonly tabMethods = new Map<string, TabMethod>();

  constructor(readonly api: Export) {
    const ids = new Map<string, string>();
    for (const m of api.models) {
      const pattern = this.schemaOf(m.fields.find((f) => f.name === "id")?.schema)?.pattern;
      const id = `${m.name}Id`;
      if (typeof pattern === "string") ids.set(pattern, id);
      this.idOf.set(m.kind, id);
      this.modelOf.set(id, m.name);
    }
    for (const f of api.functions) this.fns.set(f.name, f);
    for (const p of api.tabMethods) this.tabMethods.set(p.name, p);
    // Option scopes take any object's id (`%3`, `@2`, `$1`, `+7`).
    const any = JSON.stringify(api).match(/"pattern":"(\^\(\?:\[A-Za-z0-9\._-\]\+\/\)\?\[\$@%\+\][^"]*)"/);
    if (any) ids.set(JSON.parse(`"${any[1]}"`), "ObjectId");
    this.types = new Types(ids);
    this.declareNamed();
  }

  private schemaOf(s: Schema | undefined): Exclude<Schema, boolean> | undefined {
    return s && typeof s === "object" ? s : undefined;
  }

  /** A field's schema as `<kind>.get` answers it, which is more exact than the model's. */
  private fieldSchema(model: ModelSpec | undefined, field: Field): Schema {
    const got = model && this.schemaOf(this.fns.get(`${model.kind}.get`)?.returns_schema)?.properties?.[field.name];
    return got ?? field.schema;
  }

  /** `schema` without its `null` (for `X | null`) or its array (for `X[]`). */
  private bare(schema: Schema, type: string): Schema {
    let s = this.schemaOf(schema) ?? {};
    if (type.endsWith(" | null") && Array.isArray(s.anyOf)) s = s.anyOf.find((p: any) => p?.type !== "null") ?? s;
    if (type.endsWith("[]") && s.type === "array") s = s.items ?? {};
    return s;
  }

  private declareNamed() {
    const fields: [ModelSpec | undefined, Field][] = [
      ...this.api.models.flatMap((m) => m.fields.map((f): [ModelSpec, Field] => [m, f])),
      ...this.api.traits.flatMap((t) => t.fields.map((f): [undefined, Field] => [undefined, f])),
    ];
    for (const [model, field] of fields) {
      const name = field.type.replace(/ \| null$/, "").replace(/\[\]$/, "");
      if (!/^[A-Z][A-Za-z]*$/.test(name) || this.modelOf.has(name) || name === "ObjectId" || this.named.has(name)) continue;
      const schema = this.bare(this.fieldSchema(model, field), field.type);
      const root = this.rootOf(model);
      // A tree refers back to itself by its definition (each side of a split is a layout): when
      // that definition is the field's whole type, it is this type.
      for (const ref of this.types.refs(schema)) {
        const self = new Map([[ref, name]]);
        if (this.types.ts(this.types.body(root, ref), self, "") === this.types.ts(schema, self, "")) this.types.claim(root, ref, name);
      }
      const ts = this.types.ts(schema, this.types.defs(root, `${model?.kind}.get`), "");
      this.named.set(name, `export type ${name} = ${ts};\n`);
    }
  }

  /** The schema a model's fields are read from (`<kind>.get`'s answer), with its `$defs`. */
  private rootOf(model: ModelSpec | undefined): Schema {
    return (model && this.fns.get(`${model.kind}.get`)?.returns_schema) || {};
  }

  /** A field's type as data: ids stay ids. */
  fieldType(model: ModelSpec | undefined, field: Field): string {
    if (field.type === "Record<string, object>") return "TabTraits";
    const tokens = field.type.match(/[A-Z][A-Za-z]*/g) ?? [];
    if (tokens.some((t) => this.named.has(t) || this.modelOf.has(t) || t === "ObjectId")) return field.type;
    return this.types.ts(this.fieldSchema(model, field), this.types.defs(this.rootOf(model), `${model?.kind}.get`), "  ");
  }

  /** A field's type as a model's getter answers it: an id is its model (`ViewId[]` is `View[]`). */
  getterType(model: ModelSpec, field: Field): { type: string; kind?: string; many?: boolean } {
    const m = field.type.match(/^([A-Z][A-Za-z]*Id)(\[\]| \| null)?$/);
    const cls = m && field.name !== "id" ? this.modelOf.get(m[1]!) : undefined;
    if (!m || !cls) return { type: this.fieldType(model, field) };
    const kind = this.api.models.find((x) => x.name === cls)!.kind;
    return { type: `${cls}${m[2] ?? ""}`, kind, many: m[2] === "[]" };
  }

  params(f: Fn): { name: string; type: string; optional: boolean }[] {
    const schema = this.schemaOf(f.params_schema) ?? {};
    const props: Record<string, Schema> = schema.properties ?? {};
    const required = new Set<string>(schema.required ?? []);
    const local = this.types.defs(schema, f.name);
    const list = f.parameters.map(([, name]) => ({ name, type: this.types.ts(props[name], local, "  "), optional: !required.has(name) }));
    // Only a trailing run of optional parameters is optional in TypeScript; an earlier one takes
    // `undefined`.
    let trailing = true;
    for (let i = list.length - 1; i >= 0; i--) {
      const p = list[i]!;
      if (!p.optional) trailing = false;
      else if (!trailing) p.type = `${p.type} | undefined`;
      p.optional &&= trailing;
    }
    return list;
  }

  returns(f: Fn): string {
    return this.of(f.returns_schema, f.name);
  }

  /** The TypeScript type of `schema`, a whole one of `owner`'s (its `$defs` beside it). */
  of(schema: Schema, owner: string, indent = "  "): string {
    return this.types.ts(schema, this.types.defs(schema, owner), indent);
  }
}

const signature = (ps: { name: string; type: string; optional: boolean }[]) =>
  ps.map((p) => `${safe(p.name)}${p.optional ? "?" : ""}: ${p.type}`).join(", ");

/** An example's arguments as a call's: positional ones as they are. */
const callArgs = (args: unknown) => (Array.isArray(args) ? args.map(literal).join(", ") : literal(args));

/** The noun tree: `tab.open` is `tab.open(…)`; `call` is apart. */
function nouns(api: Export): Map<string, Fn[]> {
  const out = new Map<string, Fn[]>();
  for (const f of api.functions) {
    const [noun, verb, more] = f.name.split(".");
    if (!verb) continue;
    if (more) throw new Error(`${f.name}: not noun.verb`);
    out.set(noun!, [...(out.get(noun!) ?? []), f]);
  }
  return out;
}

/** worker/sdk/api.types.gen.ts: the types, read by tsc and editors only. */
export function typesFile(api: Export, header: string): string {
  const b = new Built(api);
  let out = header;
  out += "// Ids are prefixed strings: `$1` a session, `@2` a view, `%3` a tab, `+7` a request, `owner/name` a\n";
  out += "// plugin, and `devbox/%3` one on the server `devbox`.\n\n";
  for (const m of api.models) out += `${doc(`${m.name}'s id (\`${m.idPrefix ? `${m.idPrefix}1` : "owner/name"}\`).`)}export type ${m.name}Id = string;\n`;
  out += `${doc("Any object's id, for an option's scope.")}export type ObjectId = string;\n\n`;
  for (const decl of b.named.values()) out += `${decl}\n`;
  // The definitions a schema below still refers to are named as they come up: they go here.
  const at = out.length;

  for (const t of api.traits) {
    out += doc(`${t.desc} A tab has it while its option \`@${t.name}\` is set; its fields are \`@${t.name}.<field>\`.`);
    out += `export interface ${pascal(t.name)}Trait {\n`;
    for (const f of t.fields) out += `${doc(f.desc, "  ")}  ${prop(f.name)}: ${b.fieldType(undefined, f)};\n`;
    out += "}\n";
  }
  out += "/** A tab's traits, each with its fields. */\nexport interface TabTraits {\n";
  for (const t of api.traits) out += `  ${prop(t.name)}?: ${pascal(t.name)}Trait;\n`;
  out += "  [trait: string]: unknown;\n}\n\n";

  for (const m of api.models) {
    out += doc(`What a ${m.name} is, as data: ${m.desc[0]!.toLowerCase()}${m.desc.slice(1)}`);
    out += `export interface ${m.name}Data {\n`;
    for (const f of m.fields) out += `${doc(f.desc, "  ")}  ${prop(f.name)}: ${b.fieldType(m, f)};\n`;
    out += "}\n";
    const keys = m.fields.filter((f) => f.watch).map((f) => JSON.stringify(f.name));
    out += `${doc(`What a ${m.name} can be watched for.`)}export type ${m.name}Key = ${keys.join(" | ") || "never"};\n\n`;
  }

  out += "/** Every API function: its arguments, in order, and what it answers. */\n";
  out += "export interface Functions {\n";
  for (const f of api.functions) out += `${doc(f.desc, "  ")}  ${JSON.stringify(f.name)}: { args: [${signature(b.params(f))}]; returns: ${b.returns(f)} };\n`;
  out += "}\n\n";

  out += "/** Every method a tab answers through `tab.call`: its arguments and its answer. */\n";
  out += "export interface TabMethods {\n";
  for (const p of api.tabMethods) {
    out += `${doc(p.desc, "  ")}  ${prop(p.name)}: { args: ${b.of(p.params_schema, p.name)}; returns: ${b.of(p.returns_schema, p.name)} };\n`;
  }
  out += "}\n\n";

  out += "/** Every event the core and the host fire, and its `data`. */\n";
  out += "export interface Events {\n";
  for (const e of api.events) {
    const text = `${e.desc}${e.match ? `\n\nIts \`match\` is ${e.match}.` : ""}`;
    out += `${doc(text, "  ")}  ${JSON.stringify(e.name)}: ${b.of(e.params_schema, e.name)};\n`;
  }
  out += "}\n\n";

  out += "/** One object per noun of the export, a method per function: each is one wire call. */\n";
  out += "export interface Nouns {\n";
  for (const [noun, fns] of nouns(api)) {
    out += `  ${prop(noun)}: {\n`;
    for (const f of fns) {
      const ex = f.examples?.[0];
      out += doc(documented(f.desc, ex && `await ${f.name}(${callArgs(ex.args)})`, ex?.result), "    ");
      out += `    ${prop(f.name.split(".")[1]!)}(${signature(b.params(f))}): Promise<${b.returns(f)}>;\n`;
    }
    out += "  };\n";
  }
  out += "}\n\n";
  out += "export type EventName = keyof Events;\nexport type FunctionName = keyof Functions;\nexport type TabMethodName = keyof TabMethods;\n";
  const defs = [...b.types.named.values()].map((decl) => `${decl}\n`).join("");
  return out.slice(0, at) + defs + out.slice(at);
}

/** worker/sdk/models.gen.ts: the model classes. */
export function modelsFile(api: Export, header: string): string {
  const b = new Built(api);
  const typeNames = new Set<string>(["TabTraits", ...b.named.keys(), "ObjectId"]);
  for (const m of api.models) typeNames.add(`${m.name}Id`).add(`${m.name}Data`).add(`${m.name}Key`);
  let out = "";
  for (const m of api.models) {
    const self = m.name[0]!.toLowerCase() + m.name.slice(1);
    out += doc(`${m.desc} Its fields read the live cache; each method is one call.`);
    out += `export class ${m.name} extends Model<${m.name}Data, ${m.name}Key> {\n`;
    out += `  static readonly kind = ${JSON.stringify(m.kind)};\n`;
    for (const f of m.fields) {
      if (f.name === "id") continue;
      const g = b.getterType(m, f);
      const read = `this.field(${JSON.stringify(f.name)})`;
      const body = g.kind ? `this.${g.many ? "models" : "model"}(${JSON.stringify(g.kind)}, ${read})` : read;
      out += `${doc(f.desc, "  ")}  get ${prop(f.name)}(): ${g.type} {\n    return ${body} as ${g.type};\n  }\n`;
    }
    for (const method of m.methods) {
      if (method.method) {
        const p = b.tabMethods.get(method.method);
        if (!p) throw new Error(`${m.name}.${method.name}: no tab method ${method.method}`);
        const schema: any = p.params_schema;
        const optional = !schema?.oneOf && !(schema?.required?.length > 0);
        const args = b.of(p.params_schema, p.name);
        const ex = p.example && `await ${self}.${method.name}(${literal(p.example.args)})`;
        out += doc(documented(p.desc, ex, p.example?.result), "  ");
        out += `  ${prop(method.name)}(args${optional ? "?" : ""}: ${args}): Promise<${b.of(p.returns_schema, p.name)}> {\n`;
        out += `    return this.invoke("tab.call", [this.id, ${JSON.stringify(method.method)}, args${optional ? " ?? {}" : ""}]);\n  }\n`;
        continue;
      }
      const f = b.fns.get(method.fn);
      if (!f) throw new Error(`${m.name}.${method.name}: no function ${method.fn}`);
      const ps = b.params(f);
      if (ps[0]?.name !== m.kind) throw new Error(`${method.fn}: its first parameter is not the ${m.kind}`);
      const rest = ps.slice(1);
      const ex = f.examples?.[0];
      const code = ex && Array.isArray(ex.args) ? `await ${self}.${method.name}(${callArgs(ex.args.slice(1))})` : undefined;
      out += doc(documented(f.desc, code, ex?.result), "  ");
      out += `  ${prop(method.name)}(${signature(rest)}): Promise<${b.returns(f)}> {\n`;
      out += `    return this.invoke(${JSON.stringify(f.name)}, [this.id${rest.map((p) => `, ${safe(p.name)}`).join("")}]);\n  }\n`;
    }
    out += "}\n\n";
  }
  out += "/** Each model's class, by its kind. */\n";
  out += `export const MODEL_CLASSES = { ${api.models.map((m) => `${prop(m.kind)}: ${m.name}`).join(", ")} } as const;\n\n`;
  // The cache makes a model by its kind, so the classes give themselves to it as they load.
  out += "installClasses(MODEL_CLASSES);\n";
  // The definitions the methods' schemas named, as they came up.
  for (const name of b.types.named.keys()) typeNames.add(name);
  const imports = `import { Model, installClasses } from "./model.ts";\nimport type { ${[...typeNames].sort().join(", ")} } from "./api.types.gen.ts";\n\n`;
  return header + imports + out;
}

/** worker/sdk/api.gen.ts: what the SDK builds its nouns, models and cache from at run time. */
export function runtimeFile(api: Export, header: string): string {
  let out = header;
  out += "/** Every function, by name. */\n";
  out += `export const FUNCTIONS = ${JSON.stringify(api.functions.map((f) => f.name))} as const;\n\n`;
  out += "/** The events the core and the host fire. */\n";
  out += `export const EVENTS = ${JSON.stringify(api.events.map((e) => e.name))} as const;\n\n`;
  out += "/** The methods every tab answers through `tab.call`. */\n";
  out += `export const TAB_METHODS = ${JSON.stringify(api.tabMethods.map((p) => p.name))} as const;\n\n`;
  out += "/** Each model, by its kind: its id's prefix, its fields, which of them can be watched, its events. */\n";
  out += "export const MODELS = {\n";
  for (const m of api.models) {
    const spec = { name: m.name, prefix: m.idPrefix, fields: m.fields.map((f) => f.name), watch: m.fields.filter((f) => f.watch).map((f) => f.name), events: m.events };
    out += `  ${prop(m.kind)}: ${JSON.stringify(spec)},\n`;
  }
  out += "} as const;\n";
  return out;
}
