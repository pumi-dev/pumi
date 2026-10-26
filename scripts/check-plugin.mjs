// Check a plugin package against the export: its manifest, and every API name it calls.
//
//   bun scripts/check-plugin.mjs ./some-plugin ./other-plugin
//
// What it refuses: a manifest without `pumi.api = 1`, a main nothing runs, a component id that is
// not a lowercase word, a command without a title, and any `call("…")` naming a function
// `api/pumi-api.json` does not have. It reads source text, not types: a name built at run time is
// reported as unknown, honestly. Output goes to stderr, so it can sit inside a script safely.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, basename } from "node:path";

function read(path) {
  return readFileSync(path, "utf8");
}

function root() {
  try {
    return join(dirname(new URL(import.meta.url).pathname), "..");
  } catch (err) {
    console.error(`check: cannot find the pumi checkout: ${err.message}`);
    process.exit(2);
  }
}

function exportOf() {
  const path = join(root(), "api/pumi-api.json");
  try {
    return JSON.parse(read(path));
  } catch (err) {
    console.error(`check: ${path}: ${err.message}`);
    process.exit(2);
  }
}

function sources(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules") continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...sources(path));
    else if (entry.endsWith(".ts")) out.push(path);
  }
  return out;
}

const api = exportOf();
const known = new Set(api.functions.map((f) => f.name));
// `call("tab.open", […])` is checked. A handle's own method (`tab.call("report")`, a tab's
// type method) and the SDK's nouns (`tab.open(…)`) are not: those are not functions of the
// export, and the generated SDK is their authority.
const CALL = /(?<![.\w])call\(\s*"([a-zA-Z][\w.]*)"/g;

let bad = 0;
for (const dir of process.argv.slice(2)) {
  const name = basename(dir);
  const problems = [];
  let manifest;
  try {
    manifest = JSON.parse(read(join(dir, "package.json")));
  } catch (err) {
    console.error(`${name}: no readable package.json (${err.message})`);
    bad++;
    continue;
  }
  const p = manifest.pumi ?? {};
  if (p.api !== 1) problems.push(`pumi.api is ${JSON.stringify(p.api)}, not 1`);
  if (!p.name) problems.push("pumi.name missing");
  if (!manifest.main) problems.push("no `main`: nothing runs");
  if (!manifest.devDependencies?.["@pumid/types"]) problems.push("no @pumid/types devDependency");
  const contributes = p.contributes ?? {};
  for (const component of contributes.components ?? []) {
    if (!/^[a-z][a-z0-9_-]*$/.test(component.id ?? "")) problems.push(`component id ${JSON.stringify(component.id)} is not a lowercase word`);
  }
  for (const command of contributes.commands ?? []) {
    if (!command.id || !command.title) problems.push(`command ${JSON.stringify(command.id)} needs an id and a title`);
  }
  let files = [];
  try {
    files = sources(join(dir, "src"));
  } catch {
    problems.push("src/ is missing");
  }
  const unknown = new Map();
  for (const file of files) {
    for (const match of read(file).matchAll(CALL)) {
      if (!known.has(match[1])) unknown.set(match[1], file.slice(dir.length + 1));
    }
  }
  for (const [at, file] of unknown) problems.push(`${file} calls ${at}, which the export does not have`);
  const components = (contributes.components ?? []).length;
  const commands = (contributes.commands ?? []).length;
  const options = Object.keys(contributes.options ?? {}).length;
  const tally = `${files.length} files, ${components} component(s), ${commands} command(s), ${options} option(s)`;
  console.error(problems.length ? `${name}: ${problems.length} problem(s) — ${tally}\n  - ${problems.join("\n  - ")}` : `${name}: ok — ${tally}`);
  bad += problems.length;
}
process.exit(bad ? 1 : 0);
