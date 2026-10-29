// Writes the SDK's generated files from the server's export:
//
//   api/pumi-api.json  →  worker/sdk/api.gen.ts (function, event, tab method and model names),
//                         worker/sdk/api.types.gen.ts (every type, for tsc and editors),
//                         worker/sdk/models.gen.ts (the model classes)
//   api/ui-protocol.json → native/protocol.gen.ts (the UI protocol's types and names)
//
//   bun runtime/app/gen/index.ts           writes them
//   bun runtime/app/gen/index.ts --check   says which are stale, and fails
//
// `cargo xtask api` runs this after writing the export, so they follow it.
import { join, relative } from "node:path";
import { modelsFile, runtimeFile, typesFile, type Export } from "./api.ts";
import { protocolFile, type Protocol } from "./protocol.ts";

export const ROOT = join(import.meta.dir, "../../..");

/** The export the SDK is generated from. */
export const EXPORT = "api/pumi-api.json";
/** The UI protocol's export, which `protocol.gen.ts` is generated from. */
export const PROTOCOL = "api/ui-protocol.json";

const generated = (from: string) =>
  `// Generated from ${from} by \`bun runtime/app/gen/index.ts\` (\`cargo xtask api\` runs it); do not edit.\n\n`;
const header = generated(EXPORT);

/** Every generated file, by its path from the checkout, and what it should hold. */
export async function files(root = ROOT): Promise<Map<string, string>> {
  const exported: Export = await Bun.file(join(root, EXPORT)).json();
  const protocol: Protocol = await Bun.file(join(root, PROTOCOL)).json();
  return new Map([
    ["runtime/app/worker/sdk/api.gen.ts", runtimeFile(exported, header)],
    ["runtime/app/worker/sdk/api.types.gen.ts", typesFile(exported, header)],
    ["runtime/app/worker/sdk/models.gen.ts", modelsFile(exported, header)],
    ["runtime/app/native/protocol.gen.ts", protocolFile(protocol, generated(PROTOCOL))],
  ]);
}

/** The generated files that differ from what is on disk. */
export async function stale(root = ROOT): Promise<string[]> {
  const out: string[] = [];
  for (const [path, text] of await files(root)) {
    const file = Bun.file(join(root, path));
    if (!(await file.exists()) || (await file.text()) !== text) out.push(path);
  }
  return out;
}

if (import.meta.main) {
  if (process.argv.includes("--check")) {
    const old = await stale();
    if (old.length > 0) {
      console.error(`stale, run \`bun runtime/app/gen/index.ts\`: ${old.join(", ")}`);
      process.exit(1);
    }
    console.log("the SDK's generated files are current");
  } else {
    for (const [path, text] of await files()) {
      const full = join(ROOT, path);
      const file = Bun.file(full);
      if ((await file.exists()) && (await file.text()) === text) continue;
      await Bun.write(full, text);
      console.log(`wrote ${relative(process.cwd(), full)}`);
    }
  }
}
