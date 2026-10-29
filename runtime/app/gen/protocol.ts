// The UI protocol's TS types and names, from its export (api/ui-protocol.json, which
// `cargo xtask api` writes from crates/pumi-ui-proto): every type under `$defs` as a named type
// (`Op`, `Event`, `StyleDesc`, …), and the protocol's level and names as constants.
import { Types, doc, type Schema } from "./schema.ts";

export interface Protocol {
  level: number;
  builtInTypes: string[];
  elementEvents: string[];
  universalProps: string[];
  $defs: Record<string, Schema>;
}

/**
 * `schema` with what `Types.ts` reads one way spelled the way it reads: an object that is a
 * reference plus fields of its own (an internally tagged variant: `Press` and its `type`), or
 * fields plus a union (`Event`: its `target`, then one of its kinds), is the intersection.
 */
function normalize(schema: Schema): Schema {
  if (Array.isArray(schema)) return schema.map(normalize);
  if (!schema || typeof schema !== "object") return schema;
  const s: Record<string, Schema> = Object.fromEntries(Object.entries(schema).map(([k, v]) => [k, normalize(v)]));
  const { description, ...rest } = s;
  const own = (picked: Record<string, Schema>) => ({ type: "object", ...picked });
  if (typeof rest.$ref === "string" && rest.properties) {
    const { $ref, ...fields } = rest;
    return { description, allOf: [{ $ref }, own(fields)] };
  }
  const union = rest.oneOf ?? rest.anyOf;
  if (Array.isArray(union) && rest.properties) {
    const { oneOf: _o, anyOf: _a, ...fields } = rest;
    return { description, allOf: [own(fields), { oneOf: union }] };
  }
  return s;
}

const list = (names: string[]) => `[\n${names.map((n) => `  ${JSON.stringify(n)},\n`).join("")}] as const`;

/** runtime/app/native/protocol.gen.ts. */
export function protocolFile(protocol: Protocol, header: string): string {
  const types = new Types();
  const names = Object.keys(protocol.$defs).sort();
  const local = new Map(names.map((name) => [name, name]));
  let out = header;
  out += `/** The protocol's level: what \`pumi_ui_proto::LEVEL\` is. */\n`;
  out += `export const LEVEL = ${protocol.level};\n\n`;
  out += `/** The element types the renderer always has; every other type is a custom element. */\n`;
  out += `export const BUILT_IN_TYPES = ${list(protocol.builtInTypes)};\n\n`;
  out += `/** The events \`setEventListener\` can listen for on an element. */\n`;
  out += `export const ELEMENT_EVENTS = ${list(protocol.elementEvents)};\n`;
  out += `export type ElementEvent = (typeof ELEMENT_EVENTS)[number];\n\n`;
  out += `/** The custom props a built-in element reads. */\n`;
  out += `export const UNIVERSAL_PROPS = ${list(protocol.universalProps)};\n`;
  for (const name of names) {
    const body = normalize(protocol.$defs[name]);
    const desc = typeof body === "object" ? body.description : undefined;
    out += `\n${doc(desc)}export type ${name} = ${types.ts(body, local, "")};\n`;
  }
  return out;
}
